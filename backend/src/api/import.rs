//! Import-Endpunkte (RFC-0003 Abschnitt 8): Upload, Vorgänge lesen, bestätigen und abbrechen.

use std::collections::HashMap;
use std::time::Duration;

use axum::Json;
use axum::body::Bytes;
use axum::extract::multipart::{Multipart, MultipartRejection};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use super::AppState;
use super::error::ApiError;
use crate::db::new_id;
use crate::import::ImportError;
use crate::import_job::{self, CancelError, ConfirmError, CreateError, ImportDecision};

/// Spielraum für Multipart-Rahmen und Kopfzeilen über der Dateigröße.
const MULTIPART_OVERHEAD: u64 = 64 * 1024;

struct Received {
    filename: String,
    size: u64,
    sha256: String,
}

/// Nur der letzte Pfadteil, ohne Steuerzeichen, höchstens 255 Zeichen; dient nur der Anzeige (RFC-0011).
fn display_name(raw: Option<&str>) -> String {
    let base = raw.unwrap_or("").rsplit(['/', '\\']).next().unwrap_or("");
    let clean: String = base.chars().filter(|c| !c.is_control()).take(255).collect();
    let clean = clean.trim();
    if clean.is_empty() { "upload".into() } else { clean.to_string() }
}

async fn receive(mp: &mut Multipart, target: &std::path::Path, max: u64) -> Result<Received, ApiError> {
    let bad = || ApiError::validation("file", "invalid_multipart", "Expected multipart/form-data with one part 'file'");
    let mut received: Option<Received> = None;
    while let Some(mut field) = mp.next_field().await.map_err(|_| bad())? {
        if field.name() != Some("file") || received.is_some() {
            return Err(bad());
        }
        let filename = display_name(field.file_name());
        let mut file = tokio::fs::File::create(target).await.map_err(anyhow::Error::from)?;
        let mut hasher = Sha256::new();
        let mut size: u64 = 0;
        while let Some(chunk) = field.chunk().await.map_err(|_| bad())? {
            size += chunk.len() as u64;
            if size > max {
                return Err(ApiError::FILE_TOO_LARGE);
            }
            hasher.update(&chunk);
            file.write_all(&chunk).await.map_err(anyhow::Error::from)?;
        }
        file.flush().await.map_err(anyhow::Error::from)?;
        file.sync_all().await.map_err(anyhow::Error::from)?;
        let sha256 = hasher.finalize().iter().map(|b| format!("{b:02x}")).collect();
        received = Some(Received { filename, size, sha256 });
    }
    received.ok_or_else(|| ApiError::validation("file", "required", "File part 'file' is missing"))
}

pub async fn upload(State(state): State<AppState>, headers: HeaderMap, multipart: Result<Multipart, MultipartRejection>) -> Response {
    let limits = &state.cfg.raw.import;
    let max = limits.max_file_size_mb * 1024 * 1024;
    let declared = headers.get(header::CONTENT_LENGTH).and_then(|v| v.to_str().ok()).and_then(|v| v.parse::<u64>().ok());
    if declared.is_some_and(|n| n > max + MULTIPART_OVERHEAD) {
        return ApiError::FILE_TOO_LARGE.into_response();
    }
    let Ok(mut mp) = multipart else {
        return ApiError::validation("file", "invalid_multipart", "Expected multipart/form-data with one part 'file'").into_response();
    };

    let id = new_id();
    let dir = import_job::staging_dir(&state.cfg, &id);
    if let Err(e) = tokio::fs::create_dir_all(&dir).await {
        return ApiError::from(anyhow::Error::from(e)).into_response();
    }
    let timeout = Duration::from_secs(limits.upload_timeout_minutes * 60);
    let received = match tokio::time::timeout(timeout, receive(&mut mp, &import_job::upload_path(&state.cfg, &id), max)).await {
        Ok(Ok(r)) => r,
        Ok(Err(e)) => {
            import_job::remove_staging(&state.cfg, &id);
            tracing::warn!(code = e.code, "Upload abgewiesen");
            return e.into_response();
        }
        Err(_) => {
            import_job::remove_staging(&state.cfg, &id);
            tracing::warn!("Upload wegen Zeitüberschreitung abgebrochen");
            return ApiError::UPLOAD_TIMEOUT.into_response();
        }
    };

    match import_job::create(&state.cfg, &state.db, &id, &received.filename, received.size as i64, &received.sha256).await {
        Ok(job) => (StatusCode::CREATED, Json(json!({ "data": job }))).into_response(),
        Err(CreateError::Rejected(e)) => match e {
            ImportError::UnsupportedFormat => ApiError::UNSUPPORTED_FORMAT,
            ImportError::InvalidFile(_) => ApiError::INVALID_FILE,
            ImportError::GpxWithoutTime => ApiError::GPX_WITHOUT_TIME,
        }
        .into_response(),
        Err(CreateError::Internal(e)) => ApiError::from(e).into_response(),
    }
}

/// Prüft `limit`/`offset` nach RFC-0003 Abschnitt 4 und weist unbekannte Parameter ab.
pub fn paging(params: &HashMap<String, String>, allowed: &[&str]) -> Result<(i64, i64), ApiError> {
    if let Some(k) = params.keys().find(|k| !allowed.contains(&k.as_str()) && *k != "limit" && *k != "offset") {
        return Err(ApiError::validation(k, "unknown_parameter", "Unknown query parameter"));
    }
    let num = |k: &str, default: i64, min: i64, max: i64| -> Result<i64, ApiError> {
        match params.get(k) {
            None => Ok(default),
            Some(v) => match v.parse::<i64>() {
                Ok(n) if (min..=max).contains(&n) => Ok(n),
                _ => Err(ApiError::validation(k, "out_of_range", "Value out of range")),
            },
        }
    };
    Ok((num("limit", 50, 1, 200)?, num("offset", 0, 0, i64::MAX)?))
}

pub async fn list(State(state): State<AppState>, Query(params): Query<HashMap<String, String>>) -> Result<Json<Value>, ApiError> {
    let (limit, offset) = paging(&params, &["status"])?;
    let status = params.get("status").map(String::as_str);
    if let Some(s) = status
        && !import_job::STATUSES.contains(&s)
    {
        return Err(ApiError::validation("status", "invalid_value", "Unknown import status"));
    }
    let (data, total) = import_job::list(&state.cfg, &state.db, status, limit, offset).await?;
    Ok(Json(json!({ "data": data, "meta": { "total": total, "limit": limit, "offset": offset } })))
}

pub async fn get_one(State(state): State<AppState>, Path(id): Path<String>) -> Result<Json<Value>, ApiError> {
    match import_job::view(&state.cfg, &state.db, &id).await? {
        Some(job) => Ok(Json(json!({ "data": job }))),
        None => Err(ApiError::IMPORT_JOB_NOT_FOUND),
    }
}

pub async fn cancel(State(state): State<AppState>, Path(id): Path<String>) -> Result<StatusCode, ApiError> {
    match import_job::cancel(&state.cfg, &state.db, &id).await {
        Ok(()) => Ok(StatusCode::NO_CONTENT),
        Err(CancelError::NotFound) => Err(ApiError::IMPORT_JOB_NOT_FOUND),
        Err(CancelError::NotCancellable) => Err(ApiError::IMPORT_NOT_CANCELLABLE),
        Err(CancelError::Internal(e)) => Err(e.into()),
    }
}

/// JSON-Body mit eigener Fehlerform statt der Standardabweisung von axum.
pub fn parse_json<T: serde::de::DeserializeOwned>(body: &Bytes) -> Result<T, ApiError> {
    serde_json::from_slice(body).map_err(|e| {
        // Feldpfad aus der serde-Meldung ist nicht stabil; nur Zeile/Spalte, keine Werte.
        let message = format!("Invalid JSON body at line {} column {}", e.line(), e.column());
        ApiError::VALIDATION_FAILED.with_details(json!({ "fields": [{ "field": "body", "code": "invalid_json", "message": message }] }))
    })
}

pub async fn confirm(State(state): State<AppState>, Path(id): Path<String>, body: Bytes) -> Response {
    let decision: ImportDecision = match parse_json(&body) {
        Ok(d) => d,
        Err(e) => return e.into_response(),
    };
    match import_job::confirm(&state.cfg, &state.db, &id, &decision).await {
        Ok(c) => {
            let mut out = json!({ "data": c.job });
            if !c.notices.is_empty() {
                out["meta"] = json!({ "notices": c.notices });
            }
            (StatusCode::ACCEPTED, Json(out)).into_response()
        }
        Err(e) => match e {
            ConfirmError::NotFound => ApiError::IMPORT_JOB_NOT_FOUND,
            ConfirmError::NotConfirmable => ApiError::IMPORT_NOT_CONFIRMABLE,
            ConfirmError::StateChanged => ApiError::IMPORT_STATE_CHANGED,
            ConfirmError::InvalidDecision { field, message } => ApiError::INVALID_IMPORT_DECISION.field(&field, "invalid", &message),
            ConfirmError::TimeInputRequired { field } => {
                ApiError::TIME_INPUT_REQUIRED.field(&field, "required", "Start and total time required")
            }
            ConfirmError::LocalTimeAmbiguous { field } => {
                ApiError::LOCAL_TIME_AMBIGUOUS.field(&field, "occurrence_required", "Local time occurs twice")
            }
            ConfirmError::Validation { field, code, message } => ApiError::validation(&field, code, &message),
            ConfirmError::Internal(e) => ApiError::from(e),
        }
        .into_response(),
    }
}
