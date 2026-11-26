//! Importvorgänge (RFC-0003 Abschnitt 8, RFC-0008 Abschnitte 1, 7 und 8): Staging, Sessions mit Sperren und
//! Duplikatkandidaten, `state_token`, Bestätigung, atomares Speichern, Abbruch, Wiederanlauf und Aufräumen (C12).
//!
//! Staging (ADR-0011): je Vorgang `staging/<id>/upload`. Der Vorgang entsteht erst, wenn die Datei geparst ist;
//! abgewiesene Uploads hinterlassen nichts. Die geprüften Sessions stehen als JSON in `import_jobs.sessions_json`.
//! Sperren durch vorhandene Sessions, Duplikatkandidaten und der `state_token` werden bei jeder Abfrage neu
//! bestimmt. Beim Bestätigen wird die Datei erneut gelesen; weicht das Ergebnis ab (etwa nach geänderter
//! Konfiguration), gilt der Stand als geändert.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, anyhow};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::Row;
use sqlx::sqlite::SqliteRow;

use crate::audit::{self, Action};
use crate::config::Validated;
use crate::db::{self, Db, new_id};
use crate::domain::track;
use crate::import::process::{self, Lock, Prepared, StartOrigin, TimeOrigin, ValueOrigin};
use crate::import::{self, Format, ImportError};
use crate::localtime::{self, LocalTimeError, LocalTimeInput};
use crate::sports::{self, SportMapping};

/// Unbestätigte Vorgänge gelten nach 24 Stunden als verwaist (C12).
pub const ORPHAN_AFTER_MS: i64 = 24 * 3600 * 1000;
/// Abgeschlossene Importvorgänge und Aufträge bleiben 30 Tage (C12).
pub const KEEP_FINISHED_MS: i64 = 30 * 24 * 3600 * 1000;

const UPLOAD_FILE: &str = "upload";
const OPEN: &str = "('staged', 'awaiting_decision')";

pub fn staging_dir(cfg: &Validated, id: &str) -> PathBuf {
    cfg.staging_dir().join(id)
}

pub fn upload_path(cfg: &Validated, id: &str) -> PathBuf {
    staging_dir(cfg, id).join(UPLOAD_FILE)
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// Zeitpunkt nach RFC-0003: UTC, Millisekunden, `Z`.
pub fn timestamp(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms).map(|d| d.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)).unwrap_or_default()
}

fn hex(bytes: impl IntoIterator<Item = u8>) -> String {
    bytes.into_iter().map(|b| format!("{b:02x}")).collect()
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex(Sha256::digest(bytes).iter().copied())
}

// ---------------------------------------------------------------------------------------------------------
// Sessions

/// Geprüfte Session, wie sie beim Upload bestimmt wurde.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionSummary {
    pub session_index: i64,
    /// `locked_limit` oder `locked_vehicle`; `locked_existing` wird bei jeder Abfrage neu bestimmt
    pub lock_code: Option<String>,
    pub lock_reason: Option<String>,
    /// Titel aus der Datei oder Ersatztitel; ohne Start erst nach der Zeiteingabe bestimmbar
    pub title: Option<String>,
    pub source_sport: Option<String>,
    pub sport_class: String,
    pub sub_sport: Option<String>,
    /// Bezeichnung des Untertyps; fehlt in Vorgängen älterer Versionen
    #[serde(default)]
    pub sub_sport_label: Option<String>,
    pub start_ms: Option<i64>,
    pub total_time_ms: Option<i64>,
    pub distance_m: Option<f64>,
    pub point_count: i64,
    pub has_position: bool,
    pub time_input_required: bool,
    /// Ø Herzfrequenz (Dateiwert, sonst Mittel der Messpunkte), nur Anzeige im Importdialog
    #[serde(default)]
    pub avg_heart_rate_bpm: Option<u16>,
}

impl SessionSummary {
    fn display_title(&self) -> String {
        self.title.clone().or_else(|| self.sub_sport_label.clone()).unwrap_or_else(|| process::class_label(&self.sport_class).to_string())
    }
}

fn summarize(p: &Prepared, cfg: &Validated) -> SessionSummary {
    let (lock_code, lock_reason) = match &p.lock {
        None => (None, None),
        Some(l @ Lock::PointLimit { count, max }) => {
            (Some(l.code().to_string()), Some(format!("Die Session hat {count} Punkte, erlaubt sind höchstens {max}.")))
        }
        Some(l @ Lock::Vehicle(f)) => (
            Some(l.code().to_string()),
            Some(process::vehicle_message(f, p.sport_class, cfg.timezone, cfg.raw.processing.vehicle_detection.window_s)),
        ),
    };
    SessionSummary {
        session_index: p.session_index as i64,
        lock_code,
        lock_reason,
        title: p.title.clone(),
        source_sport: p.source_sport.clone(),
        sport_class: p.sport_class.to_string(),
        sub_sport: p.sub_sport.clone(),
        sub_sport_label: p.sub_sport_label.clone(),
        start_ms: p.start.map(|s| s.0),
        total_time_ms: p.total_time.map(|t| t.0),
        distance_m: p.distance.map(|d| d.0),
        point_count: p.point_count() as i64,
        has_position: p.sections.iter().flatten().any(|pt| pt.pos.is_some()),
        time_input_required: p.needs_time_input,
        avg_heart_rate_bpm: p.avg_hr.map(|(h, _)| h).filter(|h| (1..=300).contains(h)),
    }
}

/// Parsen und Aufbereiten (rechenintensiv, im Blocking-Pool aufrufen). `source_override` ersetzt den
/// `source_sport` einer Datei mit genau einer Session (Garmin-Typ beim Abruf).
fn analyse(bytes: &[u8], cfg: &Validated, mapping: &SportMapping, source_override: Option<&str>) -> Result<Analysed, ImportError> {
    let mut parsed = import::parse(bytes)?;
    if let (Some(o), [only]) = (source_override, parsed.sessions.as_mut_slice()) {
        only.source_sport = Some(o.to_string());
    }
    let prepared = parsed.sessions.iter().map(|s| process::prepare(s, &cfg.raw, mapping, cfg.timezone)).collect();
    Ok(Analysed { format: parsed.format, prepared, checksum_ignored: parsed.checksum_ignored })
}

struct Analysed {
    format: Format,
    prepared: Vec<Prepared>,
    /// Datei-Prüfsumme falsch, Inhalt trotzdem gelesen
    checksum_ignored: bool,
}

async fn analyse_file(
    cfg: &Arc<Validated>,
    db: &Db,
    id: &str,
    source_override: Option<String>,
) -> anyhow::Result<Result<Analysed, ImportError>> {
    let path = upload_path(cfg, id);
    let bytes = tokio::fs::read(&path).await.with_context(|| format!("Staging-Datei {} fehlt", path.display()))?;
    // Sportart-Zuordnung aus der Datenbank
    let mapping = sports::load(db).await?;
    let c = cfg.clone();
    Ok(tokio::task::spawn_blocking(move || analyse(&bytes, &c, &mapping, source_override.as_deref())).await?)
}

// ---------------------------------------------------------------------------------------------------------
// Vorgänge lesen

#[derive(Debug, Clone)]
struct JobRow {
    id: String,
    original_filename: String,
    format: Option<String>,
    size_bytes: Option<i64>,
    sha256: Option<String>,
    status: String,
    error_code: Option<String>,
    error_message: Option<String>,
    created_at_ms: i64,
    finished_at_ms: Option<i64>,
    sessions_json: String,
    sessions: Vec<SessionSummary>,
    imported_ids: Vec<String>,
    source_override: Option<String>,
}

const JOB_COLUMNS: &str = "id, original_filename, format, size_bytes, sha256, status, error_code, error_message, \
                           created_at_ms, finished_at_ms, sessions_json, imported_activity_ids, source_sport_override";

fn job_from_row(r: &SqliteRow) -> anyhow::Result<JobRow> {
    let sessions_json: Option<String> = r.get("sessions_json");
    let sessions_json = sessions_json.unwrap_or_else(|| "[]".into());
    let imported: Option<String> = r.get("imported_activity_ids");
    Ok(JobRow {
        id: r.get("id"),
        original_filename: r.get("original_filename"),
        format: r.get("format"),
        size_bytes: r.get("size_bytes"),
        sha256: r.get("sha256"),
        status: r.get("status"),
        error_code: r.get("error_code"),
        error_message: r.get("error_message"),
        created_at_ms: r.get("created_at_ms"),
        finished_at_ms: r.get("finished_at_ms"),
        sessions: serde_json::from_str(&sessions_json).context("sessions_json unlesbar")?,
        sessions_json,
        imported_ids: imported.map(|s| serde_json::from_str(&s)).transpose()?.unwrap_or_default(),
        source_override: r.get("source_sport_override"),
    })
}

async fn load(db: &Db, id: &str) -> anyhow::Result<Option<JobRow>> {
    // Nur echte UUIDs; die ID wird später Teil eines Staging-Pfads.
    if uuid::Uuid::parse_str(id).is_err() {
        return Ok(None);
    }
    let sql = format!("SELECT {JOB_COLUMNS} FROM import_jobs WHERE id = ?");
    let row = sqlx::query(sqlx::AssertSqlSafe(sql)).bind(id).fetch_optional(&db.reader).await?;
    row.as_ref().map(job_from_row).transpose()
}

#[derive(Debug, Clone)]
struct Candidate {
    activity_id: String,
    title: String,
    sport_class: String,
    start_ms: i64,
    total_time_ms: i64,
    distance_m: Option<f64>,
    matched: Vec<&'static str>,
}

#[derive(Debug, Clone)]
struct LiveSession {
    state: &'static str,
    lock_reason: Option<String>,
    existing_activity_id: Option<String>,
    candidates: Vec<Candidate>,
}

/// Abweichung höchstens `ratio` vom größeren Wert.
fn within(a: f64, b: f64, ratio: f64) -> bool {
    (a - b).abs() <= ratio * a.max(b)
}

/// Aktueller Stand eines offenen Vorgangs: Sperren, vorhandene Sessions, Kandidaten und `state_token`.
async fn live(db: &Db, cfg: &Validated, job: &JobRow) -> anyhow::Result<(Vec<LiveSession>, String)> {
    let mut file_id: Option<String> = None;
    let mut existing: HashMap<i64, String> = HashMap::new();
    if let Some(sha) = &job.sha256 {
        file_id = sqlx::query_scalar("SELECT id FROM original_files WHERE sha256 = ?").bind(sha).fetch_optional(&db.reader).await?;
        if let Some(fid) = &file_id {
            for r in
                sqlx::query("SELECT session_index, id FROM activities WHERE original_file_id = ?").bind(fid).fetch_all(&db.reader).await?
            {
                existing.insert(r.get("session_index"), r.get("id"));
            }
        }
    }

    let dup = &cfg.raw.import.duplicates;
    let tol_ms = dup.start_tolerance_s as i64 * 1000;
    let mut out = Vec::with_capacity(job.sessions.len());
    for s in &job.sessions {
        if let Some(id) = existing.get(&s.session_index) {
            out.push(LiveSession {
                state: "locked_existing",
                lock_reason: Some("already_imported".into()),
                existing_activity_id: Some(id.clone()),
                candidates: vec![],
            });
            continue;
        }
        if let Some(code) = &s.lock_code {
            let state = if code == "locked_limit" { "locked_limit" } else { "locked_vehicle" };
            out.push(LiveSession { state, lock_reason: s.lock_reason.clone(), existing_activity_id: None, candidates: vec![] });
            continue;
        }
        // Duplikatkandidaten nach Originalwerten (RFC-0008 Abschnitt 8); ohne Start kein Vergleich.
        let mut candidates = vec![];
        if let Some(start) = s.start_ms {
            let rows = sqlx::query(
                "SELECT id, title, sport_class, coalesce(start_time_original_ms, start_time_ms) AS s, \
                 coalesce(total_time_original_ms, total_time_ms) AS t, distance_m FROM activities \
                 WHERE coalesce(start_time_original_ms, start_time_ms) BETWEEN ? AND ? AND original_file_id <> ? \
                 ORDER BY s, id",
            )
            .bind(start - tol_ms)
            .bind(start + tol_ms)
            .bind(file_id.as_deref().unwrap_or(""))
            .fetch_all(&db.reader)
            .await?;
            for r in rows {
                let t: i64 = r.get("t");
                let d: Option<f64> = r.get("distance_m");
                let mut matched = vec!["start_time"];
                if let Some(nt) = s.total_time_ms {
                    if !within(nt as f64, t as f64, dup.duration_tolerance_ratio) {
                        continue;
                    }
                    matched.push("total_time");
                }
                if let (Some(nd), Some(od)) = (s.distance_m, d) {
                    if !within(nd, od, dup.distance_tolerance_ratio) {
                        continue;
                    }
                    matched.push("distance");
                }
                candidates.push(Candidate {
                    activity_id: r.get("id"),
                    title: r.get("title"),
                    sport_class: r.get("sport_class"),
                    start_ms: r.get("s"),
                    total_time_ms: t,
                    distance_m: d,
                    matched,
                });
            }
        }
        out.push(LiveSession { state: "selectable", lock_reason: None, existing_activity_id: None, candidates });
    }

    // Token über alles Entscheidungsrelevante: geprüfte Sessions, Sperren, vorhandene Aktivitäten, Kandidaten.
    let basis: Vec<Value> = job
        .sessions
        .iter()
        .zip(&out)
        .map(|(s, l)| {
            let ids: Vec<&str> = l.candidates.iter().map(|c| c.activity_id.as_str()).collect();
            json!([s.session_index, l.state, l.existing_activity_id, ids])
        })
        .collect();
    let mut h = Sha256::new();
    h.update(job.id.as_bytes());
    h.update(job.sessions_json.as_bytes());
    h.update(Value::Array(basis).to_string().as_bytes());
    let token = format!("st_{}", hex(h.finalize().iter().take(12).copied()));
    Ok((out, token))
}

fn session_json(s: &SessionSummary, l: Option<&LiveSession>) -> Value {
    let (state, lock_reason, existing, candidates) = match l {
        Some(l) => (l.state, l.lock_reason.clone(), l.existing_activity_id.clone(), l.candidates.as_slice()),
        None => match s.lock_code.as_deref() {
            Some("locked_limit") => ("locked_limit", s.lock_reason.clone(), None, &[][..]),
            Some(_) => ("locked_vehicle", s.lock_reason.clone(), None, &[][..]),
            None => ("selectable", None, None, &[][..]),
        },
    };
    let candidates: Vec<Value> = candidates
        .iter()
        .map(|c| {
            json!({
                "activity_id": c.activity_id,
                "title": c.title,
                "sport_class": c.sport_class,
                "start_time": timestamp(c.start_ms),
                "total_time_ms": c.total_time_ms,
                "distance_m": c.distance_m,
                "matched_criteria": c.matched,
            })
        })
        .collect();
    json!({
        "session_index": s.session_index,
        "state": state,
        "lock_reason": lock_reason,
        "preselected": state == "selectable",
        "title": s.display_title(),
        "source_sport": s.source_sport,
        "sport_class": s.sport_class,
        "sub_sport": s.sub_sport,
        "start_time": s.start_ms.map(timestamp),
        "total_time_ms": s.total_time_ms,
        "distance_m": s.distance_m,
        "point_count": s.point_count,
        "has_position": s.has_position,
        "time_input_required": s.time_input_required,
        "avg_heart_rate_bpm": s.avg_heart_rate_bpm,
        "existing_activity_id": existing,
        "duplicate_candidates": candidates,
    })
}

async fn job_json(db: &Db, cfg: &Validated, job: &JobRow) -> anyhow::Result<Value> {
    let (sessions, token) = if job.status == "awaiting_decision" {
        let (live, token) = live(db, cfg, job).await?;
        (job.sessions.iter().zip(&live).map(|(s, l)| session_json(s, Some(l))).collect::<Vec<_>>(), Some(token))
    } else {
        (job.sessions.iter().map(|s| session_json(s, None)).collect(), None)
    };
    let error = job.error_code.as_ref().map(|c| json!({ "code": c, "message": job.error_message.clone().unwrap_or_default() }));
    Ok(json!({
        "id": job.id,
        "status": job.status,
        "original_filename": job.original_filename,
        "format": job.format,
        "size_bytes": job.size_bytes,
        "state_token": token,
        "error": error,
        "sessions": sessions,
        "imported_activity_ids": job.imported_ids,
        "created_at": timestamp(job.created_at_ms),
        "finished_at": job.finished_at_ms.map(timestamp),
    }))
}

pub async fn view(cfg: &Validated, db: &Db, id: &str) -> anyhow::Result<Option<Value>> {
    match load(db, id).await? {
        Some(job) => Ok(Some(job_json(db, cfg, &job).await?)),
        None => Ok(None),
    }
}

pub const STATUSES: &[&str] = &["staged", "awaiting_decision", "running", "done", "failed", "rolled_back"];

/// Vorgänge, neueste zuerst; liefert Seite und Gesamtzahl.
pub async fn list(cfg: &Validated, db: &Db, status: Option<&str>, limit: i64, offset: i64) -> anyhow::Result<(Vec<Value>, i64)> {
    let filter = if status.is_some() { "WHERE status = ?" } else { "" };
    let sql = format!("SELECT {JOB_COLUMNS} FROM import_jobs {filter} ORDER BY created_at_ms DESC, id DESC LIMIT ? OFFSET ?");
    let count_sql = format!("SELECT count(*) FROM import_jobs {filter}");
    let mut q = sqlx::query(sqlx::AssertSqlSafe(sql));
    let mut c = sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(count_sql));
    if let Some(s) = status {
        q = q.bind(s);
        c = c.bind(s);
    }
    let rows = q.bind(limit).bind(offset).fetch_all(&db.reader).await?;
    let total = c.fetch_one(&db.reader).await?;
    let mut out = Vec::with_capacity(rows.len());
    for r in &rows {
        out.push(job_json(db, cfg, &job_from_row(r)?).await?);
    }
    Ok((out, total))
}

// ---------------------------------------------------------------------------------------------------------
// Anlegen

#[derive(Debug)]
pub enum CreateError {
    Rejected(ImportError),
    Internal(anyhow::Error),
}

impl<E: Into<anyhow::Error>> From<E> for CreateError {
    fn from(e: E) -> Self {
        Self::Internal(e.into())
    }
}

/// Legt den Vorgang zu einer fertig empfangenen Staging-Datei an. Bei Abweisung bleibt nichts liegen.
pub async fn create(cfg: &Arc<Validated>, db: &Db, id: &str, filename: &str, size: i64, sha: &str) -> Result<Value, CreateError> {
    create_with(cfg, db, id, filename, size, sha, None).await
}

/// Wie [`create`], mit Ersatz für den `source_sport` (Garmin-Abruf).
pub async fn create_with(
    cfg: &Arc<Validated>,
    db: &Db,
    id: &str,
    filename: &str,
    size: i64,
    sha: &str,
    source_override: Option<&str>,
) -> Result<Value, CreateError> {
    let analysed = analyse_file(cfg, db, id, source_override.map(str::to_string)).await;
    let Analysed { format, prepared, checksum_ignored } = match analysed {
        Ok(Ok(v)) => v,
        Ok(Err(rejected)) => {
            remove_staging(cfg, id);
            tracing::warn!(file = filename, code = rejected.code(), reason = %rejected, "Upload abgewiesen");
            return Err(CreateError::Rejected(rejected));
        }
        Err(e) => {
            remove_staging(cfg, id);
            return Err(e.into());
        }
    };
    let sessions: Vec<SessionSummary> = prepared.iter().map(|p| summarize(p, cfg)).collect();
    let insert = sqlx::query(
        "INSERT INTO import_jobs (id, original_filename, format, size_bytes, sha256, status, created_at_ms, sessions_json, \
         source_sport_override) VALUES (?, ?, ?, ?, ?, 'awaiting_decision', ?, ?, ?)",
    )
    .bind(id)
    .bind(filename)
    .bind(format.as_str())
    .bind(size)
    .bind(sha)
    .bind(now_ms())
    .bind(serde_json::to_string(&sessions)?)
    .bind(source_override)
    .execute(&db.writer)
    .await;
    if let Err(e) = insert {
        remove_staging(cfg, id);
        return Err(e.into());
    }
    if checksum_ignored {
        tracing::warn!(id, file = filename, "Prüfsumme der Datei falsch, Inhalt trotzdem gelesen");
    }
    tracing::info!(id, file = filename, format = format.as_str(), sessions = sessions.len(), "Importvorgang angelegt");
    view(cfg, db, id).await?.ok_or_else(|| CreateError::Internal(anyhow!("Vorgang {id} fehlt nach dem Anlegen")))
}

pub fn remove_staging(cfg: &Validated, id: &str) {
    let dir = staging_dir(cfg, id);
    if dir.exists()
        && let Err(e) = std::fs::remove_dir_all(&dir)
    {
        tracing::warn!(path = %dir.display(), error = %e, "Staging-Ordner lässt sich nicht entfernen");
    }
}

// ---------------------------------------------------------------------------------------------------------
// Bestätigen

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportDecision {
    pub state_token: String,
    pub sessions: Vec<SessionDecision>,
}

/// Ohne Zustands-Token im Klartext (ADR-0013).
impl std::fmt::Debug for ImportDecision {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImportDecision").field("state_token", &crate::logging::Hidden).field("sessions", &self.sessions).finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DecisionAction {
    Import,
    Replace,
    Skip,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionDecision {
    pub session_index: i64,
    pub action: DecisionAction,
    #[serde(default)]
    pub replace_activity_id: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub carry_over: Option<CarryOver>,
    #[serde(default)]
    pub start_time: Option<LocalTimeInput>,
    #[serde(default)]
    pub total_time_ms: Option<i64>,
}

fn yes() -> bool {
    true
}

/// Übernahme aus der ersetzten Aktivität, Vorgabe jeweils `true`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CarryOver {
    #[serde(default = "yes")]
    pub title: bool,
    #[serde(default = "yes")]
    pub description: bool,
    #[serde(default = "yes")]
    pub location: bool,
    #[serde(default = "yes")]
    pub sport_class: bool,
    #[serde(default = "yes")]
    pub sub_sport: bool,
}

impl Default for CarryOver {
    fn default() -> Self {
        Self { title: true, description: true, location: true, sport_class: true, sub_sport: true }
    }
}

#[derive(Debug)]
pub enum ConfirmError {
    NotFound,
    NotConfirmable,
    StateChanged,
    /// `invalid_import_decision`: Session unbekannt oder gesperrt, Ziel kein Kandidat oder doppelt, Aktion unpassend
    InvalidDecision {
        field: String,
        message: String,
    },
    TimeInputRequired {
        field: String,
    },
    LocalTimeAmbiguous {
        field: String,
    },
    Validation {
        field: String,
        code: &'static str,
        message: String,
    },
    Internal(anyhow::Error),
}

impl<E: Into<anyhow::Error>> From<E> for ConfirmError {
    fn from(e: E) -> Self {
        Self::Internal(e.into())
    }
}

#[derive(Debug, Clone)]
enum PlanAction {
    Import,
    Replace { target: String, carry: CarryOver },
}

#[derive(Debug, Clone)]
struct Plan {
    /// Position der Session im Vorgang
    pos: usize,
    action: PlanAction,
    title: Option<String>,
    manual_start_ms: Option<i64>,
    manual_total_ms: Option<i64>,
}

pub struct Confirmed {
    pub job: Value,
    pub notices: Vec<Value>,
}

fn invalid(field: String, message: &str) -> ConfirmError {
    ConfirmError::InvalidDecision { field, message: message.into() }
}

fn validate(
    cfg: &Validated,
    job: &JobRow,
    live: &[LiveSession],
    decision: &ImportDecision,
) -> Result<(Vec<Plan>, Vec<Value>), ConfirmError> {
    let mut plans = vec![];
    let mut notices = vec![];
    let mut seen = HashSet::new();
    let mut targets = HashSet::new();
    if decision.sessions.is_empty() {
        return Err(ConfirmError::Validation {
            field: "sessions".into(),
            code: "required",
            message: "At least one session decision".into(),
        });
    }
    for (k, d) in decision.sessions.iter().enumerate() {
        let f = |name: &str| format!("sessions[{k}].{name}");
        let Some(pos) = job.sessions.iter().position(|s| s.session_index == d.session_index) else {
            return Err(invalid(f("session_index"), "Unknown session"));
        };
        if !seen.insert(d.session_index) {
            return Err(invalid(f("session_index"), "Session decided more than once"));
        }
        let summary = &job.sessions[pos];
        if d.action == DecisionAction::Skip {
            if d.replace_activity_id.is_some() || d.carry_over.is_some() || d.start_time.is_some() || d.total_time_ms.is_some() {
                return Err(invalid(f("action"), "Skip takes no further values"));
            }
            continue;
        }
        if live[pos].state != "selectable" {
            return Err(invalid(f("session_index"), "Session is locked"));
        }
        let title = match &d.title {
            Some(t) if t.trim().is_empty() => {
                return Err(ConfirmError::Validation {
                    field: f("title"),
                    code: "must_not_be_empty",
                    message: "Title must not be empty".into(),
                });
            }
            Some(t) => Some(t.trim().to_string()),
            None => None,
        };
        let action = match d.action {
            DecisionAction::Import => {
                if d.replace_activity_id.is_some() || d.carry_over.is_some() {
                    return Err(invalid(f("action"), "replace_activity_id and carry_over need action replace"));
                }
                PlanAction::Import
            }
            DecisionAction::Replace => {
                let Some(target) = &d.replace_activity_id else {
                    return Err(ConfirmError::Validation {
                        field: f("replace_activity_id"),
                        code: "required",
                        message: "replace_activity_id is required for replace".into(),
                    });
                };
                if !live[pos].candidates.iter().any(|c| &c.activity_id == target) {
                    return Err(invalid(f("replace_activity_id"), "Replace target is not a duplicate candidate"));
                }
                if !targets.insert(target.clone()) {
                    return Err(invalid(f("replace_activity_id"), "Replace target chosen more than once"));
                }
                PlanAction::Replace { target: target.clone(), carry: d.carry_over.unwrap_or_default() }
            }
            DecisionAction::Skip => unreachable!(),
        };
        let (mut manual_start_ms, mut manual_total_ms) = (None, None);
        if summary.time_input_required {
            let (Some(start), Some(total)) = (&d.start_time, d.total_time_ms) else {
                return Err(ConfirmError::TimeInputRequired { field: f("start_time") });
            };
            if total < 0 {
                return Err(ConfirmError::Validation {
                    field: f("total_time_ms"),
                    code: "must_not_be_negative",
                    message: "total_time_ms must be >= 0".into(),
                });
            }
            let converted = localtime::to_utc(start, cfg.timezone).map_err(|e| match e {
                LocalTimeError::Ambiguous => ConfirmError::LocalTimeAmbiguous { field: f("start_time") },
                LocalTimeError::Invalid => ConfirmError::Validation {
                    field: f("start_time.local"),
                    code: "invalid_format",
                    message: "Expected YYYY-MM-DDTHH:MM:SS".into(),
                },
            })?;
            if converted.shifted {
                notices.push(json!({
                    "code": "local_time_shifted",
                    "message": format!("Session {}: local time did not exist and was moved one hour later", d.session_index),
                }));
            }
            manual_start_ms = Some(converted.utc_ms);
            manual_total_ms = Some(total);
        } else if d.start_time.is_some() || d.total_time_ms.is_some() {
            return Err(invalid(f("start_time"), "Start and total time are only accepted when time input is required"));
        }
        plans.push(Plan { pos, action, title, manual_start_ms, manual_total_ms });
    }
    Ok((plans, notices))
}

pub async fn confirm(cfg: &Arc<Validated>, db: &Db, id: &str, decision: &ImportDecision) -> Result<Confirmed, ConfirmError> {
    let job = load(db, id).await?.ok_or(ConfirmError::NotFound)?;
    if job.status != "awaiting_decision" {
        return Err(ConfirmError::NotConfirmable);
    }
    let (live, token) = live(db, cfg, &job).await?;
    if token != decision.state_token {
        return Err(ConfirmError::StateChanged);
    }
    let (plans, mut notices) = validate(cfg, &job, &live, decision)?;

    // Datei erneut lesen; das Ergebnis muss dem angezeigten entsprechen.
    let (prepared, checksum_ignored) = if plans.is_empty() {
        (vec![], false)
    } else {
        let Analysed { prepared, checksum_ignored, .. } = analyse_file(cfg, db, &job.id, job.source_override.clone())
            .await?
            .map_err(|e| anyhow!("Staging-Datei nicht mehr lesbar: {e}"))?;
        let fresh: Vec<SessionSummary> = prepared.iter().map(|p| summarize(p, cfg)).collect();
        let fresh_json = serde_json::to_string(&fresh)?;
        if fresh_json != job.sessions_json {
            sqlx::query("UPDATE import_jobs SET sessions_json = ? WHERE id = ? AND status = 'awaiting_decision'")
                .bind(&fresh_json)
                .bind(&job.id)
                .execute(&db.writer)
                .await?;
            tracing::info!(id = job.id, "Importvorgang neu geprüft, Ergebnis weicht ab");
            return Err(ConfirmError::StateChanged);
        }
        (prepared, checksum_ignored)
    };
    if checksum_ignored {
        notices.push(json!({
            "code": "file_checksum_ignored",
            "message": "The file checksum did not match; the content was read completely and imported",
        }));
    }

    let started = sqlx::query("UPDATE import_jobs SET status = 'running' WHERE id = ? AND status = 'awaiting_decision'")
        .bind(&job.id)
        .execute(&db.writer)
        .await?;
    if started.rows_affected() == 0 {
        return Err(ConfirmError::NotConfirmable);
    }

    // Eigener Task: ein abgebrochener Request darf das Speichern nicht mittendrin beenden.
    let (c, d, j) = (cfg.clone(), db.clone(), job.clone());
    tokio::spawn(async move { save(&c, &d, &j, prepared, plans, checksum_ignored).await }).await?;

    let view = view(cfg, db, id).await?.ok_or_else(|| anyhow!("Vorgang {id} fehlt nach dem Speichern"))?;
    Ok(Confirmed { job: view, notices })
}

/// Audit-Details eines gespeicherten Imports; eine übergangene Prüfsumme bleibt dort sichtbar.
fn audit_details(job: &JobRow, session_index: usize, checksum_ignored: bool) -> Value {
    let mut d = json!({ "import_job_id": job.id, "session_index": session_index, "file": job.original_filename });
    if checksum_ignored {
        d["checksum_ignored"] = json!(true);
    }
    d
}

fn start_origin(o: StartOrigin) -> &'static str {
    match o {
        StartOrigin::File => "file",
        StartOrigin::FirstPoint => "first_point",
    }
}

fn time_origin(o: TimeOrigin) -> &'static str {
    match o {
        TimeOrigin::File => "file",
        TimeOrigin::GpsPoints => "gps_points",
    }
}

/// Speichert und setzt den Endstatus; Fehler werden am Vorgang vermerkt, nicht weitergereicht.
async fn save(cfg: &Validated, db: &Db, job: &JobRow, prepared: Vec<Prepared>, plans: Vec<Plan>, checksum_ignored: bool) {
    match save_inner(cfg, db, job, &prepared, &plans, checksum_ignored).await {
        Ok(removed) => {
            for path in removed {
                if let Err(e) = std::fs::remove_file(&path) {
                    tracing::warn!(path = %path.display(), error = %e, "Nicht mehr referenziertes Original lässt sich nicht entfernen");
                }
            }
            tracing::info!(id = job.id, sessions = plans.len(), "Import gespeichert");
        }
        Err(e) => {
            tracing::error!(id = job.id, error = format!("{e:#}"), "Import fehlgeschlagen, zurückgenommen");
            if let Err(e2) = mark_failed(db, &job.id, "internal_error", "Saving failed; nothing was imported").await {
                tracing::error!(id = job.id, error = %e2, "Fehlerstatus lässt sich nicht setzen");
            }
        }
    }
    remove_staging(cfg, &job.id);
}

async fn mark_failed(db: &Db, id: &str, code: &str, message: &str) -> anyhow::Result<()> {
    let mut tx = db.writer.begin().await?;
    sqlx::query("UPDATE import_jobs SET status = 'failed', error_code = ?, error_message = ?, finished_at_ms = ? WHERE id = ?")
        .bind(code)
        .bind(message)
        .bind(now_ms())
        .bind(id)
        .execute(&mut *tx)
        .await?;
    audit::record(&mut tx, Action::Import, "import_job", Some(id), false, json!({ "error": code })).await?;
    tx.commit().await?;
    Ok(())
}

/// Liefert die Originale, die nach dem Commit entfernt werden (nicht mehr referenziert).
async fn save_inner(
    cfg: &Validated,
    db: &Db,
    job: &JobRow,
    prepared: &[Prepared],
    plans: &[Plan],
    checksum_ignored: bool,
) -> anyhow::Result<Vec<PathBuf>> {
    let now = now_ms();
    if plans.is_empty() {
        sqlx::query(
            "UPDATE import_jobs SET status = 'done', finished_at_ms = ?, imported_activity_count = 0, imported_activity_ids = '[]' WHERE id = ?",
        )
        .bind(now)
        .bind(&job.id)
        .execute(&db.writer)
        .await?;
        return Ok(vec![]);
    }

    let sha = job.sha256.as_deref().context("Vorgang ohne Prüfsumme")?;
    let format = job.format.as_deref().and_then(Format::parse).context("Vorgang ohne Format")?;
    let relative = format!("{sha}.{}", format.as_str());
    let archive = cfg.raw.paths.activities_dir.join(&relative);

    // Original ablegen, bevor die Transaktion beginnt: atomares Umbenennen auf demselben Laufwerk (ADR-0011).
    let known: Option<String> =
        sqlx::query_scalar("SELECT id FROM original_files WHERE sha256 = ?").bind(sha).fetch_optional(&db.writer).await?;
    let mut placed = false;
    if known.is_none() {
        std::fs::rename(upload_path(cfg, &job.id), &archive)
            .with_context(|| format!("Original {} lässt sich nicht ablegen", archive.display()))?;
        placed = true;
    }

    let result = write_activities(cfg, db, job, prepared, plans, known, &relative, now, checksum_ignored).await;
    if result.is_err() && placed {
        let _ = std::fs::remove_file(&archive);
    }
    result
}

#[allow(clippy::too_many_arguments)]
async fn write_activities(
    cfg: &Validated,
    db: &Db,
    job: &JobRow,
    prepared: &[Prepared],
    plans: &[Plan],
    known_file: Option<String>,
    relative: &str,
    now: i64,
    checksum_ignored: bool,
) -> anyhow::Result<Vec<PathBuf>> {
    let mut tx = db.writer.begin().await?;
    let user_id: String = sqlx::query_scalar("SELECT id FROM users LIMIT 1").fetch_one(&mut *tx).await.context("kein Benutzer")?;
    let file_id = match known_file {
        Some(id) => id,
        None => {
            let id = new_id();
            sqlx::query(
                "INSERT INTO original_files (id, format, relative_path, sha256, size_bytes, created_at_ms) VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(job.format.as_deref())
            .bind(relative)
            .bind(job.sha256.as_deref())
            .bind(job.size_bytes)
            .bind(now)
            .execute(&mut *tx)
            .await?;
            id
        }
    };

    let mut ids = vec![];
    let mut removed = vec![];
    for plan in plans {
        let p = &prepared[plan.pos];
        let (start_ms, start_src) = match plan.manual_start_ms {
            Some(ms) => (ms, "manual"),
            None => p.start.map(|(s, o)| (s, start_origin(o))).context("Start fehlt")?,
        };
        let (total_ms, total_src) = match plan.manual_total_ms {
            Some(ms) => (ms, "manual"),
            None => p.total_time.map(|(t, o)| (t, time_origin(o))).context("Gesamtzeit fehlt")?,
        };
        let distance = p.distance.map(|(d, _)| d);
        let distance_src = p.distance.map(|(_, o)| if o == ValueOrigin::File { "file" } else { "gps_points" });
        let hr = p.avg_hr.filter(|(h, _)| (1..=300).contains(h));
        let hr_src = hr.map(|(_, o)| if o == ValueOrigin::File { "file" } else { "points" });
        let ascent = p.ascent_file_m.filter(|a| *a > 0.0);
        let has_position = p.sections.iter().flatten().any(|pt| pt.pos.is_some());
        let own_title = || {
            plan.title
                .clone()
                .or_else(|| p.title.clone())
                .unwrap_or_else(|| process::fallback_title(p.sub_sport_label.as_deref(), p.sport_class, start_ms, cfg.timezone))
        };

        let activity_id = match &plan.action {
            PlanAction::Import => {
                let id = new_id();
                sqlx::query(
                    "INSERT INTO activities (id, user_id, original_file_id, session_index, title, sport_class, source_sport, sub_sport, \
                     start_time_ms, start_time_origin, total_time_ms, total_time_origin, distance_m, distance_origin, \
                     elevation_gain_file_m, filtered_outliers_count, max_speed_threshold_kmh, created_at_ms, updated_at_ms, \
                     avg_heart_rate_bpm, heart_rate_origin, has_position) \
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                )
                .bind(&id)
                .bind(&user_id)
                .bind(&file_id)
                .bind(p.session_index as i64)
                .bind(own_title())
                .bind(p.sport_class)
                .bind(&p.source_sport)
                .bind(&p.sub_sport)
                .bind(start_ms)
                .bind(start_src)
                .bind(total_ms)
                .bind(total_src)
                .bind(distance)
                .bind(distance_src)
                .bind(ascent)
                .bind(p.filtered_outliers_count as i64)
                .bind(p.max_speed_threshold_kmh)
                .bind(now)
                .bind(now)
                .bind(hr.map(|(h, _)| h as i64))
                .bind(hr_src)
                .bind(has_position)
                .execute(&mut *tx)
                .await?;
                audit::record(&mut tx, Action::Import, "activity", Some(&id), true, audit_details(job, p.session_index, checksum_ignored))
                    .await?;
                id
            }
            PlanAction::Replace { target, carry } => {
                let old = sqlx::query(
                    "SELECT title, description, location, sport_class, sub_sport, original_file_id FROM activities WHERE id = ?",
                )
                .bind(target)
                .fetch_optional(&mut *tx)
                .await?
                .with_context(|| format!("Ersetzungsziel {target} existiert nicht mehr"))?;
                let old_title: String = old.get("title");
                let old_file: String = old.get("original_file_id");
                let title = if carry.title { old_title } else { own_title() };
                let description: Option<String> = if carry.description { old.get("description") } else { None };
                let location: Option<String> = if carry.location { old.get("location") } else { None };
                let sport_class: String = if carry.sport_class { old.get("sport_class") } else { p.sport_class.to_string() };
                let mut sub_sport: Option<String> = if carry.sub_sport { old.get("sub_sport") } else { p.sub_sport.clone() };
                // Untertyp-Schlüssel gelten je Klasse; passt der übernommene nicht zur neuen Klasse, entfällt er.
                if let Some(k) = &sub_sport
                    && !sports::sub_exists(&mut tx, &sport_class, k).await?
                {
                    sub_sport = None;
                }

                sqlx::query(
                    "UPDATE activities SET original_file_id = ?, session_index = ?, title = ?, description = ?, location = ?, \
                     sport_class = ?, source_sport = ?, sub_sport = ?, start_time_ms = ?, start_time_original_ms = NULL, \
                     start_time_origin = ?, total_time_ms = ?, total_time_original_ms = NULL, total_time_origin = ?, distance_m = ?, \
                     distance_origin = ?, elevation_gain_file_m = ?, processing_status = 'pending', filtered_outliers_count = ?, \
                     max_speed_threshold_kmh = ?, revision = revision + 1, updated_at_ms = ?, avg_heart_rate_bpm = ?, \
                     heart_rate_origin = ?, has_position = ? WHERE id = ?",
                )
                .bind(&file_id)
                .bind(p.session_index as i64)
                .bind(title)
                .bind(description)
                .bind(location)
                .bind(sport_class)
                .bind(&p.source_sport)
                .bind(sub_sport)
                .bind(start_ms)
                .bind(start_src)
                .bind(total_ms)
                .bind(total_src)
                .bind(distance)
                .bind(distance_src)
                .bind(ascent)
                .bind(p.filtered_outliers_count as i64)
                .bind(p.max_speed_threshold_kmh)
                .bind(now)
                .bind(hr.map(|(h, _)| h as i64))
                .bind(hr_src)
                .bind(has_position)
                .bind(target)
                .execute(&mut *tx)
                .await?;
                // Alter Inhalt und alle Auswertungen der alten Fassung entfallen.
                for sql in [
                    "DELETE FROM segment_efforts WHERE activity_id = ?",
                    "DELETE FROM activity_calculations WHERE activity_id = ?",
                    "DELETE FROM track_sections WHERE activity_id = ?",
                ] {
                    sqlx::query(sql).bind(target).execute(&mut *tx).await?;
                }
                // Original der alten Fassung nur entfernen, wenn niemand mehr darauf verweist (ADR-0011).
                if old_file != file_id {
                    let refs: i64 = sqlx::query_scalar("SELECT count(*) FROM activities WHERE original_file_id = ?")
                        .bind(&old_file)
                        .fetch_one(&mut *tx)
                        .await?;
                    if refs == 0 {
                        let rel: String = sqlx::query_scalar("SELECT relative_path FROM original_files WHERE id = ?")
                            .bind(&old_file)
                            .fetch_one(&mut *tx)
                            .await?;
                        sqlx::query("DELETE FROM original_files WHERE id = ?").bind(&old_file).execute(&mut *tx).await?;
                        removed.push(archive_path(cfg, &rel)?);
                    }
                }
                audit::record(
                    &mut tx,
                    Action::Replace,
                    "activity",
                    Some(target),
                    true,
                    audit_details(job, p.session_index, checksum_ignored),
                )
                .await?;
                target.clone()
            }
        };

        for (i, points) in p.sections.iter().enumerate() {
            let data = track::encode(points)?;
            sqlx::query(
                "INSERT INTO track_sections (id, activity_id, section_index, format_version, point_count, first_time_ms, last_time_ms, data) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(new_id())
            .bind(&activity_id)
            .bind(i as i64)
            .bind(track::FORMAT_VERSION as i64)
            .bind(points.len() as i64)
            .bind(points.iter().find_map(|pt| pt.t_ms))
            .bind(points.iter().rev().find_map(|pt| pt.t_ms))
            .bind(data)
            .execute(&mut *tx)
            .await?;
        }

        // Hysteresewert in den veröffentlichten Berechnungsstand (M2, RFC-0007)
        crate::calculation::record_activity(&mut tx, &activity_id, p.elevation.map(|e| e.gain_m)).await?;

        // Auswertung als Auftrag, getrennt vom Import (RFC-0008 Abschnitt 1).
        let queued: i64 =
            sqlx::query_scalar("SELECT count(*) FROM jobs WHERE activity_id = ? AND kind = 'activity_evaluation' AND status = 'queued'")
                .bind(&activity_id)
                .fetch_one(&mut *tx)
                .await?;
        if queued == 0 {
            sqlx::query("INSERT INTO jobs (id, kind, activity_id, status, total_items, created_at_ms) VALUES (?, 'activity_evaluation', ?, 'queued', 1, ?)")
                .bind(new_id())
                .bind(&activity_id)
                .bind(now)
                .execute(&mut *tx)
                .await?;
        }
        ids.push(activity_id);
    }

    db::bump_revision(&mut tx, now).await?;
    sqlx::query(
        "UPDATE import_jobs SET status = 'done', finished_at_ms = ?, imported_activity_count = ?, imported_activity_ids = ? WHERE id = ?",
    )
    .bind(now)
    .bind(ids.len() as i64)
    .bind(serde_json::to_string(&ids)?)
    .bind(&job.id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(removed)
}

/// Pfad im Archiv; nur relative Pfade ohne `..` unter `activities_dir` (RFC-0011).
pub fn archive_path(cfg: &Validated, relative: &str) -> anyhow::Result<PathBuf> {
    let rel = std::path::Path::new(relative);
    let safe = rel.components().all(|c| matches!(c, std::path::Component::Normal(_)));
    if !safe || relative.is_empty() {
        anyhow::bail!("unsicherer Archivpfad {relative:?}");
    }
    Ok(cfg.raw.paths.activities_dir.join(rel))
}

// ---------------------------------------------------------------------------------------------------------
// Abbrechen, Wiederanlauf, Aufräumen

#[derive(Debug)]
pub enum CancelError {
    NotFound,
    NotCancellable,
    Internal(anyhow::Error),
}

impl<E: Into<anyhow::Error>> From<E> for CancelError {
    fn from(e: E) -> Self {
        Self::Internal(e.into())
    }
}

pub async fn cancel(cfg: &Validated, db: &Db, id: &str) -> Result<(), CancelError> {
    let job = load(db, id).await?.ok_or(CancelError::NotFound)?;
    let sql = format!("DELETE FROM import_jobs WHERE id = ? AND status IN {OPEN}");
    let deleted = sqlx::query(sqlx::AssertSqlSafe(sql)).bind(&job.id).execute(&db.writer).await?;
    if deleted.rows_affected() == 0 {
        return Err(CancelError::NotCancellable);
    }
    remove_staging(cfg, &job.id);
    tracing::info!(id = job.id, "Importvorgang abgebrochen");
    Ok(())
}

/// Beim Start (ADR-0011): unterbrochene Speicherungen zurücknehmen, Staging auf offene Vorgänge beschränken,
/// danach regulär aufräumen.
pub async fn recover(cfg: &Validated, db: &Db) -> anyhow::Result<()> {
    let now = now_ms();
    let running = sqlx::query("SELECT id, sha256, format FROM import_jobs WHERE status = 'running'").fetch_all(&db.writer).await?;
    for r in running {
        let id: String = r.get("id");
        let sha: Option<String> = r.get("sha256");
        let format: Option<String> = r.get("format");
        // Die Transaktion wurde nicht abgeschlossen; ein schon abgelegtes, nicht referenziertes Original entfällt.
        if let (Some(sha), Some(format)) = (sha, format) {
            let referenced: Option<String> =
                sqlx::query_scalar("SELECT id FROM original_files WHERE sha256 = ?").bind(&sha).fetch_optional(&db.writer).await?;
            let path = archive_path(cfg, &format!("{sha}.{format}"))?;
            if referenced.is_none() && path.exists() {
                std::fs::remove_file(&path).with_context(|| format!("{} lässt sich nicht entfernen", path.display()))?;
            }
        }
        let mut tx = db.writer.begin().await?;
        sqlx::query(
            "UPDATE import_jobs SET status = 'rolled_back', error_code = 'interrupted', \
             error_message = 'Interrupted by restart; nothing was imported', finished_at_ms = ? WHERE id = ?",
        )
        .bind(now)
        .bind(&id)
        .execute(&mut *tx)
        .await?;
        audit::record(&mut tx, Action::Import, "import_job", Some(&id), false, json!({ "error": "interrupted" })).await?;
        tx.commit().await?;
        tracing::warn!(id, "Unterbrochener Import zurückgenommen");
    }
    sqlx::query("DELETE FROM import_jobs WHERE status = 'staged'").execute(&db.writer).await?;

    let open: HashSet<String> = sqlx::query_scalar("SELECT id FROM import_jobs WHERE status = 'awaiting_decision'")
        .fetch_all(&db.writer)
        .await?
        .into_iter()
        .collect();
    let staging = cfg.staging_dir();
    for entry in std::fs::read_dir(&staging)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if entry.file_type()?.is_dir() && open.contains(&name) {
            continue;
        }
        tracing::warn!(path = %path.display(), "Staging-Rest ohne offenen Vorgang entfernt");
        if entry.file_type()?.is_dir() { std::fs::remove_dir_all(&path)? } else { std::fs::remove_file(&path)? }
    }
    cleanup(cfg, db, now).await?;
    Ok(())
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Cleaned {
    pub orphaned_imports: u64,
    pub finished_imports: u64,
    pub finished_jobs: u64,
    pub audit_entries: u64,
}

/// Aufbewahrung nach C12 und C11; läuft beim Start und stündlich.
pub async fn cleanup(cfg: &Validated, db: &Db, now: i64) -> anyhow::Result<Cleaned> {
    let mut c = Cleaned::default();
    let sql = format!("SELECT id FROM import_jobs WHERE status IN {OPEN} AND created_at_ms < ?");
    let orphans: Vec<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(sql)).bind(now - ORPHAN_AFTER_MS).fetch_all(&db.writer).await?;
    for id in orphans {
        let sql = format!("DELETE FROM import_jobs WHERE id = ? AND status IN {OPEN}");
        if sqlx::query(sqlx::AssertSqlSafe(sql)).bind(&id).execute(&db.writer).await?.rows_affected() > 0 {
            remove_staging(cfg, &id);
            c.orphaned_imports += 1;
            tracing::info!(id, "Verwaister Importvorgang entfernt");
        }
    }
    let keep = now - KEEP_FINISHED_MS;
    c.finished_imports = sqlx::query("DELETE FROM import_jobs WHERE status IN ('done', 'failed', 'rolled_back') AND finished_at_ms < ?")
        .bind(keep)
        .execute(&db.writer)
        .await?
        .rows_affected();
    c.finished_jobs = sqlx::query("DELETE FROM jobs WHERE status IN ('done', 'failed') AND finished_at_ms < ?")
        .bind(keep)
        .execute(&db.writer)
        .await?
        .rows_affected();
    c.audit_entries =
        sqlx::query("DELETE FROM audit_log WHERE at_ms < ?").bind(now - audit::RETENTION_MS).execute(&db.writer).await?.rows_affected();
    if c != Cleaned::default() {
        tracing::info!(?c, "Aufbewahrung angewendet");
    }
    Ok(c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tolerance_is_symmetric() {
        assert!(within(100.0, 105.0, 0.05));
        assert!(within(105.0, 100.0, 0.05));
        assert!(!within(100.0, 106.0, 0.05));
    }

    #[test]
    fn carry_over_defaults_to_true() {
        let c: CarryOver = serde_json::from_str(r#"{"sub_sport": false}"#).unwrap();
        assert_eq!(c, CarryOver { sub_sport: false, ..CarryOver::default() });
    }

    #[test]
    fn decision_rejects_unknown_fields() {
        let r: Result<ImportDecision, _> = serde_json::from_str(r#"{"state_token":"x","sessions":[],"extra":1}"#);
        assert!(r.is_err());
    }

    #[test]
    fn timestamps_have_millis_and_z() {
        assert_eq!(timestamp(1_782_892_800_500), "2026-07-01T08:00:00.500Z");
    }
}
