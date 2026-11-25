//! Wiederherstellen (#57 Teil 2, RFC-0012 „Restoreauswahl und Wirkung“, „Ausfallsicherheit und Vergleich“).
//!
//! 1. **Vorschau** (`preview`): Stand im Sicherungsziel vollständig prüfen (Manifest, Prüfsummen, Integrität der
//!    Datenbank, Originale gegen Datenbank), mit dem aktuellen Datenstand vergleichen (älter, gleich, neuer, nicht
//!    vergleichbar über den Revisionsverlauf) und ein Token ausgeben, das Stand und aktuellen Datenstand bindet.
//! 2. **Bereitstellen** (`run`): Stand ins Staging kopieren und dort prüfen, ältere Schemas migrieren, Sicherungsstatus
//!    und Vorgänge der laufenden Datenbank übernehmen, neue Revision setzen. Dann eine Merker-Datei schreiben und den
//!    internen Neustart anfordern (Uli 25.11.2026).
//! 3. **Umschalten** (`apply_pending`, beim Start vor dem Öffnen der Datenbank) in nummerierten Schritten, jeder
//!    wiederholbar: alte Datenbank und Originale nach `vor-wiederherstellung/`, neue an ihren Platz. Scheitert danach
//!    der Start oder die Funktionsprüfung, wird zurückgenommen (`rollback`); ein Neustart mitten im Umschalten setzt
//!    anhand des Schritts fort oder nimmt zurück, nie bleibt eine Mischung.
//!
//! Der alte Bestand bleibt in `database_dir/vor-wiederherstellung/` bis zum nächsten Wiederherstellen liegen.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::{ConnectOptions, Connection, Row, SqliteConnection};

use super::Runner;
use super::format::{self, Manifest};
use crate::audit::{self, Action};
use crate::config::Validated;
use crate::db::{self, Db, SCHEMA_VERSION};

/// Merker-Datei im Datenbankordner; solange sie existiert, ist ein Wiederherstellen nicht abgeschlossen.
pub const MARKER: &str = "wiederherstellung.json";
/// Alter Bestand nach dem Umschalten (nur der letzte).
pub const ROLLBACK: &str = "vor-wiederherstellung";
const DB_NAMES: [&str; 3] = ["trackcraft.db", "trackcraft.db-wal", "trackcraft.db-shm"];

pub const ERROR_FAILED: &str = "restore_failed";
pub const ERROR_STALE: &str = "restore_preview_stale";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Comparison {
    Older,
    Same,
    Newer,
    NotComparable,
}

#[derive(Debug, Clone, Serialize)]
pub struct Current {
    pub revision_id: Option<String>,
    pub activity_count: i64,
    pub segment_count: i64,
    pub is_empty: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Notice {
    pub code: &'static str,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct Preview {
    pub token: String,
    pub stand: format::StandInfo,
    pub current: Current,
    pub comparison: Comparison,
    pub requires_acknowledgement: bool,
    pub warnings: Vec<Notice>,
}

#[derive(Debug)]
pub enum RestoreError {
    NotFound,
    /// Stand beschädigt oder unvollständig
    Invalid(String),
    /// Format oder Schema neuer als dieses Programm
    Incompatible(String),
    /// Token passt nicht (mehr) zu Stand und aktuellem Datenstand
    Stale,
    AcknowledgementRequired,
    Other(anyhow::Error),
}

impl From<anyhow::Error> for RestoreError {
    fn from(e: anyhow::Error) -> Self {
        Self::Other(e)
    }
}

impl From<sqlx::Error> for RestoreError {
    fn from(e: sqlx::Error) -> Self {
        Self::Other(e.into())
    }
}

/// Ordner eines Stands im Sicherungsziel; nur Namen aus der Liste, nie ein Pfad (RFC-0012: kein beliebiger Pfad).
fn stand_dir(cfg: &Validated, backup_id: &str) -> Result<PathBuf, RestoreError> {
    if !format::is_stand_name(backup_id) {
        return Err(RestoreError::NotFound);
    }
    let dir = cfg.raw.backup.target_dir.join(backup_id);
    if !dir.join(format::MANIFEST).is_file() {
        return Err(RestoreError::NotFound);
    }
    Ok(dir)
}

fn read_manifest(dir: &Path) -> Result<(Manifest, String), RestoreError> {
    let bytes = fs::read(dir.join(format::MANIFEST)).map_err(|e| RestoreError::Invalid(format!("Manifest unlesbar: {e}")))?;
    let m = format::read_manifest(dir).map_err(|e| {
        let text = format!("{e:#}");
        if text.contains("neuer als") { RestoreError::Incompatible(text) } else { RestoreError::Invalid(text) }
    })?;
    if m.schema_version > SCHEMA_VERSION {
        return Err(RestoreError::Incompatible(format!(
            "Der Stand hat Schemaversion {}, dieses Programm kennt nur bis {SCHEMA_VERSION}; erst TrackCraft aktualisieren",
            m.schema_version
        )));
    }
    let sha: String = Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect();
    Ok((m, sha))
}

/// Öffnet eine Datenbankdatei nur lesend und ohne jede Änderung an ihr (auch keine WAL-Dateien).
async fn open_immutable(file: &Path) -> sqlx::Result<SqliteConnection> {
    SqliteConnectOptions::new().filename(file).read_only(true).immutable(true).connect().await
}

async fn table_exists(conn: &mut SqliteConnection, name: &str) -> sqlx::Result<bool> {
    let n: i64 =
        sqlx::query_scalar("SELECT count(*) FROM sqlite_schema WHERE type = 'table' AND name = ?").bind(name).fetch_one(conn).await?;
    Ok(n > 0)
}

/// Integrität der gesicherten Datenbank und Abgleich der Originale mit dem Manifest.
async fn check_database(dir: &Path, m: &Manifest) -> Result<(), RestoreError> {
    let mut conn =
        open_immutable(&dir.join(format::DB_FILE)).await.map_err(|e| RestoreError::Invalid(format!("Datenbank unlesbar: {e}")))?;
    let result = async {
        let check: String = sqlx::query_scalar("PRAGMA integrity_check").fetch_one(&mut conn).await?;
        if check != "ok" {
            return Err(RestoreError::Invalid(format!("Datenbank beschädigt: {check}")));
        }
        if table_exists(&mut conn, "original_files").await? {
            let rels: Vec<String> = sqlx::query_scalar("SELECT relative_path FROM original_files").fetch_all(&mut conn).await?;
            for rel in rels {
                let listed = m.files.iter().any(|f| f.path == format!("{}/{rel}", format::ORIGINALS));
                if !listed && !m.missing_originals.contains(&rel) {
                    return Err(RestoreError::Invalid(format!("Original {rel} fehlt im Stand")));
                }
            }
        }
        Ok(())
    }
    .await;
    let _ = conn.close().await;
    result
}

/// Revisionen im Verlauf der gesicherten Datenbank (leer, wenn sie noch keinen hat).
async fn backup_history_contains(dir: &Path, revision: &str) -> sqlx::Result<bool> {
    let mut conn = open_immutable(&dir.join(format::DB_FILE)).await?;
    let found = if table_exists(&mut conn, "revision_history").await? {
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM revision_history WHERE revision_id = ?")
            .bind(revision)
            .fetch_one(&mut conn)
            .await?
            > 0
    } else {
        false
    };
    let _ = conn.close().await;
    Ok(found)
}

async fn current(db: &Db) -> anyhow::Result<(Current, Option<String>)> {
    let state = sqlx::query("SELECT dataset_id, revision_id FROM data_state WHERE id = 1").fetch_optional(&db.reader).await?;
    let activity_count: i64 = sqlx::query_scalar("SELECT count(*) FROM activities").fetch_one(&db.reader).await?;
    let segment_count: i64 = sqlx::query_scalar("SELECT count(*) FROM segments").fetch_one(&db.reader).await?;
    let plan: i64 = sqlx::query_scalar("SELECT count(*) FROM planned_units").fetch_one(&db.reader).await.unwrap_or(0);
    Ok((
        Current {
            revision_id: state.as_ref().map(|r| r.get("revision_id")),
            activity_count,
            segment_count,
            is_empty: activity_count == 0 && segment_count == 0 && plan == 0,
        },
        state.map(|r| r.get("dataset_id")),
    ))
}

fn token(backup_id: &str, manifest_sha: &str, revision: Option<&str>) -> String {
    let h: String = Sha256::digest(format!("{backup_id}|{manifest_sha}|{}", revision.unwrap_or("-")).as_bytes())
        .iter()
        .take(16)
        .map(|b| format!("{b:02x}"))
        .collect();
    format!("{backup_id}.{h}")
}

/// Vorschau. `verify` prüft zusätzlich alle Prüfsummen und die Datenbank (dauert bei großen Ständen).
pub async fn preview(cfg: &Validated, db: &Db, backup_id: &str, verify: bool) -> Result<Preview, RestoreError> {
    let dir = stand_dir(cfg, backup_id)?;
    let (m, manifest_sha) = read_manifest(&dir)?;
    if verify {
        let (d, mm) = (dir.clone(), m.clone());
        tokio::task::spawn_blocking(move || format::verify(&d, &mm))
            .await
            .map_err(|e| RestoreError::Other(e.into()))?
            .map_err(|e| RestoreError::Invalid(format!("{e:#}")))?;
        check_database(&dir, &m).await?;
    }

    let (current, dataset) = current(db).await?;
    let backup_state = m.data_state.as_ref();
    let comparison = match (backup_state, current.revision_id.as_deref()) {
        (Some(b), Some(now)) if b.revision_id == now => Comparison::Same,
        (Some(b), Some(now)) if dataset.as_deref() == Some(b.dataset_id.as_str()) => {
            let older: i64 = sqlx::query_scalar("SELECT count(*) FROM revision_history WHERE revision_id = ?")
                .bind(&b.revision_id)
                .fetch_one(&db.reader)
                .await?;
            if older > 0 {
                Comparison::Older
            } else if backup_history_contains(&dir, now).await? {
                Comparison::Newer
            } else {
                Comparison::NotComparable
            }
        }
        _ => Comparison::NotComparable,
    };

    let mut warnings = vec![];
    match comparison {
        Comparison::Older => warnings.push(Notice {
            code: "restore_older",
            message: "Der Stand ist älter als die aktuellen Daten; spätere Änderungen gehen verloren.".into(),
        }),
        Comparison::NotComparable if !current.is_empty => warnings.push(Notice {
            code: "restore_not_comparable",
            message: "Der Stand lässt sich nicht mit den aktuellen Daten vergleichen; sie werden vollständig ersetzt.".into(),
        }),
        _ => {}
    }
    if m.schema_version < SCHEMA_VERSION {
        warnings.push(Notice {
            code: "restore_schema_migration",
            message: format!(
                "Der Stand hat Schemaversion {} und wird beim Wiederherstellen auf {SCHEMA_VERSION} migriert.",
                m.schema_version
            ),
        });
    }
    if !m.missing_originals.is_empty() {
        warnings.push(Notice {
            code: "restore_missing_originals",
            message: format!("{} Originaldateien fehlten schon beim Sichern.", m.missing_originals.len()),
        });
    }
    if m.parameters_hash != format::parameters(cfg) {
        warnings.push(Notice {
            code: "restore_recalculation",
            message: "Die Rechenparameter weichen ab; Segmentversuche werden nach dem Wiederherstellen neu berechnet.".into(),
        });
    }
    let requires_acknowledgement = !current.is_empty && matches!(comparison, Comparison::Older | Comparison::NotComparable);
    let stand = format::StandInfo { name: backup_id.to_string(), manifest: Some(m), problem: None };
    Ok(Preview {
        token: token(backup_id, &manifest_sha, current.revision_id.as_deref()),
        stand,
        current,
        comparison,
        requires_acknowledgement,
        warnings,
    })
}

/// Prüft Token und Zustimmung ohne die teure Vollprüfung; liefert die Stand-ID.
pub async fn check_request(cfg: &Validated, db: &Db, preview_token: &str, acknowledged: bool) -> Result<String, RestoreError> {
    let Some((backup_id, _)) = preview_token.rsplit_once('.') else {
        return Err(RestoreError::Stale);
    };
    let p =
        preview(cfg, db, backup_id, false).await.map_err(|e| if matches!(e, RestoreError::NotFound) { RestoreError::Stale } else { e })?;
    if p.token != preview_token {
        return Err(RestoreError::Stale);
    }
    if p.requires_acknowledgement && !acknowledged {
        return Err(RestoreError::AcknowledgementRequired);
    }
    Ok(backup_id.to_string())
}

// ---------------------------------------------------------------------------------------------------------
// Bereitstellen

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Marker {
    operation_id: String,
    backup_id: String,
    /// Ordnername im Staging
    staged: String,
    /// Revision der laufenden Datenbank beim Bereitstellen; weicht sie beim Umschalten ab, wird abgebrochen
    expected_revision: Option<String>,
    /// 0 bereitgestellt, 1 alte DB weg, 2 alte Originale weg, 3 neue Originale da, 4 neue DB da, 5 umgeschaltet
    step: u8,
}

fn marker_path(cfg: &Validated) -> PathBuf {
    cfg.raw.paths.database_dir.join(MARKER)
}

fn write_marker(cfg: &Validated, m: &Marker) -> anyhow::Result<()> {
    let p = marker_path(cfg);
    let tmp = p.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_vec_pretty(m)?)?;
    fs::rename(&tmp, &p).context("Merker der Wiederherstellung lässt sich nicht schreiben")?;
    Ok(())
}

fn read_marker(cfg: &Validated) -> anyhow::Result<Option<Marker>> {
    match fs::read(marker_path(cfg)) {
        Ok(b) => Ok(Some(serde_json::from_slice(&b).context("Merker der Wiederherstellung unlesbar")?)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

async fn set_phase(db: &Db, id: &str, phase: &str) {
    if let Err(e) = sqlx::query("UPDATE operations SET phase = ? WHERE id = ?").bind(phase).bind(id).execute(&db.writer).await {
        tracing::warn!(error = %e, "Phase des Wiederherstellens lässt sich nicht speichern");
    }
}

async fn fail(db: &Db, id: &str, code: &str, message: &str, status: &str) {
    let r = sqlx::query(
        "UPDATE operations SET status = ?, phase = NULL, outcome = 'failed', error_code = ?, error_message = ?, finished_at_ms = ? WHERE id = ?",
    )
    .bind(status)
    .bind(code)
    .bind(message)
    .bind(chrono::Utc::now().timestamp_millis())
    .bind(id)
    .execute(&db.writer)
    .await;
    if let Err(e) = r {
        tracing::error!(error = %e, "Ergebnis des Wiederherstellens lässt sich nicht speichern");
    }
}

/// Legt den Vorgang an und startet das Bereitstellen im Hintergrund; `None`, wenn schon ein Vorgang läuft.
pub async fn start(
    cfg: std::sync::Arc<Validated>,
    db: Db,
    runner: std::sync::Arc<Runner>,
    backup_id: String,
) -> anyhow::Result<Option<String>> {
    let id = db::new_id();
    if !runner.try_begin(&id) {
        return Ok(None);
    }
    let inserted = sqlx::query(
        "INSERT INTO operations (id, kind, trigger, status, phase, backup_id, started_at_ms) VALUES (?, 'restore', 'manual', 'running', 'checking', ?, ?)",
    )
    .bind(&id)
    .bind(&backup_id)
    .bind(chrono::Utc::now().timestamp_millis())
    .execute(&db.writer)
    .await;
    if let Err(e) = inserted {
        runner.end();
        return Err(e.into());
    }
    let op = id.clone();
    tokio::spawn(async move {
        match run(&cfg, &db, &op, &backup_id).await {
            Ok(()) => {
                tracing::info!(backup_id, "Wiederherstellen bereitgestellt, interner Neustart");
                // Der Vorgang bleibt belegt bis zum Neustart; danach gibt es einen neuen Runner.
                runner.request_restart();
            }
            Err(e) => {
                let msg = format!("{e:#}");
                tracing::error!(error = msg, backup_id, "Wiederherstellen fehlgeschlagen, nichts geändert");
                let _ = fs::remove_dir_all(cfg.staging_dir().join(staged_name(&op)));
                let _ = fs::remove_dir_all(cfg.staging_dir().join(format!("{}{}", staged_name(&op), format::PARTIAL)));
                fail(&db, &op, ERROR_FAILED, &msg, "failed").await;
                runner.end();
            }
        }
    });
    Ok(Some(id))
}

fn staged_name(op: &str) -> String {
    format!("wiederherstellung-{op}")
}

async fn run(cfg: &Validated, db: &Db, op: &str, backup_id: &str) -> anyhow::Result<()> {
    // Vollprüfung im Sicherungsziel
    let p = preview(cfg, db, backup_id, true).await.map_err(|e| match e {
        RestoreError::Other(e) => e,
        other => anyhow::anyhow!("{}", describe(&other)),
    })?;
    let expected_revision = p.current.revision_id.clone();

    // Ins Staging kopieren und dort erneut prüfen
    set_phase(db, op, "staging").await;
    let name = staged_name(op);
    let (src, root, n) = (cfg.raw.backup.target_dir.join(backup_id), cfg.staging_dir(), name.clone());
    let staged = tokio::task::spawn_blocking(move || format::copy_stand(&src, &root, &n)).await??;

    // Datenbank vorbereiten: migrieren, Status und Vorgänge der laufenden DB übernehmen, neue Revision
    set_phase(db, op, "migrating").await;
    prepare_staged_db(db, &staged.join(format::DB_FILE), op, backup_id).await?;

    // Zwischenzeitliche Änderung? Dann abbrechen statt Neues zu verlieren.
    let (now, _) = current(db).await?;
    if now.revision_id != expected_revision {
        bail!("Die Daten haben sich seit der Vorschau geändert; bitte erneut prüfen");
    }
    set_phase(db, op, "switching").await;
    write_marker(
        cfg,
        &Marker { operation_id: op.to_string(), backup_id: backup_id.to_string(), staged: name, expected_revision, step: 0 },
    )?;
    Ok(())
}

async fn prepare_staged_db(current: &Db, file: &Path, op: &str, backup_id: &str) -> anyhow::Result<()> {
    // Zustand der laufenden Datenbank, der erhalten bleiben soll
    let state = sqlx::query("SELECT * FROM backup_state WHERE id = 1").fetch_one(&current.reader).await?;
    let ops = sqlx::query("SELECT * FROM operations").fetch_all(&current.reader).await?;

    let staged = Db::open(file).await?;
    let result = async {
        let pending = staged.pending_migrations().await?;
        if !pending.is_empty() {
            staged.migrate(&pending).await?;
        }
        let check: String = sqlx::query_scalar("PRAGMA integrity_check").fetch_one(&staged.writer).await?;
        if check != "ok" {
            bail!("Datenbank nach der Migration beschädigt: {check}");
        }
        let now = chrono::Utc::now().timestamp_millis();
        let mut tx = staged.writer.begin().await?;
        sqlx::query(
            "UPDATE backup_state SET last_success_at_ms = ?, last_success_name = ?, last_success_revision_id = ?, \
             last_success_counter = ?, last_success_parameters = ?, daily_last_date = ? WHERE id = 1",
        )
        .bind(state.get::<Option<i64>, _>("last_success_at_ms"))
        .bind(state.get::<Option<String>, _>("last_success_name"))
        .bind(state.get::<Option<String>, _>("last_success_revision_id"))
        .bind(state.get::<Option<i64>, _>("last_success_counter"))
        .bind(state.get::<Option<String>, _>("last_success_parameters"))
        .bind(state.get::<Option<String>, _>("daily_last_date"))
        .execute(&mut *tx)
        .await?;
        sqlx::query("DELETE FROM operations").execute(&mut *tx).await?;
        for r in &ops {
            sqlx::query(
                "INSERT INTO operations (id, kind, trigger, status, phase, outcome, error_code, error_message, backup_id, \
                 started_at_ms, finished_at_ms) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(r.get::<String, _>("id"))
            .bind(r.get::<String, _>("kind"))
            .bind(r.get::<String, _>("trigger"))
            .bind(r.get::<String, _>("status"))
            .bind(r.get::<Option<String>, _>("phase"))
            .bind(r.get::<Option<String>, _>("outcome"))
            .bind(r.get::<Option<String>, _>("error_code"))
            .bind(r.get::<Option<String>, _>("error_message"))
            .bind(r.get::<Option<String>, _>("backup_id"))
            .bind(r.get::<i64, _>("started_at_ms"))
            .bind(r.get::<Option<i64>, _>("finished_at_ms"))
            .execute(&mut *tx)
            .await?;
        }
        // Neue eindeutige Revision vom Sicherungsstand aus (RFC-0012)
        db::bump_revision(&mut tx, now).await?;
        audit::record(&mut tx, Action::Restore, "backup", Some(backup_id), true, json!({ "operation_id": op })).await?;
        tx.commit().await?;
        Ok(())
    }
    .await;
    staged.close().await;
    result
}

// ---------------------------------------------------------------------------------------------------------
// Umschalten beim Start

/// Ergebnis von [`apply_pending`].
#[derive(Debug)]
pub enum Pending {
    /// Neue Daten sind an ihrem Platz; Start und Funktionsprüfung müssen noch gelingen.
    Switched { operation_id: String, backup_id: String },
    /// Nicht umgeschaltet (z. B. Daten zwischenzeitlich geändert); alter Bestand unverändert.
    Aborted { operation_id: String, code: &'static str, message: String },
}

/// Verschiebt alle Einträge eines Ordners in einen anderen (gleiches Laufwerk). Bereits verschobene fehlen in `from`.
fn move_entries(from: &Path, to: &Path) -> anyhow::Result<()> {
    if !from.is_dir() {
        return Ok(());
    }
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let dst = to.join(entry.file_name());
        if dst.exists() {
            // Gleicher Name heißt gleicher Inhalt (Original nach SHA-256 benannt) oder ein Rest eines früheren Laufs.
            if dst.is_dir() { fs::remove_dir_all(&dst)? } else { fs::remove_file(&dst)? }
        }
        fs::rename(entry.path(), &dst).with_context(|| format!("{} lässt sich nicht verschieben", entry.path().display()))?;
    }
    Ok(())
}

fn move_db_files(from: &Path, to: &Path) -> anyhow::Result<()> {
    fs::create_dir_all(to)?;
    for n in DB_NAMES {
        let src = from.join(n);
        if src.exists() {
            let dst = to.join(n);
            if dst.exists() {
                fs::remove_file(&dst)?;
            }
            fs::rename(&src, &dst).with_context(|| format!("{} lässt sich nicht verschieben", src.display()))?;
        }
    }
    Ok(())
}

async fn current_revision_of(file: &Path) -> anyhow::Result<Option<String>> {
    if !file.exists() {
        return Ok(None);
    }
    let mut conn = SqliteConnectOptions::new().filename(file).read_only(true).connect().await?;
    let r: Option<String> =
        sqlx::query_scalar("SELECT revision_id FROM data_state WHERE id = 1").fetch_optional(&mut conn).await.ok().flatten();
    let _ = conn.close().await;
    Ok(r)
}

/// Beim Start vor dem Öffnen der Datenbank: ein bereitgestelltes Wiederherstellen umschalten bzw. fortsetzen.
pub async fn apply_pending(cfg: &Validated) -> anyhow::Result<Option<Pending>> {
    let Some(mut m) = read_marker(cfg)? else {
        return Ok(None);
    };
    let db_dir = cfg.raw.paths.database_dir.clone();
    let staged = cfg.staging_dir().join(&m.staged);
    if m.step == 0 {
        let now = current_revision_of(&cfg.database_file()).await?;
        if now != m.expected_revision || !staged.join(format::DB_FILE).is_file() {
            let _ = fs::remove_dir_all(&staged);
            fs::remove_file(marker_path(cfg))?;
            return Ok(Some(Pending::Aborted {
                operation_id: m.operation_id,
                code: ERROR_STALE,
                message: "Die Daten haben sich vor dem Umschalten geändert; nichts ersetzt".into(),
            }));
        }
        let rollback = db_dir.join(ROLLBACK);
        if rollback.exists() {
            fs::remove_dir_all(&rollback).context("Alter Ordner vor-wiederherstellung lässt sich nicht entfernen")?;
        }
        fs::create_dir_all(rollback.join("database"))?;
        fs::create_dir_all(rollback.join("activities"))?;
        m.step = 1;
        write_marker(cfg, &m)?;
    }
    let rollback = db_dir.join(ROLLBACK);
    let activities = cfg.raw.paths.activities_dir.clone();
    while m.step < 5 {
        match m.step {
            1 => move_db_files(&db_dir, &rollback.join("database"))?,
            2 => move_entries(&activities, &rollback.join("activities"))?,
            3 => move_entries(&staged.join(format::ORIGINALS), &activities)?,
            4 => move_db_files(&staged, &db_dir)?,
            _ => unreachable!(),
        }
        m.step += 1;
        write_marker(cfg, &m)?;
    }
    tracing::info!(backup_id = m.backup_id, "Wiederherstellen umgeschaltet, alter Bestand in vor-wiederherstellung");
    Ok(Some(Pending::Switched { operation_id: m.operation_id, backup_id: m.backup_id }))
}

/// Nimmt ein (auch teilweises) Umschalten zurück: neue Dateien zurück ins Staging, alter Bestand an seinen Platz.
pub fn rollback(cfg: &Validated) -> anyhow::Result<Option<String>> {
    let Some(m) = read_marker(cfg)? else {
        return Ok(None);
    };
    let db_dir = cfg.raw.paths.database_dir.clone();
    let staged = cfg.staging_dir().join(&m.staged);
    let rollback = db_dir.join(ROLLBACK);
    let activities = cfg.raw.paths.activities_dir.clone();
    if m.step >= 4 {
        move_db_files(&db_dir, &staged)?;
    }
    if m.step >= 3 {
        move_entries(&activities, &staged.join(format::ORIGINALS))?;
    }
    if m.step >= 1 {
        move_entries(&rollback.join("activities"), &activities)?;
        move_db_files(&rollback.join("database"), &db_dir)?;
    }
    let _ = fs::remove_dir_all(&staged);
    fs::remove_file(marker_path(cfg))?;
    tracing::warn!(backup_id = m.backup_id, "Wiederherstellen zurückgenommen, alter Bestand wieder aktiv");
    Ok(Some(m.operation_id))
}

/// Funktionsprüfung nach dem Umschalten (in der neuen Datenbank, nach Migrationen und Start).
pub async fn verify_switched(cfg: &Validated, db: &Db, backup_id: &str) -> anyhow::Result<()> {
    let check: String = sqlx::query_scalar("PRAGMA quick_check").fetch_one(&db.reader).await?;
    if check != "ok" {
        bail!("Datenbank nach dem Umschalten nicht in Ordnung: {check}");
    }
    let m = read_marker(cfg)?.context("Merker fehlt")?;
    let manifest = format::read_manifest(&cfg.staging_dir().join(&m.staged))?;
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM activities").fetch_one(&db.reader).await?;
    if n != manifest.activity_count {
        bail!("{n} Aktivitäten statt {} aus dem Stand {backup_id}", manifest.activity_count);
    }
    let missing = sqlx::query_scalar::<_, String>("SELECT relative_path FROM original_files").fetch_all(&db.reader).await?;
    let absent =
        missing.iter().filter(|r| !cfg.raw.paths.activities_dir.join(r).is_file() && !manifest.missing_originals.contains(r)).count();
    if absent > 0 {
        bail!("{absent} Originaldateien fehlen nach dem Umschalten");
    }
    Ok(())
}

/// Abschluss nach erfolgreichem Start: Vorgang als erledigt vermerken, Merker und Staging entfernen.
pub async fn finish(cfg: &Validated, db: &Db, operation_id: &str) -> anyhow::Result<()> {
    let m = read_marker(cfg)?;
    sqlx::query(
        "UPDATE operations SET status = 'done', phase = NULL, outcome = 'restored', error_code = NULL, error_message = NULL, \
         finished_at_ms = ? WHERE id = ?",
    )
    .bind(chrono::Utc::now().timestamp_millis())
    .bind(operation_id)
    .execute(&db.writer)
    .await?;
    fs::remove_file(marker_path(cfg))?;
    if let Some(m) = m {
        let _ = fs::remove_dir_all(cfg.staging_dir().join(&m.staged));
    }
    Ok(())
}

/// Vorgang in der (alten) Datenbank als zurückgenommen bzw. abgebrochen vermerken.
pub async fn record_failure(db: &Db, operation_id: &str, code: &str, message: &str, rolled_back: bool) {
    fail(db, operation_id, code, message, if rolled_back { "rolled_back" } else { "failed" }).await;
    let mut conn = match db.writer.acquire().await {
        Ok(c) => c,
        Err(_) => return,
    };
    let _ =
        audit::record(&mut conn, Action::Restore, "backup", None, false, json!({ "operation_id": operation_id, "error": message })).await;
}

pub fn describe(e: &RestoreError) -> String {
    match e {
        RestoreError::NotFound => "Stand nicht gefunden".into(),
        RestoreError::Invalid(m) | RestoreError::Incompatible(m) => m.clone(),
        RestoreError::Stale => "Die Daten haben sich seit der Vorschau geändert".into(),
        RestoreError::AcknowledgementRequired => "Ausdrückliche Zustimmung nötig".into(),
        RestoreError::Other(e) => format!("{e:#}"),
    }
}

/// Nach bestandener Funktionsprüfung wird der bereitgestellte Ordner nicht mehr gebraucht (Rücknahme legt ihn neu an).
pub fn clear_staged(cfg: &Validated) -> anyhow::Result<()> {
    if let Some(m) = read_marker(cfg)? {
        let dir = cfg.staging_dir().join(&m.staged);
        if dir.exists() {
            fs::remove_dir_all(&dir).with_context(|| format!("{} lässt sich nicht entfernen", dir.display()))?;
        }
    }
    Ok(())
}
