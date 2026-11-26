//! Einstellungen und Herkunft der Garmin-Aktivitäten (Migration 8).

use std::collections::HashSet;

use serde_json::{Value, json};
use sqlx::Row;

use crate::db::Db;
use crate::import_job::timestamp;
use crate::logging::hidden;

/// Versuche, nach denen eine immer wieder scheiternde Aktivität liegen bleibt (bis der Benutzer sie neu anstößt).
pub const MAX_ATTEMPTS: i64 = 5;

#[derive(Clone, Default)]
pub struct Settings {
    pub api_key: Option<String>,
    pub key_set_at_ms: Option<i64>,
    pub enabled: bool,
    pub start_date: Option<String>,
    pub interval_minutes: i64,
    pub backfill_done: bool,
    pub last_run_started_ms: Option<i64>,
    pub last_run_finished_ms: Option<i64>,
    pub last_success_ms: Option<i64>,
    pub last_error_code: Option<String>,
    pub last_error_message: Option<String>,
}

/// Ohne Schlüssel im Klartext (ADR-0013).
impl std::fmt::Debug for Settings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Settings")
            .field("api_key", &hidden(&self.api_key))
            .field("key_set_at_ms", &self.key_set_at_ms)
            .field("enabled", &self.enabled)
            .field("start_date", &self.start_date)
            .field("interval_minutes", &self.interval_minutes)
            .field("backfill_done", &self.backfill_done)
            .field("last_run_started_ms", &self.last_run_started_ms)
            .field("last_run_finished_ms", &self.last_run_finished_ms)
            .field("last_success_ms", &self.last_success_ms)
            .field("last_error_code", &self.last_error_code)
            .field("last_error_message", &self.last_error_message)
            .finish()
    }
}

pub async fn load(db: &Db) -> anyhow::Result<Settings> {
    let r = sqlx::query(
        "SELECT api_key, key_set_at_ms, enabled, start_date, interval_minutes, backfill_done, last_run_started_ms, \
         last_run_finished_ms, last_success_ms, last_error_code, last_error_message FROM garmin_settings WHERE id = 1",
    )
    .fetch_one(&db.reader)
    .await?;
    Ok(Settings {
        api_key: r.get("api_key"),
        key_set_at_ms: r.get("key_set_at_ms"),
        enabled: r.get::<i64, _>("enabled") == 1,
        start_date: r.get("start_date"),
        interval_minutes: r.get("interval_minutes"),
        backfill_done: r.get::<i64, _>("backfill_done") == 1,
        last_run_started_ms: r.get("last_run_started_ms"),
        last_run_finished_ms: r.get("last_run_finished_ms"),
        last_success_ms: r.get("last_success_ms"),
        last_error_code: r.get("last_error_code"),
        last_error_message: r.get("last_error_message"),
    })
}

/// Änderung aus der Oberfläche; `None` = unverändert, `Some(None)` = entfernen.
#[derive(Clone, Default)]
pub struct Change {
    pub api_key: Option<Option<String>>,
    pub enabled: Option<bool>,
    pub start_date: Option<Option<String>>,
    pub interval_minutes: Option<i64>,
}

/// Ohne Schlüssel im Klartext (ADR-0013).
impl std::fmt::Debug for Change {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Change")
            .field("api_key", &self.api_key.as_ref().map(hidden))
            .field("enabled", &self.enabled)
            .field("start_date", &self.start_date)
            .field("interval_minutes", &self.interval_minutes)
            .finish()
    }
}

pub async fn update(db: &Db, change: &Change, now: i64) -> anyhow::Result<()> {
    let mut tx = db.writer.begin().await?;
    let old: Option<String> = sqlx::query_scalar("SELECT start_date FROM garmin_settings WHERE id = 1").fetch_one(&mut *tx).await?;
    if let Some(key) = &change.api_key {
        // Neuer Schlüssel: alter Fehler (meist „abgelaufen“) ist erledigt.
        sqlx::query(
            "UPDATE garmin_settings SET api_key = ?, key_set_at_ms = ?, last_error_code = NULL, last_error_message = NULL WHERE id = 1",
        )
        .bind(key)
        .bind(key.as_ref().map(|_| now))
        .execute(&mut *tx)
        .await?;
    }
    if let Some(e) = change.enabled {
        sqlx::query("UPDATE garmin_settings SET enabled = ? WHERE id = 1").bind(e as i64).execute(&mut *tx).await?;
    }
    if let Some(d) = &change.start_date {
        // Früheres Anfangsdatum (oder keines mehr): der Gesamtimport muss weiter zurück.
        let earlier = match (d, &old) {
            (None, Some(_)) => true,
            (Some(new), Some(o)) => new < o,
            _ => false,
        };
        sqlx::query("UPDATE garmin_settings SET start_date = ? WHERE id = 1").bind(d).execute(&mut *tx).await?;
        if earlier {
            sqlx::query("UPDATE garmin_settings SET backfill_done = 0 WHERE id = 1").execute(&mut *tx).await?;
        }
    }
    if let Some(m) = change.interval_minutes {
        sqlx::query("UPDATE garmin_settings SET interval_minutes = ? WHERE id = 1").bind(m).execute(&mut *tx).await?;
    }
    sqlx::query("UPDATE garmin_settings SET updated_at_ms = ? WHERE id = 1").bind(now).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn run_started(db: &Db, now: i64) -> anyhow::Result<()> {
    sqlx::query("UPDATE garmin_settings SET last_run_started_ms = ? WHERE id = 1").bind(now).execute(&db.writer).await?;
    Ok(())
}

/// Ende eines Laufs; `error` = (Code, Meldung) oder Erfolg.
pub async fn run_finished(db: &Db, now: i64, error: Option<(&str, &str)>, backfill_done: bool) -> anyhow::Result<()> {
    match error {
        None => sqlx::query(
            "UPDATE garmin_settings SET last_run_finished_ms = ?, last_success_ms = ?, last_error_code = NULL, \
             last_error_message = NULL, backfill_done = max(backfill_done, ?) WHERE id = 1",
        )
        .bind(now)
        .bind(now)
        .bind(backfill_done as i64),
        Some((code, message)) => sqlx::query(
            "UPDATE garmin_settings SET last_run_finished_ms = ?, last_error_code = ?, last_error_message = ?, \
             backfill_done = max(backfill_done, ?) WHERE id = 1",
        )
        .bind(now)
        .bind(code)
        .bind(message)
        .bind(backfill_done as i64),
    }
    .execute(&db.writer)
    .await?;
    Ok(())
}

/// Garmin-IDs, die nicht erneut geholt werden: alles außer `failed` unterhalb der Versuchsgrenze.
pub async fn settled_ids(db: &Db) -> anyhow::Result<HashSet<i64>> {
    let ids: Vec<i64> = sqlx::query_scalar("SELECT garmin_id FROM garmin_activities WHERE status <> 'failed' OR attempts >= ?")
        .bind(MAX_ATTEMPTS)
        .fetch_all(&db.reader)
        .await?;
    Ok(ids.into_iter().collect())
}

/// Gescheiterte Abrufe für einen weiteren Versuch: `(garmin_id, start_ms, type_key)`.
pub async fn retryable(db: &Db) -> anyhow::Result<Vec<(i64, i64, Option<String>)>> {
    let rows = sqlx::query(
        "SELECT garmin_id, start_time_ms, type_key FROM garmin_activities WHERE status = 'failed' AND attempts < ? \
         ORDER BY start_time_ms DESC",
    )
    .bind(MAX_ATTEMPTS)
    .fetch_all(&db.reader)
    .await?;
    Ok(rows.iter().map(|r| (r.get("garmin_id"), r.get("start_time_ms"), r.get("type_key"))).collect())
}

/// Ergebnis für eine Garmin-Aktivität.
#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    pub status: &'static str,
    pub activity_id: Option<String>,
    pub reason_code: Option<String>,
    pub reason: Option<String>,
}

pub async fn record(db: &Db, garmin_id: i64, start_ms: i64, type_key: Option<&str>, o: &Outcome, now: i64) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO garmin_activities (garmin_id, start_time_ms, type_key, status, activity_id, reason_code, reason, attempts, \
         created_at_ms, updated_at_ms) VALUES (?, ?, ?, ?, ?, ?, ?, 1, ?, ?) \
         ON CONFLICT (garmin_id) DO UPDATE SET status = excluded.status, activity_id = excluded.activity_id, \
         reason_code = excluded.reason_code, reason = excluded.reason, type_key = excluded.type_key, \
         attempts = garmin_activities.attempts + 1, updated_at_ms = excluded.updated_at_ms",
    )
    .bind(garmin_id)
    .bind(start_ms)
    .bind(type_key)
    .bind(o.status)
    .bind(&o.activity_id)
    .bind(&o.reason_code)
    .bind(&o.reason)
    .bind(now)
    .bind(now)
    .execute(&db.writer)
    .await?;
    Ok(())
}

pub async fn counts(db: &Db) -> anyhow::Result<Value> {
    let mut out = json!({ "imported": 0, "linked": 0, "locked": 0, "rejected": 0, "failed": 0 });
    for r in sqlx::query("SELECT status, count(*) AS n FROM garmin_activities GROUP BY status").fetch_all(&db.reader).await? {
        out[r.get::<String, _>("status")] = json!(r.get::<i64, _>("n"));
    }
    Ok(out)
}

pub const LIST_STATUSES: &[&str] = &["imported", "linked", "locked", "rejected", "failed"];

/// Einträge der gewählten Status (leer = alle), neueste zuerst; liefert Seite und Gesamtzahl. Die Status sind
/// vorher gegen [`LIST_STATUSES`] geprüft.
pub async fn list(db: &Db, statuses: &[&str], limit: i64, offset: i64) -> anyhow::Result<(Vec<Value>, i64)> {
    let filter = if statuses.is_empty() { String::new() } else { format!("WHERE g.status IN ({})", vec!["?"; statuses.len()].join(", ")) };
    let sql = format!(
        "SELECT g.garmin_id, g.start_time_ms, g.type_key, g.status, g.activity_id, g.reason_code, g.reason, g.attempts, \
         g.updated_at_ms, a.title FROM garmin_activities g LEFT JOIN activities a ON a.id = g.activity_id {filter} \
         ORDER BY g.start_time_ms DESC, g.garmin_id DESC LIMIT ? OFFSET ?"
    );
    let count_sql = format!("SELECT count(*) FROM garmin_activities g {filter}");
    let mut q = sqlx::query(sqlx::AssertSqlSafe(sql));
    let mut c = sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(count_sql));
    for s in statuses {
        q = q.bind(*s);
        c = c.bind(*s);
    }
    let rows = q.bind(limit).bind(offset).fetch_all(&db.reader).await?;
    let total = c.fetch_one(&db.reader).await?;
    let data = rows
        .iter()
        .map(|r| {
            json!({
                // Als Text: Garmin-IDs überschreiten 2^53 nicht, bleiben aber IDs, keine Zahlen.
                "garmin_id": r.get::<i64, _>("garmin_id").to_string(),
                "start_time": timestamp(r.get("start_time_ms")),
                "type_key": r.get::<Option<String>, _>("type_key"),
                "status": r.get::<String, _>("status"),
                "activity_id": r.get::<Option<String>, _>("activity_id"),
                "activity_title": r.get::<Option<String>, _>("title"),
                "reason_code": r.get::<Option<String>, _>("reason_code"),
                "reason": r.get::<Option<String>, _>("reason"),
                "attempts": r.get::<i64, _>("attempts"),
                "updated_at": timestamp(r.get("updated_at_ms")),
            })
        })
        .collect();
    Ok((data, total))
}

/// Gesperrte, abgewiesene und aufgegebene Einträge erneut holen lassen (z. B. nach geänderten Grenzen). Der
/// Gesamtimport muss die Liste dafür noch einmal ganz lesen.
pub async fn reset_unsettled(db: &Db) -> anyhow::Result<u64> {
    let mut tx = db.writer.begin().await?;
    let n = sqlx::query("DELETE FROM garmin_activities WHERE status IN ('locked', 'rejected', 'failed')")
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if n > 0 {
        sqlx::query("UPDATE garmin_settings SET backfill_done = 0 WHERE id = 1").execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(n)
}
