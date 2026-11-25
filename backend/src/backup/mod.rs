//! Sicherung (#57, ADR-0014, RFC-0012): täglicher Versuch zur konfigurierten Uhrzeit, „Jetzt sichern“ aus der
//! Oberfläche und die Sicherung vor Migration. Es läuft immer höchstens ein Vorgang.
//!
//! Ein Versuch sichert nur, wenn das Ziel vorhanden und beschreibbar ist und sich seit der letzten erfolgreichen
//! Sicherung etwas geändert hat (Revision in `data_state` oder Parameter der Neuberechnung). Der Stand entsteht im
//! Staging (gleiches Laufwerk, harte Links auf die Originale), wird ins Ziel kopiert, dort geprüft und erst dann als
//! Erfolg vermerkt; danach greift die Aufbewahrung. Jeder Versuch ist ein Vorgang in `operations`.

pub mod format;
pub mod restore;

use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Context;
use serde_json::json;
use sqlx::Row;
use tokio::sync::Notify;

use crate::audit::{self, Action};
use crate::config::Validated;
use crate::db::{Db, new_id};

/// Abstand, in dem der Planer nachsieht.
const TICK: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    Daily,
    Manual,
}

impl Trigger {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Daily => "daily",
            Self::Manual => "manual",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Created { name: String },
    Unchanged,
    TargetMissing(String),
    Failed(String),
}

impl Outcome {
    /// `BackupOutcome` im Vertrag
    pub fn code(&self) -> &'static str {
        match self {
            Self::Created { .. } => "created",
            Self::Unchanged => "skipped_no_changes",
            Self::TargetMissing(_) => "skipped_target_unavailable",
            Self::Failed(_) => "failed",
        }
    }
}

/// Fehlercodes eines fehlgeschlagenen bzw. abgebrochenen Vorgangs.
pub const ERROR_FAILED: &str = "backup_failed";
pub const ERROR_INTERRUPTED: &str = "operation_interrupted";

/// Aufbewahrung beendeter Vorgänge, wie das Audit-Protokoll ein Jahr.
const OPERATION_RETENTION_MS: i64 = crate::audit::RETENTION_MS;

/// Gemeinsamer Zustand von Planer und API (nur im Speicher). Es gibt höchstens einen angeforderten oder laufenden Vorgang.
#[derive(Default)]
pub struct Runner {
    running: AtomicBool,
    /// angefordert („Jetzt sichern“), vom Planer noch nicht abgeschlossen
    queued: Mutex<Option<String>>,
    /// laufender Vorgang
    current: Mutex<Option<String>>,
    wake: Notify,
    restart: Notify,
    restart_requested: AtomicBool,
}

impl Runner {
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Angeforderter oder laufender Vorgang.
    pub fn active_operation(&self) -> Option<String> {
        let current = self.current.lock().ok().and_then(|g| g.clone());
        current.or_else(|| self.queued.lock().ok().and_then(|g| g.clone()))
    }

    /// „Jetzt sichern“ anfordern: legt den Vorgang als `queued` an und weckt den Planer.
    /// `None`, wenn schon einer angefordert ist oder läuft.
    pub async fn request(&self, db: &Db) -> anyhow::Result<Option<String>> {
        let id = new_id();
        {
            let mut q = self.queued.lock().map_err(|_| anyhow::anyhow!("Sperre vergiftet"))?;
            if q.is_some() || self.is_running() {
                return Ok(None);
            }
            *q = Some(id.clone());
        }
        // Der Planer kann den Vorgang schon übernommen haben; dann bleibt seine Zeile.
        sqlx::query(
            "INSERT OR IGNORE INTO operations (id, kind, trigger, status, started_at_ms) VALUES (?, 'backup', 'manual', 'queued', ?)",
        )
        .bind(&id)
        .bind(chrono::Utc::now().timestamp_millis())
        .execute(&db.writer)
        .await?;
        self.wake.notify_one();
        Ok(Some(id))
    }

    /// Führt einen Versuch aus, wenn keiner läuft (Planer, Tests). Liefert Vorgangs-ID und Ergebnis.
    pub async fn attempt(&self, cfg: &Validated, db: &Db, trigger: Trigger) -> Option<(String, Outcome)> {
        self.attempt_with(cfg, db, trigger, None).await
    }

    async fn attempt_with(&self, cfg: &Validated, db: &Db, trigger: Trigger, id: Option<String>) -> Option<(String, Outcome)> {
        if self.running.swap(true, Ordering::SeqCst) {
            return None;
        }
        let id = id.unwrap_or_else(new_id);
        if let Ok(mut g) = self.current.lock() {
            *g = Some(id.clone());
        }
        let outcome = attempt(cfg, db, trigger, &id).await;
        if let Ok(mut g) = self.current.lock() {
            *g = None;
        }
        self.running.store(false, Ordering::SeqCst);
        Some((id, outcome))
    }

    /// Belegt den Runner für einen anderen Vorgang (Wiederherstellen); `false`, wenn schon einer angefordert ist oder läuft.
    pub fn try_begin(&self, id: &str) -> bool {
        let Ok(q) = self.queued.lock() else {
            return false;
        };
        if q.is_some() || self.running.swap(true, Ordering::SeqCst) {
            return false;
        }
        if let Ok(mut g) = self.current.lock() {
            *g = Some(id.to_string());
        }
        true
    }

    /// Gibt den mit [`Runner::try_begin`] belegten Runner wieder frei.
    pub fn end(&self) {
        if let Ok(mut g) = self.current.lock() {
            *g = None;
        }
        self.running.store(false, Ordering::SeqCst);
    }

    /// Fordert den internen Neustart an (Server und Datenbank herunterfahren, Start erneut durchlaufen).
    pub fn request_restart(&self) {
        self.restart_requested.store(true, Ordering::SeqCst);
        self.restart.notify_one();
    }

    /// Wartet, bis ein interner Neustart angefordert ist.
    pub async fn restart_requested(&self) {
        while !self.restart_requested.load(Ordering::SeqCst) {
            self.restart.notified().await;
        }
    }
}

/// Gespeicherter Sicherungsstatus (Migration 21).
#[derive(Debug, Clone, Default)]
pub struct State {
    pub last_success_at_ms: Option<i64>,
    pub last_success_name: Option<String>,
    pub last_success_revision_id: Option<String>,
    pub last_success_counter: Option<i64>,
    pub last_success_parameters: Option<String>,
    pub daily_last_date: Option<String>,
    /// In der Oberfläche gesetzte Uhrzeit; `None` = `[backup] daily_time`
    pub daily_time: Option<String>,
    /// In der Oberfläche gesetzte Aufbewahrung; `None` = fünf
    pub keep: Option<i64>,
}

impl State {
    /// Wirksame Uhrzeit des täglichen Versuchs.
    pub fn daily_time(&self, cfg: &Validated) -> String {
        self.daily_time.clone().unwrap_or_else(|| cfg.raw.backup.daily_time.clone())
    }

    /// Wirksame Zahl aufbewahrter Stände.
    pub fn keep(&self) -> usize {
        self.keep.map_or(format::KEEP, |k| k.clamp(1, format::KEEP_MAX) as usize)
    }
}

pub async fn load_state(db: &Db) -> anyhow::Result<State> {
    let r = sqlx::query("SELECT * FROM backup_state WHERE id = 1").fetch_one(&db.reader).await?;
    Ok(State {
        last_success_at_ms: r.get("last_success_at_ms"),
        last_success_name: r.get("last_success_name"),
        last_success_revision_id: r.get("last_success_revision_id"),
        last_success_counter: r.get("last_success_counter"),
        last_success_parameters: r.get("last_success_parameters"),
        daily_last_date: r.get("daily_last_date"),
        daily_time: r.get("daily_time"),
        keep: r.get("keep"),
    })
}

/// Änderung der Einstellungen; äußeres `None` = unverändert, inneres `None` = zurück auf die Vorgabe.
#[derive(Debug, Default)]
pub struct SettingsChange {
    pub daily_time: Option<Option<String>>,
    pub keep: Option<Option<i64>>,
}

pub async fn update_settings(db: &Db, change: &SettingsChange) -> anyhow::Result<()> {
    let mut tx = db.writer.begin().await?;
    if let Some(t) = &change.daily_time {
        sqlx::query("UPDATE backup_state SET daily_time = ? WHERE id = 1").bind(t).execute(&mut *tx).await?;
    }
    if let Some(k) = change.keep {
        sqlx::query("UPDATE backup_state SET keep = ? WHERE id = 1").bind(k).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Ein Vorgang (`Operation` im Vertrag).
#[derive(Debug, Clone)]
pub struct Operation {
    pub id: String,
    pub kind: String,
    pub trigger: String,
    pub status: String,
    pub phase: Option<String>,
    pub outcome: Option<String>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub backup_id: Option<String>,
    pub started_at_ms: i64,
    pub finished_at_ms: Option<i64>,
}

fn operation_from(r: &sqlx::sqlite::SqliteRow) -> Operation {
    Operation {
        id: r.get("id"),
        kind: r.get("kind"),
        trigger: r.get("trigger"),
        status: r.get("status"),
        phase: r.get("phase"),
        outcome: r.get("outcome"),
        error_code: r.get("error_code"),
        error_message: r.get("error_message"),
        backup_id: r.get("backup_id"),
        started_at_ms: r.get("started_at_ms"),
        finished_at_ms: r.get("finished_at_ms"),
    }
}

pub async fn operation(db: &Db, id: &str) -> anyhow::Result<Option<Operation>> {
    let r = sqlx::query("SELECT * FROM operations WHERE id = ?").bind(id).fetch_optional(&db.reader).await?;
    Ok(r.as_ref().map(operation_from))
}

/// Zuletzt beendeter Sicherungsvorgang (letzter Prüfversuch).
pub async fn last_attempt(db: &Db) -> anyhow::Result<Option<Operation>> {
    let r = sqlx::query(
        "SELECT * FROM operations WHERE kind = 'backup' AND finished_at_ms IS NOT NULL ORDER BY finished_at_ms DESC, id DESC LIMIT 1",
    )
    .fetch_optional(&db.reader)
    .await?;
    Ok(r.as_ref().map(operation_from))
}

/// Beim Start: offene Vorgänge stammen aus einem abgebrochenen Lauf und enden als fehlgeschlagen.
pub async fn recover(db: &Db, keep: Option<&str>) -> anyhow::Result<u64> {
    let r = sqlx::query(
        "UPDATE operations SET status = 'failed', outcome = 'failed', phase = NULL, error_code = ?, \
         error_message = 'Vorgang durch Neustart abgebrochen', finished_at_ms = ? WHERE status IN ('queued', 'running') AND id IS NOT ?",
    )
    .bind(ERROR_INTERRUPTED)
    .bind(chrono::Utc::now().timestamp_millis())
    .bind(keep)
    .execute(&db.writer)
    .await?;
    Ok(r.rows_affected())
}

/// Aktueller Datenstand (Revision und Zähler).
pub async fn current_revision(db: &Db) -> anyhow::Result<Option<(String, i64)>> {
    let r = sqlx::query("SELECT revision_id, change_counter FROM data_state WHERE id = 1").fetch_optional(&db.reader).await?;
    Ok(r.map(|r| (r.get("revision_id"), r.get("change_counter"))))
}

/// Gibt es sicherungsrelevante Änderungen seit der letzten erfolgreichen Sicherung? Ohne Sicherung immer ja.
pub fn pending(state: &State, current: Option<&(String, i64)>, parameters: &str) -> bool {
    match (&state.last_success_revision_id, current) {
        (Some(saved), Some((now, _))) => saved != now || state.last_success_parameters.as_deref() != Some(parameters),
        _ => true,
    }
}

async fn start_operation(db: &Db, id: &str, trigger: Trigger) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO operations (id, kind, trigger, status, phase, started_at_ms) VALUES (?, 'backup', ?, 'running', 'checking', ?) \
         ON CONFLICT (id) DO UPDATE SET status = 'running', phase = 'checking', started_at_ms = excluded.started_at_ms",
    )
    .bind(id)
    .bind(trigger.as_str())
    .bind(chrono::Utc::now().timestamp_millis())
    .execute(&db.writer)
    .await?;
    Ok(())
}

async fn set_phase(db: &Db, id: &str, phase: &str) {
    if let Err(e) = sqlx::query("UPDATE operations SET phase = ? WHERE id = ?").bind(phase).bind(id).execute(&db.writer).await {
        tracing::warn!(error = %e, "Phase des Sicherungsvorgangs lässt sich nicht speichern");
    }
}

async fn finish(db: &Db, id: &str, trigger: Trigger, outcome: &Outcome, created: Option<&format::Manifest>) -> anyhow::Result<()> {
    let now = chrono::Utc::now().timestamp_millis();
    let (status, error_code, error_message, backup_id) = match outcome {
        Outcome::Created { name } => ("done", None, None, Some(name.as_str())),
        Outcome::Unchanged => ("done", None, None, None),
        Outcome::TargetMissing(e) => ("done", None, Some(e.as_str()), None),
        Outcome::Failed(e) => ("failed", Some(ERROR_FAILED), Some(e.as_str()), None),
    };
    let mut tx = db.writer.begin().await?;
    sqlx::query(
        "UPDATE operations SET status = ?, phase = NULL, outcome = ?, error_code = ?, error_message = ?, backup_id = ?, \
         finished_at_ms = ? WHERE id = ?",
    )
    .bind(status)
    .bind(outcome.code())
    .bind(error_code)
    .bind(error_message)
    .bind(backup_id)
    .bind(now)
    .bind(id)
    .execute(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM operations WHERE finished_at_ms < ?").bind(now - OPERATION_RETENTION_MS).execute(&mut *tx).await?;
    if let (Outcome::Created { name }, Some(m)) = (outcome, created) {
        sqlx::query(
            "UPDATE backup_state SET last_success_at_ms = ?, last_success_name = ?, last_success_revision_id = ?, \
             last_success_counter = ?, last_success_parameters = ? WHERE id = 1",
        )
        .bind(now)
        .bind(name)
        .bind(m.data_state.as_ref().map(|d| d.revision_id.clone()))
        .bind(m.data_state.as_ref().map(|d| d.change_counter))
        .bind(&m.parameters_hash)
        .execute(&mut *tx)
        .await?;
    }
    if matches!(outcome, Outcome::Created { .. } | Outcome::Failed(_)) {
        let details = match (outcome, created) {
            (Outcome::Created { name }, Some(m)) => json!({
                "operation_id": id, "trigger": trigger.as_str(), "name": name, "files": m.files.len(), "bytes": m.total_bytes(),
                "change_counter": m.data_state.as_ref().map(|d| d.change_counter),
            }),
            _ => json!({ "operation_id": id, "trigger": trigger.as_str(), "error": error_message }),
        };
        audit::record(&mut tx, Action::Backup, "backup", backup_id, matches!(outcome, Outcome::Created { .. }), details).await?;
    }
    tx.commit().await?;
    Ok(())
}

async fn attempt(cfg: &Validated, db: &Db, trigger: Trigger, id: &str) -> Outcome {
    if let Err(e) = start_operation(db, id, trigger).await {
        tracing::error!(error = %e, "Sicherungsvorgang lässt sich nicht anlegen");
    }
    let outcome = match run(cfg, db, id, trigger).await {
        Ok(o) => o,
        Err(e) => Outcome::Failed(format!("{e:#}")),
    };
    let created =
        if let Outcome::Created { name } = &outcome { format::read_manifest(&cfg.raw.backup.target_dir.join(name)).ok() } else { None };
    if let Err(e) = finish(db, id, trigger, &outcome, created.as_ref()).await {
        tracing::error!(error = format!("{e:#}"), "Sicherungsstatus lässt sich nicht speichern");
    }
    match &outcome {
        Outcome::Created { name } => tracing::info!(trigger = trigger.as_str(), name, "Sicherung erstellt"),
        Outcome::Unchanged => tracing::info!(trigger = trigger.as_str(), "Sicherung übersprungen: keine Änderungen"),
        Outcome::TargetMissing(e) => tracing::warn!(trigger = trigger.as_str(), error = e, "Sicherung übersprungen: Ziel fehlt"),
        Outcome::Failed(e) => tracing::error!(trigger = trigger.as_str(), error = e, "Sicherung fehlgeschlagen"),
    }
    outcome
}

async fn run(cfg: &Validated, db: &Db, id: &str, trigger: Trigger) -> anyhow::Result<Outcome> {
    let target = cfg.raw.backup.target_dir.clone();
    if let Err(e) = format::target_ready(&target) {
        return Ok(Outcome::TargetMissing(e));
    }
    let state = load_state(db).await?;
    let current = current_revision(db).await?;
    if !pending(&state, current.as_ref(), &format::parameters(cfg)) {
        return Ok(Outcome::Unchanged);
    }

    set_phase(db, id, "staging").await;
    let staging = cfg.staging_dir().join(format!("sicherung-{id}"));
    let result = async {
        let snap = format::snapshot(cfg, db, &staging, trigger.as_str()).await?;
        let counter = snap.manifest.data_state.as_ref().map_or(0, |d| d.change_counter);
        let local = chrono::Utc::now().with_timezone(&cfg.timezone).naive_local();
        let name = format::stand_name(local, snap.manifest.schema_version, counter);
        set_phase(db, id, "copying").await;
        let (src, dst, n) = (staging.clone(), target.clone(), name.clone());
        tokio::task::spawn_blocking(move || format::copy_stand(&src, &dst, &n)).await??;
        set_phase(db, id, "cleanup").await;
        let t = target.clone();
        let keep = state.keep();
        let removed = tokio::task::spawn_blocking(move || format::retention(&t, keep)).await??;
        if !removed.is_empty() {
            tracing::info!(?removed, "Ältere Sicherungsstände entfernt");
        }
        Ok::<_, anyhow::Error>(Outcome::Created { name })
    }
    .await;
    if let Err(e) = fs::remove_dir_all(&staging)
        && staging.exists()
    {
        tracing::warn!(path = %staging.display(), error = %e, "Zwischenstand der Sicherung lässt sich nicht entfernen");
    }
    result
}

/// Fällig: Uhrzeit erreicht und heute noch kein täglicher Versuch (auch nicht vor einem Neustart).
pub fn daily_due(daily_time: &str, now_local: chrono::NaiveDateTime, last_date: Option<&str>) -> bool {
    let Ok(at) = chrono::NaiveTime::parse_from_str(daily_time, "%H:%M") else {
        return false;
    };
    let today = now_local.date().format("%Y-%m-%d").to_string();
    now_local.time() >= at && last_date != Some(today.as_str())
}

async fn mark_daily(db: &Db, date: &str) -> anyhow::Result<()> {
    sqlx::query("UPDATE backup_state SET daily_last_date = ? WHERE id = 1").bind(date).execute(&db.writer).await?;
    Ok(())
}

/// Läuft bis zum Abbruch der Aufgabe.
pub async fn scheduler(cfg: Arc<Validated>, db: Db, runner: Arc<Runner>) {
    loop {
        let queued = runner.queued.lock().ok().and_then(|g| g.clone());
        if let Some(id) = queued {
            let _ = runner.attempt_with(&cfg, &db, Trigger::Manual, Some(id)).await;
            if let Ok(mut g) = runner.queued.lock() {
                *g = None;
            }
        }
        let now = chrono::Utc::now().with_timezone(&cfg.timezone).naive_local();
        match load_state(&db).await {
            // Die Demo sichert nicht täglich; „Jetzt sichern“ geht auch dort.
            Ok(s) if !cfg.raw.demo.enabled && daily_due(&s.daily_time(&cfg), now, s.daily_last_date.as_deref()) => {
                if runner.attempt(&cfg, &db, Trigger::Daily).await.is_some()
                    && let Err(e) = mark_daily(&db, &now.date().format("%Y-%m-%d").to_string()).await
                {
                    tracing::error!(error = format!("{e:#}"), "Tagesversuch der Sicherung lässt sich nicht vermerken");
                }
            }
            Ok(_) => {}
            Err(e) => tracing::error!(error = format!("{e:#}"), "Sicherungsstatus nicht lesbar"),
        }
        tokio::select! {
            _ = runner.wake.notified() => {}
            _ = tokio::time::sleep(TICK) => {}
        }
    }
}

/// Name der lokalen Sicherung vor Migration; es bleibt nur die letzte (RFC-0012).
pub fn migration_name(from: i64, to: i64) -> String {
    format!("trackcraft-vor-migration-v{from}-v{to}")
}

fn is_migration_entry(name: &str) -> bool {
    name.starts_with("trackcraft-vor-migration-")
}

/// Vollsicherung vor Migration: lokal in `migration_dir` (nur die letzte bleibt), dann, falls erreichbar, ins Ziel.
/// Scheitert die lokale Sicherung, wird nicht migriert; scheitert nur die Kopie, wird protokolliert und migriert.
pub async fn before_migration(cfg: &Validated, db: &Db, from: i64, to: i64) -> anyhow::Result<()> {
    let dir = &cfg.raw.backup.migration_dir;
    let name = migration_name(from, to);
    let final_dir = dir.join(&name);
    let partial = dir.join(format!("{name}{}", format::PARTIAL));
    remove_any(&partial)?;
    let snap = match format::snapshot(cfg, db, &partial, "migration").await {
        Ok(s) => s,
        Err(e) => {
            let _ = remove_any(&partial);
            return Err(e)
                .with_context(|| format!("Sicherung vor Migration nach {} fehlgeschlagen, es wird nicht migriert", dir.display()));
        }
    };
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let n = entry.file_name().to_string_lossy().to_string();
        if is_migration_entry(&n) && entry.path() != partial {
            remove_any(&entry.path())?;
        }
    }
    fs::rename(&partial, &final_dir)?;
    tracing::info!(path = %final_dir.display(), from, to, "Sicherung vor Migration angelegt");

    let target = cfg.raw.backup.target_dir.clone();
    match format::target_ready(&target) {
        Err(e) => tracing::warn!(error = e, "Sicherung vor Migration nicht ins Sicherungsziel kopiert: Ziel fehlt"),
        Ok(()) => {
            let counter = snap.manifest.data_state.as_ref().map_or(0, |d| d.change_counter);
            let local = chrono::Utc::now().with_timezone(&cfg.timezone).naive_local();
            let stand = format::stand_name(local, from, counter);
            let src = final_dir.clone();
            // Vor Migration 23 gibt es die Spalte noch nicht: dann gilt die Vorgabe.
            let keep: Option<i64> =
                sqlx::query_scalar("SELECT keep FROM backup_state WHERE id = 1").fetch_optional(&db.reader).await.ok().flatten().flatten();
            let keep = State { keep, ..Default::default() }.keep();
            let copied = tokio::task::spawn_blocking(move || {
                let p = format::copy_stand(&src, &target, &stand)?;
                format::retention(&target, keep)?;
                Ok::<_, anyhow::Error>(p)
            })
            .await?;
            match copied {
                Ok(p) => tracing::info!(path = %p.display(), "Sicherung vor Migration ins Sicherungsziel kopiert"),
                Err(e) => tracing::error!(
                    error = format!("{e:#}"),
                    "Sicherung vor Migration nicht ins Sicherungsziel kopiert, es wird trotzdem migriert"
                ),
            }
        }
    }
    Ok(())
}

fn remove_any(p: &Path) -> std::io::Result<()> {
    match fs::symlink_metadata(p) {
        Ok(m) if m.is_dir() => fs::remove_dir_all(p),
        Ok(_) => fs::remove_file(p),
        Err(_) => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(h: u32, m: u32) -> chrono::NaiveDateTime {
        chrono::NaiveDate::from_ymd_opt(2026, 11, 26).unwrap().and_hms_opt(h, m, 0).unwrap()
    }

    #[test]
    fn daily_runs_once_after_time() {
        assert!(!daily_due("03:00", at(2, 59), None));
        assert!(daily_due("03:00", at(3, 0), None));
        assert!(daily_due("03:00", at(14, 0), Some("2026-11-25")));
        assert!(!daily_due("03:00", at(14, 0), Some("2026-11-26")));
    }

    #[test]
    fn pending_needs_revision_and_parameters() {
        let s = State { last_success_revision_id: Some("r1".into()), last_success_parameters: Some("p".into()), ..Default::default() };
        assert!(!pending(&s, Some(&("r1".into(), 3)), "p"));
        assert!(pending(&s, Some(&("r2".into(), 4)), "p"));
        assert!(pending(&s, Some(&("r1".into(), 3)), "q"));
        assert!(pending(&State::default(), Some(&("r1".into(), 3)), "p"));
    }
}
