//! Ein Abruflauf: Liste seitenweise lesen, unbekannte Aktivitäten laden, entpacken und ohne Rückfrage
//! durch die Importstrecke schicken. Gleiche Datei oder Duplikatkandidat → nur verknüpfen (Uli 14.11.2026);
//! gesperrte Sessions bleiben mit Grund stehen. Der Garmin-Typ ersetzt den `source_sport` der Datei.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use chrono::{NaiveDate, TimeZone};
use serde_json::{Value, json};

use super::client::{self, Client, ClientError};
use super::store::{self, Outcome};
use super::{Progress, Runner};
use crate::config::Validated;
use crate::db::{Db, new_id};
use crate::import_job::{self, ConfirmError, CreateError, DecisionAction, ImportDecision, SessionDecision};

/// Seitengröße der Liste.
const PAGE: usize = 100;
/// So viele Downloads hintereinander dürfen am Netz scheitern, dann bricht der Lauf ab.
const MAX_NETWORK_FAILURES: u32 = 3;

/// Abbruch eines ganzen Laufs.
#[derive(Debug)]
pub enum RunError {
    NotConfigured,
    Client(ClientError),
    Internal(anyhow::Error),
}

impl<E: Into<anyhow::Error>> From<E> for RunError {
    fn from(e: E) -> Self {
        Self::Internal(e.into())
    }
}

impl RunError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotConfigured => "garmin_not_configured",
            Self::Client(c) => c.code(),
            Self::Internal(_) => "internal_error",
        }
    }

    pub fn message(&self) -> String {
        match self {
            Self::NotConfigured => "Kein Garmin-Schlüssel eingetragen.".into(),
            Self::Client(c) => c.message(),
            Self::Internal(_) => "Interner Fehler beim Abruf; Einzelheiten im Log.".into(),
        }
    }
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// Beginn des Anfangsdatums in der Konfigurationszone.
pub fn start_limit_ms(date: Option<&str>, tz: chrono_tz::Tz) -> Option<i64> {
    let d = NaiveDate::parse_from_str(date?, "%Y-%m-%d").ok()?;
    let local = d.and_hms_opt(0, 0, 0)?;
    tz.from_local_datetime(&local).earliest().map(|t| t.timestamp_millis())
}

/// Führt einen Lauf aus und hält Fortschritt und Status fest.
pub async fn run(cfg: &Arc<Validated>, db: &Db, runner: &Runner) -> Result<Progress, RunError> {
    let settings = store::load(db).await?;
    let Some(key) = settings.api_key.clone() else {
        return Err(RunError::NotConfigured);
    };
    store::run_started(db, now_ms()).await?;
    runner.set_progress(Progress { phase: "token", ..Default::default() });
    let mut backfill_done = false;
    let result = run_inner(cfg, db, runner, &key, &settings, &mut backfill_done).await;
    let progress = runner.progress();
    match &result {
        Ok(()) => {
            store::run_finished(db, now_ms(), None, backfill_done).await?;
            tracing::info!(
                imported = progress.imported,
                linked = progress.linked,
                locked = progress.locked,
                rejected = progress.rejected,
                failed = progress.failed,
                "Garmin-Abruf beendet"
            );
        }
        Err(e) => {
            if let RunError::Internal(err) = e {
                tracing::error!(error = format!("{err:#}"), "Garmin-Abruf abgebrochen");
            } else {
                tracing::warn!(code = e.code(), "Garmin-Abruf abgebrochen: {}", e.message());
            }
            store::run_finished(db, now_ms(), Some((e.code(), &e.message())), backfill_done).await?;
        }
    }
    result.map(|()| progress)
}

async fn run_inner(
    cfg: &Arc<Validated>,
    db: &Db,
    runner: &Runner,
    key: &str,
    settings: &store::Settings,
    backfill_done: &mut bool,
) -> Result<(), RunError> {
    let client = Client::new(&cfg.raw.garmin.api_base_url)?;
    let token = client.token(key).await.map_err(RunError::Client)?;
    let limit = start_limit_ms(settings.start_date.as_deref(), cfg.timezone);
    let settled = store::settled_ids(db).await?;

    // Erst die Liste lesen, dann laden: so steht die Gesamtzahl für den Fortschritt fest.
    runner.update(|p| p.phase = "list");
    let (mut todo, list_complete) = collect(&client, &token, &settled, limit, settings.backfill_done).await?;
    // Früher gescheiterte Abrufe, die die Liste diesmal nicht erreicht hat.
    let mut queued: HashSet<i64> = todo.iter().map(|t| t.0).collect();
    for (id, begin_ms, type_key) in store::retryable(db).await? {
        if !settled.contains(&id) && limit.is_none_or(|l| begin_ms >= l) && queued.insert(id) {
            todo.push((id, begin_ms, type_key));
        }
    }
    runner.update(|p| p.total = todo.len() as u64);

    let mut ctx = Ctx { cfg, db, runner, client: &client, token: &token, network_failures: 0 };
    for (id, begin_ms, type_key) in &todo {
        ctx.process(*id, *begin_ms, type_key.as_deref()).await?;
    }
    // Erst wenn alles aus der vollständig gelesenen Liste bearbeitet ist, gilt der Gesamtimport als fertig.
    *backfill_done = list_complete;
    Ok(())
}

/// Noch zu bearbeitende Einträge der Liste (neueste zuerst) und ob die Liste bis zum Ende bzw. bis zum
/// Anfangsdatum gelesen wurde. Nach abgeschlossenem Gesamtimport genügt es, bis zur ersten Seite ohne Neues zu lesen.
async fn collect(
    client: &Client,
    token: &str,
    settled: &HashSet<i64>,
    limit: Option<i64>,
    backfill_done: bool,
) -> Result<(Vec<(i64, i64, Option<String>)>, bool), RunError> {
    let mut todo = vec![];
    let mut seen = HashSet::new();
    let mut start = 0;
    loop {
        let page = client.list(token, start, PAGE).await.map_err(RunError::Client)?;
        if page.is_empty() {
            return Ok((todo, true));
        }
        let mut anything_new = false;
        let mut reached_limit = false;
        for e in &page {
            if limit.is_some_and(|l| e.begin_ms < l) {
                reached_limit = true;
                continue;
            }
            if settled.contains(&e.activity_id) || !seen.insert(e.activity_id) {
                continue;
            }
            anything_new = true;
            todo.push((e.activity_id, e.begin_ms, e.type_key().map(str::to_string)));
        }
        if reached_limit {
            return Ok((todo, true));
        }
        if backfill_done && !anything_new {
            return Ok((todo, false));
        }
        start += page.len();
    }
}

struct Ctx<'a> {
    cfg: &'a Arc<Validated>,
    db: &'a Db,
    runner: &'a Runner,
    client: &'a Client,
    token: &'a str,
    network_failures: u32,
}

impl Ctx<'_> {
    /// Holt und importiert eine Aktivität. Fehler des Schlüssels oder der Rate brechen den Lauf ab, andere
    /// werden an der Aktivität vermerkt.
    async fn process(&mut self, garmin_id: i64, begin_ms: i64, type_key: Option<&str>) -> Result<(), RunError> {
        let delay = self.cfg.raw.garmin.request_delay_ms;
        if delay > 0 {
            tokio::time::sleep(Duration::from_millis(delay)).await;
        }
        self.runner.update(|p| p.phase = "download");
        let max = self.cfg.raw.import.max_file_size_mb * 1024 * 1024;
        let outcome = match self.client.download(self.token, garmin_id, max).await {
            Err(e @ (ClientError::KeyInvalid | ClientError::RateLimited)) => return Err(RunError::Client(e)),
            Err(e @ ClientError::Network(_)) => {
                self.network_failures += 1;
                if self.network_failures >= MAX_NETWORK_FAILURES {
                    return Err(RunError::Client(e));
                }
                failed(&e)
            }
            Err(e @ ClientError::NotFound) => rejected(e.code(), e.message()),
            Err(e) => failed(&e),
            Ok(bytes) => {
                self.network_failures = 0;
                match client::extract_fit(&bytes, max) {
                    Err(msg) => rejected("invalid_archive", msg),
                    Ok(fit) => {
                        self.runner.update(|p| p.phase = "import");
                        import(self.cfg, self.db, garmin_id, fit, type_key).await?
                    }
                }
            }
        };
        store::record(self.db, garmin_id, begin_ms, type_key, &outcome, now_ms()).await?;
        self.runner.update(|p| {
            p.processed += 1;
            match outcome.status {
                "imported" => p.imported += 1,
                "linked" => p.linked += 1,
                "locked" => p.locked += 1,
                "rejected" => p.rejected += 1,
                _ => p.failed += 1,
            }
        });
        Ok(())
    }
}

fn failed(e: &ClientError) -> Outcome {
    Outcome { status: "failed", activity_id: None, reason_code: Some(e.code().into()), reason: Some(e.message()) }
}

fn rejected(code: &str, reason: String) -> Outcome {
    Outcome { status: "rejected", activity_id: None, reason_code: Some(code.into()), reason: Some(reason) }
}

/// Importiert eine geladene FIT-Datei ohne Rückfrage über die normale Importstrecke.
pub async fn import(cfg: &Arc<Validated>, db: &Db, garmin_id: i64, fit: Vec<u8>, type_key: Option<&str>) -> anyhow::Result<Outcome> {
    let id = new_id();
    let dir = import_job::staging_dir(cfg, &id);
    tokio::fs::create_dir_all(&dir).await?;
    let sha = import_job::sha256_hex(&fit);
    let size = fit.len() as i64;
    if let Err(e) = tokio::fs::write(import_job::upload_path(cfg, &id), &fit).await {
        import_job::remove_staging(cfg, &id);
        return Err(e.into());
    }
    let filename = format!("garmin-{garmin_id}.fit");
    let view = match import_job::create_with(cfg, db, &id, &filename, size, &sha, type_key).await {
        Ok(v) => v,
        Err(CreateError::Rejected(e)) => return Ok(rejected(e.code(), e.to_string())),
        Err(CreateError::Internal(e)) => return Err(e),
    };
    // Eine Wiederholung, falls sich zwischen Anlegen und Bestätigen etwas ändert (etwa ein paralleler Import).
    let mut view = view;
    for attempt in 0..2 {
        let plan = plan(&view)?;
        match import_job::confirm(cfg, db, &id, &plan.decision).await {
            Ok(done) => return Ok(plan.outcome(&done.job)),
            Err(ConfirmError::StateChanged) if attempt == 0 => {
                view = import_job::view(cfg, db, &id).await?.ok_or_else(|| anyhow::anyhow!("Vorgang {id} fehlt"))?;
            }
            Err(e) => {
                let _ = import_job::cancel(cfg, db, &id).await;
                return Err(anyhow::anyhow!("Bestätigen fehlgeschlagen: {e:?}"));
            }
        }
    }
    unreachable!("die Schleife endet mit return")
}

struct Plan {
    decision: ImportDecision,
    linked: Option<String>,
    locked: Option<(String, String)>,
}

impl Plan {
    fn outcome(&self, job: &Value) -> Outcome {
        let imported = job["imported_activity_ids"].as_array().and_then(|a| a.first()).and_then(Value::as_str).map(str::to_string);
        if job["status"] != "done" {
            let e = &job["error"];
            return Outcome {
                status: "failed",
                activity_id: None,
                reason_code: e["code"].as_str().map(str::to_string),
                reason: e["message"].as_str().map(str::to_string),
            };
        }
        if let Some(id) = imported {
            return Outcome { status: "imported", activity_id: Some(id), reason_code: None, reason: None };
        }
        if let Some(id) = &self.linked {
            return Outcome { status: "linked", activity_id: Some(id.clone()), reason_code: None, reason: None };
        }
        let (code, reason) = self.locked.clone().unwrap_or_else(|| ("no_session".into(), "Keine Session übernommen.".into()));
        Outcome { status: "locked", activity_id: None, reason_code: Some(code), reason: Some(reason) }
    }
}

/// Entscheidung je Session aus der Ansicht des Vorgangs.
fn plan(view: &Value) -> anyhow::Result<Plan> {
    let token = view["state_token"].as_str().ok_or_else(|| anyhow::anyhow!("Vorgang ohne state_token"))?.to_string();
    let mut sessions = vec![];
    let (mut linked, mut locked) = (None, None);
    for s in view["sessions"].as_array().into_iter().flatten() {
        let index = s["session_index"].as_i64().ok_or_else(|| anyhow::anyhow!("Session ohne Index"))?;
        let candidate = s["duplicate_candidates"].as_array().and_then(|c| c.first()).and_then(|c| c["activity_id"].as_str());
        let action = match s["state"].as_str() {
            Some("selectable") if s["time_input_required"] == json!(true) => {
                locked.get_or_insert(("no_start_time".to_string(), "Die Datei enthält keine Startzeit.".to_string()));
                DecisionAction::Skip
            }
            Some("selectable") if candidate.is_some() => {
                linked.get_or_insert(candidate.unwrap_or_default().to_string());
                DecisionAction::Skip
            }
            Some("selectable") => DecisionAction::Import,
            Some("locked_existing") => {
                if let Some(id) = s["existing_activity_id"].as_str() {
                    linked.get_or_insert(id.to_string());
                }
                DecisionAction::Skip
            }
            other => {
                let code = other.unwrap_or("locked").to_string();
                let reason = s["lock_reason"].as_str().unwrap_or("Session gesperrt.").to_string();
                locked.get_or_insert((code, reason));
                DecisionAction::Skip
            }
        };
        sessions.push(SessionDecision {
            session_index: index,
            action,
            replace_activity_id: None,
            title: None,
            carry_over: None,
            start_time: None,
            total_time_ms: None,
        });
    }
    Ok(Plan { decision: ImportDecision { state_token: token, sessions }, linked, locked })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_limit_is_local_midnight() {
        let tz: chrono_tz::Tz = "Europe/Berlin".parse().unwrap();
        // 01.01.2020 00:00 MEZ = 31.12.2019 23:00 UTC
        assert_eq!(start_limit_ms(Some("2020-01-01"), tz), Some(1_577_833_200_000));
        assert_eq!(start_limit_ms(None, tz), None);
        assert_eq!(start_limit_ms(Some("kein Datum"), tz), None);
    }

    #[test]
    fn plan_imports_links_and_locks() {
        let view = json!({
            "state_token": "st_x",
            "sessions": [
                { "session_index": 0, "state": "selectable", "time_input_required": false, "duplicate_candidates": [] },
                { "session_index": 1, "state": "selectable", "time_input_required": false,
                  "duplicate_candidates": [{ "activity_id": "a1" }] },
                { "session_index": 2, "state": "locked_vehicle", "lock_reason": "Auto", "duplicate_candidates": [] },
                { "session_index": 3, "state": "locked_existing", "existing_activity_id": "a2", "duplicate_candidates": [] },
            ],
        });
        let p = plan(&view).unwrap();
        let actions: Vec<_> = p.decision.sessions.iter().map(|s| s.action).collect();
        assert_eq!(actions, [DecisionAction::Import, DecisionAction::Skip, DecisionAction::Skip, DecisionAction::Skip]);
        assert_eq!(p.linked.as_deref(), Some("a1"));
        assert_eq!(p.locked, Some(("locked_vehicle".into(), "Auto".into())));
        let o = p.outcome(&json!({ "status": "done", "imported_activity_ids": ["n1"] }));
        assert_eq!((o.status, o.activity_id.as_deref()), ("imported", Some("n1")));
        let o = p.outcome(&json!({ "status": "done", "imported_activity_ids": [] }));
        assert_eq!((o.status, o.activity_id.as_deref()), ("linked", Some("a1")));
    }
}
