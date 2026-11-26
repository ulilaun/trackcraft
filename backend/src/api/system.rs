//! `GET /system` (#69): Betriebszustand für die Systemseite nach RFC-0013 „Zustände“ und „Warnungen“.

use std::sync::OnceLock;
use std::time::Instant;

use axum::Json;
use axum::extract::State;
use serde_json::{Value, json};
use sqlx::Row;

use super::AppState;
use super::error::ApiError;
use crate::backup::{self, format};
use crate::import_job::timestamp;

/// Warnung `certificate_expiring` so viele Tage vor Ablauf (RFC-0013).
pub const CERT_WARN_DAYS: i64 = 30;

const DAY_MS: i64 = 24 * 60 * 60 * 1000;

static STARTED: OnceLock<(Instant, i64)> = OnceLock::new();

/// Merkt den Start des Prozesses; ein interner Neustart (Wiederherstellen) zählt nicht als neuer Start.
pub fn mark_started() {
    STARTED.get_or_init(|| (Instant::now(), chrono::Utc::now().timestamp_millis()));
}

struct Warning {
    code: &'static str,
    message: String,
    since_ms: i64,
}

/// `GET /system`
pub async fn get(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!({ "data": system(&state).await? })))
}

async fn system(state: &AppState) -> anyhow::Result<Value> {
    mark_started();
    let (started, started_ms) = *STARTED.get().expect("eben gesetzt");
    let now_ms = chrono::Utc::now().timestamp_millis();
    let cfg = &state.cfg;
    let db = &state.db;
    let mut warnings = Vec::new();

    // Zähler: Segmente und Strecken zusammen, Versuche nur gültige im veröffentlichten Stand.
    let activities: i64 = sqlx::query_scalar("SELECT count(*) FROM activities").fetch_one(&db.reader).await?;
    let segments: i64 = sqlx::query_scalar("SELECT count(*) FROM segments").fetch_one(&db.reader).await?;
    let efforts: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM segment_efforts e JOIN calculation_versions v ON v.id = e.calculation_version_id \
         WHERE v.status = 'published' AND e.status = 'valid'",
    )
    .fetch_one(&db.reader)
    .await?;

    // Speicher: Datenbereich und Sicherungsziel, Grenze aus `[monitoring] low_disk_free_gb`.
    let limit = (cfg.raw.monitoring.low_disk_free_gb * 1024.0 * 1024.0 * 1024.0) as u64;
    let data_free = fs4::available_space(&cfg.raw.paths.database_dir).ok();
    let target = &cfg.raw.backup.target_dir;
    let (target_state, _) = format::target_state(target);
    let target_free = if target_state == format::TargetState::Unavailable { None } else { fs4::available_space(target).ok() };
    for (name, free) in [("data", data_free), ("backup target", target_free)] {
        if let Some(free) = free.filter(|f| *f < limit) {
            warnings.push(Warning {
                code: "low_disk_space",
                message: format!("Low disk space on {name}: {} MB free", free / 1024 / 1024),
                since_ms: now_ms,
            });
        }
    }

    // Zertifikat aus `[tls]`; ohne TLS (Docker) null.
    let cert_expires_ms = match &cfg.raw.tls {
        Some(tls) => std::fs::read_to_string(&tls.cert_file)
            .ok()
            .and_then(|pem| crate::cert::inspect(&pem).ok())
            .map(|info| (info.not_after.unix_timestamp_nanos() / 1_000_000) as i64),
        None => None,
    };
    if let Some(expires) = cert_expires_ms {
        let warn_from = expires - CERT_WARN_DAYS * DAY_MS;
        if now_ms >= warn_from {
            let message = if now_ms >= expires {
                "TLS certificate has expired".to_string()
            } else {
                format!("TLS certificate expires in {} days", (expires - now_ms + DAY_MS - 1) / DAY_MS)
            };
            warnings.push(Warning { code: "certificate_expiring", message, since_ms: warn_from });
        }
    }

    // Sicherung: letzter Versuch fehlgeschlagen, ungesicherte Änderungen seit der ersten ungesicherten Revision.
    if let Some(op) = backup::last_attempt(db).await?
        && op.outcome.as_deref() == Some("failed")
    {
        warnings.push(Warning {
            code: "backup_failed",
            message: op.error_message.unwrap_or_else(|| "Last backup failed".into()),
            since_ms: op.finished_at_ms.unwrap_or(op.started_at_ms),
        });
    }
    let s = backup::load_state(db).await?;
    let current = backup::current_revision(db).await?;
    if current.is_some() && backup::pending(&s, current.as_ref(), &format::parameters(cfg)) {
        let first: Option<i64> = sqlx::query_scalar("SELECT min(created_at_ms) FROM revision_history WHERE change_counter > ?")
            .bind(s.last_success_counter.unwrap_or(-1))
            .fetch_one(&db.reader)
            .await?;
        // Ohne verzeichnete Revision (nur geänderte Parameter) gilt die letzte Sicherung als Beginn.
        let since_ms = first.or(s.last_success_at_ms).unwrap_or(now_ms);
        warnings.push(Warning { code: "backup_pending", message: "Changes not yet backed up".into(), since_ms });
    }

    // Aufträge
    let row = sqlx::query(
        "SELECT coalesce(sum(status = 'queued'), 0) AS queued, coalesce(sum(status = 'running'), 0) AS running, \
         coalesce(sum(status = 'failed'), 0) AS failed, min(CASE WHEN status = 'failed' THEN finished_at_ms END) AS failed_since \
         FROM jobs",
    )
    .fetch_one(&db.reader)
    .await?;
    let (queued, running, failed): (i64, i64, i64) = (row.get("queued"), row.get("running"), row.get("failed"));
    if failed > 0 {
        let since: Option<i64> = row.get("failed_since");
        warnings.push(Warning { code: "job_failed", message: format!("{failed} failed job(s)"), since_ms: since.unwrap_or(now_ms) });
    }

    let warnings: Vec<Value> =
        warnings.into_iter().map(|w| json!({ "code": w.code, "message": w.message, "since": timestamp(w.since_ms) })).collect();
    Ok(json!({
            "uptime_s": started.elapsed().as_secs(),
            "started_at": timestamp(started_ms),
            "platform": crate::platform::kind(),
            "tzdata_version": chrono_tz::IANA_TZDB_VERSION,
            "tls_certificate_expires_at": cert_expires_ms.map(timestamp),
            "counts": { "activities": activities, "segments": segments, "efforts": efforts },
            "storage": { "data_free_bytes": data_free.unwrap_or(0), "backup_target_free_bytes": target_free },
            "calculation": crate::calculation::meta(db).await?,
            "jobs": { "queued": queued, "running": running, "failed": failed },
            "warnings": warnings,
    }))
}
