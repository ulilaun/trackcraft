//! Ortsvorschlag (#22) und seine Einstellung.

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};

use super::AppState;
use super::error::ApiError;
use super::import::parse_json;
use crate::places;

/// Vorschlag für den Ort einer Aktivität aus eigenen Daten oder OpenStreetMap; `data: null` ohne Vorschlag.
pub async fn suggestion(State(state): State<AppState>, Path(id): Path<String>) -> Result<Json<Value>, ApiError> {
    match places::suggest(&state.cfg, &state.db, &id).await? {
        Some(s) => Ok(Json(json!({ "data": s }))),
        None => Err(ApiError::ACTIVITY_NOT_FOUND),
    }
}

pub async fn get_settings(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!({ "data": places::settings(&state.db).await? })))
}

pub async fn update_settings(State(state): State<AppState>, body: Bytes) -> Response {
    let input: places::SettingsInput = match parse_json(&body) {
        Ok(i) => i,
        Err(e) => return e.into_response(),
    };
    match places::update_settings(&state.db, &input).await {
        Ok(s) => Json(json!({ "data": s })).into_response(),
        Err(e) => ApiError::from(e).into_response(),
    }
}
