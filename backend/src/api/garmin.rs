//! Garmin-Endpunkte (OpenAPI „Garmin“): Einstellungen, Status, Abruf anstoßen, Herkunftsliste.
//! Der Schlüssel ist nur schreibbar; Antworten nennen höchstens, ob und seit wann einer gesetzt ist.

use std::collections::HashMap;

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::{Map, Value, json};

use super::AppState;
use super::error::ApiError;
use super::import::{paging, parse_json};
use crate::garmin::client::{Client, ClientError};
use crate::garmin::store::{self, Change};
use crate::import_job::timestamp;

pub const GARMIN_KEY_INVALID: ApiError =
    ApiError::new(StatusCode::BAD_REQUEST, "garmin_key_invalid", "Garmin rejected the API key (expired or invalid)");
pub const GARMIN_UNREACHABLE: ApiError =
    ApiError::new(StatusCode::BAD_GATEWAY, "garmin_unreachable", "Garmin Connect is not reachable, try again later");
pub const GARMIN_NOT_CONFIGURED: ApiError = ApiError::new(StatusCode::CONFLICT, "garmin_not_configured", "No Garmin API key is set");
pub const SECRETS_UNAVAILABLE: ApiError = ApiError::new(
    StatusCode::SERVICE_UNAVAILABLE,
    "secrets_unavailable",
    "The key file for stored secrets is missing or unreadable, the key cannot be stored",
);
pub const GARMIN_SYNC_RUNNING: ApiError = ApiError::new(StatusCode::CONFLICT, "garmin_sync_running", "A Garmin sync is already running");

async fn status(state: &AppState) -> Result<Value, ApiError> {
    let s = store::load(&state.db, &state.secrets).await?;
    let error = s.last_error_code.as_ref().map(|c| json!({ "code": c, "message": s.last_error_message.clone().unwrap_or_default() }));
    Ok(json!({
        "configured": s.api_key.is_some(),
        "key_unreadable": s.key_unreadable,
        "key_set_at": s.key_set_at_ms.map(timestamp),
        "enabled": s.enabled,
        "start_date": s.start_date,
        "interval_minutes": s.interval_minutes,
        "backfill_done": s.backfill_done,
        "running": state.garmin.is_running(),
        "progress": state.garmin.progress(),
        "last_run_started_at": s.last_run_started_ms.map(timestamp),
        "last_run_finished_at": s.last_run_finished_ms.map(timestamp),
        "last_success_at": s.last_success_ms.map(timestamp),
        "last_error": error,
        "counts": store::counts(&state.db).await?,
    }))
}

pub async fn get(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!({ "data": status(&state).await? })))
}

fn bad(field: &str, code: &str, message: &str) -> ApiError {
    ApiError::validation(field, code, message)
}

/// Liest die Änderung streng: nur bekannte Felder, `null` entfernt Schlüssel bzw. Anfangsdatum.
fn parse_change(body: &Map<String, Value>) -> Result<Change, ApiError> {
    let mut c = Change::default();
    for (k, v) in body {
        match k.as_str() {
            "api_key" => {
                c.api_key = Some(match v {
                    Value::Null => None,
                    Value::String(s) => {
                        let s = s.trim();
                        if s.len() < 8 || s.len() > 4096 || s.chars().any(|ch| ch.is_whitespace() || ch.is_control()) {
                            return Err(bad("api_key", "invalid_format", "API key looks wrong"));
                        }
                        Some(s.to_string())
                    }
                    _ => return Err(bad("api_key", "invalid_type", "String or null expected")),
                })
            }
            "enabled" => c.enabled = Some(v.as_bool().ok_or_else(|| bad("enabled", "invalid_type", "Boolean expected"))?),
            "start_date" => {
                c.start_date = Some(match v {
                    Value::Null => None,
                    Value::String(s) if chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").is_ok() && s.len() == 10 => Some(s.clone()),
                    _ => return Err(bad("start_date", "invalid_format", "Expected YYYY-MM-DD or null")),
                })
            }
            "interval_minutes" => {
                let m = v.as_i64().filter(|m| (5..=1440).contains(m));
                c.interval_minutes = Some(m.ok_or_else(|| bad("interval_minutes", "out_of_range", "Between 5 and 1440"))?);
            }
            other => return Err(bad(other, "unknown_field", "Unknown field")),
        }
    }
    Ok(c)
}

pub async fn update(State(state): State<AppState>, body: Bytes) -> Response {
    let body: Map<String, Value> = match parse_json(&body) {
        Ok(b) => b,
        Err(e) => return e.into_response(),
    };
    let change = match parse_change(&body) {
        Ok(c) => c,
        Err(e) => return e.into_response(),
    };
    if matches!(change.api_key, Some(Some(_))) && !state.secrets.is_available() {
        return SECRETS_UNAVAILABLE.into_response();
    }
    // Neuen Schlüssel sofort prüfen, damit ein Tippfehler nicht erst beim Abruf auffällt.
    if let Some(Some(key)) = &change.api_key {
        let client = match Client::new(&state.cfg.raw.garmin.api_base_url) {
            Ok(c) => c,
            Err(e) => return ApiError::from(e).into_response(),
        };
        match client.token(key).await {
            Ok(_) => {}
            Err(ClientError::KeyInvalid) => {
                return GARMIN_KEY_INVALID.field("api_key", "rejected", "Garmin rejected the key").into_response();
            }
            Err(e) => {
                tracing::warn!(code = e.code(), "Garmin-Schlüssel nicht prüfbar: {}", e.message());
                return GARMIN_UNREACHABLE.into_response();
            }
        }
    }
    if let Err(e) = store::update(&state.db, &state.secrets, &change, chrono::Utc::now().timestamp_millis()).await {
        return ApiError::from(e).into_response();
    }
    tracing::info!(
        key_changed = change.api_key.is_some(),
        enabled = ?change.enabled,
        start_date = ?change.start_date,
        interval = ?change.interval_minutes,
        "Garmin-Einstellungen geändert"
    );
    match status(&state).await {
        Ok(s) => Json(json!({ "data": s })).into_response(),
        Err(e) => e.into_response(),
    }
}

/// Lauf sofort anstoßen (auch wenn der regelmäßige Abruf ausgeschaltet ist).
pub async fn sync_now(State(state): State<AppState>) -> Result<(StatusCode, Json<Value>), ApiError> {
    if store::load(&state.db, &state.secrets).await?.api_key.is_none() {
        return Err(GARMIN_NOT_CONFIGURED);
    }
    if !state.garmin.request() {
        return Err(GARMIN_SYNC_RUNNING);
    }
    Ok((StatusCode::ACCEPTED, Json(json!({ "data": status(&state).await? }))))
}

pub async fn list(State(state): State<AppState>, Query(params): Query<HashMap<String, String>>) -> Result<Json<Value>, ApiError> {
    let (limit, offset) = paging(&params, &["status"])?;
    // Ein Status oder mehrere mit Komma, z. B. `locked,rejected,failed`
    let statuses: Vec<&str> = params.get("status").map(|s| s.split(',').collect()).unwrap_or_default();
    if statuses.iter().any(|s| !store::LIST_STATUSES.contains(s)) {
        return Err(bad("status", "invalid_value", "Unknown status"));
    }
    let (data, total) = store::list(&state.db, &statuses, limit, offset).await?;
    Ok(Json(json!({ "data": data, "meta": { "total": total, "limit": limit, "offset": offset } })))
}

/// Gesperrte, abgewiesene und aufgegebene Einträge vergessen; der nächste Lauf holt sie erneut.
pub async fn retry(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    if state.garmin.is_running() {
        return Err(GARMIN_SYNC_RUNNING);
    }
    let n = store::reset_unsettled(&state.db).await?;
    Ok(Json(json!({ "data": { "reset": n } })))
}
