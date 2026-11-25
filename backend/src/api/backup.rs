//! Sicherungs-Endpunkte (OpenAPI „Backup“, #57): Status, Stände im Sicherungsziel, „Jetzt sichern“ und Vorgänge.
//! Das Wiederherstellen folgt in einem eigenen Schritt.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};

use super::AppState;
use super::error::ApiError;
use crate::backup::{self, Operation, format};
use crate::import_job::timestamp;

pub const OPERATION_RUNNING: ApiError =
    ApiError::new(StatusCode::CONFLICT, "operation_running", "A backup or restore operation is already running");
pub const OPERATION_NOT_FOUND: ApiError = ApiError::new(StatusCode::NOT_FOUND, "operation_not_found", "Operation not found");

fn operation_json(op: &Operation) -> Value {
    let error = op.error_code.as_ref().map(|c| json!({ "code": c, "message": op.error_message.clone().unwrap_or_default() }));
    json!({
        "id": op.id,
        "kind": op.kind,
        "trigger": op.trigger,
        "status": op.status,
        "phase": op.phase,
        "outcome": op.outcome,
        "error": error,
        "backup_id": op.backup_id,
        "started_at": timestamp(op.started_at_ms),
        "finished_at": op.finished_at_ms.map(timestamp),
    })
}

/// `GET /backups/status`
pub async fn status(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let cfg = &state.cfg;
    let s = backup::load_state(&state.db).await?;
    let current = backup::current_revision(&state.db).await?;
    let last = backup::last_attempt(&state.db).await?;
    let target = cfg.raw.backup.target_dir.clone();
    let (target_state, problem) = format::target_state(&target);
    let free_bytes = if target_state == format::TargetState::Unavailable { None } else { fs4::available_space(&target).ok() };
    let last_attempt = last.map(|op| {
        json!({
            "time": timestamp(op.finished_at_ms.unwrap_or(op.started_at_ms)),
            "outcome": op.outcome,
            "reason": op.error_message,
            "trigger": op.trigger,
            "operation_id": op.id,
        })
    });
    let last_success = s.last_success_at_ms.map(|at| {
        json!({
            "time": timestamp(at),
            "revision_id": s.last_success_revision_id,
            "backup_id": s.last_success_name,
            "change_counter": s.last_success_counter,
        })
    });
    Ok(Json(json!({ "data": {
        "last_attempt": last_attempt,
        "last_success": last_success,
        "pending_changes": backup::pending(&s, current.as_ref(), &format::parameters(cfg)),
        "running_operation_id": state.backup.active_operation(),
        "target": {
            "state": target_state,
            "free_bytes": free_bytes,
            "path": target.display().to_string(),
            "problem": problem,
        },
        "daily_time": cfg.raw.backup.daily_time,
        "keep": format::KEEP,
        "current": current.map(|(revision_id, change_counter)| json!({ "revision_id": revision_id, "change_counter": change_counter })),
    }})))
}

fn entry_json(s: &format::StandInfo) -> Value {
    let m = s.manifest.as_ref();
    json!({
        "id": s.name,
        "kind": if m.is_some_and(|m| m.trigger == "migration") { "pre_migration" } else { "regular" },
        "trigger": m.map(|m| m.trigger.clone()),
        "created_at": m.map(|m| m.created_at.clone()),
        "revision_id": m.and_then(|m| m.data_state.as_ref().map(|d| d.revision_id.clone())),
        "format_version": m.map(|m| m.format_version),
        "size_bytes": m.map_or(0, format::Manifest::total_bytes),
        // Beim Erstellen vollständig geprüft; die Liste prüft nur Vorhandensein und Größe der Dateien.
        "check_status": if s.problem.is_some() { "failed" } else { "ok" },
        "problem": s.problem,
        "activity_count": m.map_or(0, |m| m.activity_count),
        "segment_count": m.map_or(0, |m| m.segment_count),
        "original_count": m.map_or(0, format::Manifest::originals),
        "missing_original_count": m.map_or(0, |m| m.missing_originals.len()),
        "damaged_original_count": m.map_or(0, |m| m.damaged_originals.len()),
        "change_counter": m.and_then(|m| m.data_state.as_ref().map(|d| d.change_counter)),
        "schema_version": m.map(|m| m.schema_version),
        "app_version": m.map(|m| m.app_version.clone()),
    })
}

/// `GET /backups`: Stände im Sicherungsziel, neueste zuerst.
pub async fn list(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let target = state.cfg.raw.backup.target_dir.clone();
    let (target_state, _) = format::target_state(&target);
    let stands = if target_state == format::TargetState::Unavailable {
        vec![]
    } else {
        tokio::task::spawn_blocking(move || format::list(&target)).await.map_err(anyhow::Error::from)??
    };
    Ok(Json(json!({
        "data": stands.iter().map(entry_json).collect::<Vec<_>>(),
        "meta": { "target": { "state": target_state } },
    })))
}

/// `POST /backups`: „Jetzt sichern“ im Hintergrund; der Vorgang zeigt Phase und Ergebnis.
pub async fn start(State(state): State<AppState>) -> Result<Response, ApiError> {
    let Some(id) = state.backup.request(&state.db).await? else {
        return Err(OPERATION_RUNNING.with_details(json!({ "operation_id": state.backup.active_operation() })));
    };
    let op = backup::operation(&state.db, &id).await?.ok_or(ApiError::INTERNAL)?;
    Ok((StatusCode::ACCEPTED, Json(json!({ "data": operation_json(&op) }))).into_response())
}

/// `GET /operations/{id}`
pub async fn operation(State(state): State<AppState>, Path(id): Path<String>) -> Result<Json<Value>, ApiError> {
    let op = backup::operation(&state.db, &id).await?.ok_or(OPERATION_NOT_FOUND)?;
    Ok(Json(json!({ "data": operation_json(&op) })))
}
