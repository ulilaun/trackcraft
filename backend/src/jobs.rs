//! Auswertungsaufträge (Tabelle `jobs`, RFC-0007): ein Worker arbeitet die Warteschlange nacheinander ab.
//! Aufträge entstehen beim Import (`activity_evaluation`), beim Anlegen eines Segments (`segment_history`) und
//! nach Änderungen, die Wertungen betreffen. Eine Neuberechnung mit neuem Stand läuft beim Start (calculation).

use std::time::Duration;

use serde_json::{Value, json};
use sqlx::Row;

use crate::config::Validated;
use crate::db::Db;
use crate::import_job::timestamp;
use crate::segments::evaluation;

/// Abstand, in dem der Worker nach neuen Aufträgen sieht.
const POLL: Duration = Duration::from_secs(1);

/// Beim Start: Aufträge, die beim Beenden liefen, erneut einreihen.
pub async fn recover(db: &Db) -> sqlx::Result<u64> {
    Ok(sqlx::query("UPDATE jobs SET status = 'queued', started_at_ms = NULL WHERE status = 'running'")
        .execute(&db.writer)
        .await?
        .rows_affected())
}

/// Läuft bis zum Abbruch der Aufgabe.
pub async fn worker(cfg: std::sync::Arc<Validated>, db: Db) {
    loop {
        match run_next(&cfg, &db).await {
            Ok(true) => continue,
            Ok(false) => tokio::time::sleep(POLL).await,
            Err(e) => {
                tracing::error!(error = format!("{e:#}"), "Auftragswarteschlange gestört");
                tokio::time::sleep(POLL * 5).await;
            }
        }
    }
}

/// Arbeitet alle wartenden Aufträge ab (Tests, Kommandozeile).
pub async fn run_all(cfg: &Validated, db: &Db) -> anyhow::Result<u64> {
    let mut n = 0;
    while run_next(cfg, db).await? {
        n += 1;
    }
    Ok(n)
}

/// Nimmt den nächsten Auftrag und führt ihn aus; `false`, wenn keiner wartet.
pub async fn run_next(cfg: &Validated, db: &Db) -> anyhow::Result<bool> {
    let now = chrono::Utc::now().timestamp_millis();
    let Some(job) = sqlx::query(
        "UPDATE jobs SET status = 'running', started_at_ms = ?, error_message = NULL \
         WHERE id = (SELECT id FROM jobs WHERE status = 'queued' ORDER BY priority, created_at_ms, id LIMIT 1) \
         RETURNING id, kind, segment_id, activity_id",
    )
    .bind(now)
    .fetch_optional(&db.writer)
    .await?
    else {
        return Ok(false);
    };
    let id: String = job.get("id");
    let kind: String = job.get("kind");
    let result = match kind.as_str() {
        "activity_evaluation" => match job.get::<Option<String>, _>("activity_id") {
            Some(a) => {
                sqlx::query("UPDATE activities SET processing_status = 'running' WHERE id = ?").bind(&a).execute(&db.writer).await?;
                let r = evaluation::evaluate_activity(cfg, db, &a).await;
                if r.is_err() {
                    sqlx::query("UPDATE activities SET processing_status = 'failed' WHERE id = ?").bind(&a).execute(&db.writer).await?;
                }
                r
            }
            None => Ok(()),
        },
        "segment_history" => match job.get::<Option<String>, _>("segment_id") {
            Some(s) => {
                let (jid, w) = (id.clone(), db.writer.clone());
                evaluation::segment_history(cfg, db, &s, move |done, total| {
                    let (jid, w) = (jid.clone(), w.clone());
                    async move {
                        sqlx::query("UPDATE jobs SET processed_items = ?, total_items = ? WHERE id = ?")
                            .bind(done as i64)
                            .bind(total as i64)
                            .bind(&jid)
                            .execute(&w)
                            .await?;
                        Ok(())
                    }
                })
                .await
            }
            None => Ok(()),
        },
        // Neuberechnungen laufen beim Start (calculation::ensure_published)
        _ => Ok(()),
    };
    let finished = chrono::Utc::now().timestamp_millis();
    match result {
        Ok(()) => {
            sqlx::query(
                "UPDATE jobs SET status = 'done', finished_at_ms = ?, processed_items = max(processed_items, total_items) WHERE id = ?",
            )
            .bind(finished)
            .bind(&id)
            .execute(&db.writer)
            .await?;
        }
        Err(e) => {
            tracing::error!(job = id, kind, error = format!("{e:#}"), "Auswertung fehlgeschlagen");
            sqlx::query("UPDATE jobs SET status = 'failed', finished_at_ms = ?, error_message = ? WHERE id = ?")
                .bind(finished)
                .bind(format!("{e:#}"))
                .bind(&id)
                .execute(&db.writer)
                .await?;
        }
    }
    Ok(true)
}

fn job_json(r: &sqlx::sqlite::SqliteRow) -> Value {
    json!({
        "id": r.get::<String, _>("id"),
        "kind": r.get::<String, _>("kind"),
        "status": r.get::<String, _>("status"),
        "segment_id": r.get::<Option<String>, _>("segment_id"),
        "activity_id": r.get::<Option<String>, _>("activity_id"),
        "processed_items": r.get::<i64, _>("processed_items"),
        "total_items": r.get::<i64, _>("total_items"),
        "error_message": r.get::<Option<String>, _>("error_message"),
        "created_at": timestamp(r.get("created_at_ms")),
        "started_at": r.get::<Option<i64>, _>("started_at_ms").map(timestamp),
        "finished_at": r.get::<Option<i64>, _>("finished_at_ms").map(timestamp),
    })
}

const COLUMNS: &str = "SELECT id, kind, status, segment_id, activity_id, processed_items, total_items, error_message, \
    created_at_ms, started_at_ms, finished_at_ms FROM jobs";

pub async fn list(db: &Db, kind: Option<&str>, status: Option<&str>, limit: i64, offset: i64) -> anyhow::Result<(Vec<Value>, i64)> {
    let filter = "(? IS NULL OR kind = ?) AND (? IS NULL OR status = ?)";
    let total: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT count(*) FROM jobs WHERE {filter}")))
        .bind(kind)
        .bind(kind)
        .bind(status)
        .bind(status)
        .fetch_one(&db.reader)
        .await?;
    let rows = sqlx::query(sqlx::AssertSqlSafe(format!("{COLUMNS} WHERE {filter} ORDER BY created_at_ms DESC, id DESC LIMIT ? OFFSET ?")))
        .bind(kind)
        .bind(kind)
        .bind(status)
        .bind(status)
        .bind(limit)
        .bind(offset)
        .fetch_all(&db.reader)
        .await?;
    Ok((rows.iter().map(job_json).collect(), total))
}

pub async fn get(db: &Db, id: &str) -> anyhow::Result<Option<Value>> {
    let r = sqlx::query(sqlx::AssertSqlSafe(format!("{COLUMNS} WHERE id = ?"))).bind(id).fetch_optional(&db.reader).await?;
    Ok(r.as_ref().map(job_json))
}

pub enum Retry {
    Queued(Value),
    NotFound,
    NotRetryable,
}

/// Fehlgeschlagenen Auftrag erneut einreihen (ohne Neuimport, RFC-0017).
pub async fn retry(db: &Db, id: &str) -> anyhow::Result<Retry> {
    let done = sqlx::query(
        "UPDATE jobs SET status = 'queued', error_message = NULL, started_at_ms = NULL, finished_at_ms = NULL, processed_items = 0 \
         WHERE id = ? AND status = 'failed'",
    )
    .bind(id)
    .execute(&db.writer)
    .await?;
    if done.rows_affected() == 1 {
        return Ok(Retry::Queued(get(db, id).await?.expect("eben eingereiht")));
    }
    Ok(if get(db, id).await?.is_some() { Retry::NotRetryable } else { Retry::NotFound })
}
