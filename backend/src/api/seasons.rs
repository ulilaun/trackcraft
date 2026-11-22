//! Saison-Endpunkte (RFC-0009 „Saisons“, OpenAPI „Seasons“).

use std::collections::HashMap;

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};

use super::AppState;
use super::error::ApiError;
use super::import::parse_json;
use crate::activities;
use crate::seasons::{self, Error, Input};

fn error(e: Error) -> Response {
    match e {
        Error::NotFound => ApiError::SEASON_NOT_FOUND.into_response(),
        Error::Validation { field, code, message } => ApiError::validation(field, code, message).into_response(),
        Error::NameTaken => ApiError::SEASON_NAME_TAKEN.field("name", "taken", "Another season has this name").into_response(),
        Error::Overlap(other) => ApiError::SEASON_OVERLAP
            .with_details(json!({
                "fields": [{ "field": "start_date", "code": "overlap", "message": "Period overlaps another season" }],
                "season": { "id": other.id, "name": other.name, "start_date": other.start.to_string(), "end_date": other.end.to_string() },
            }))
            .into_response(),
        Error::Internal(e) => ApiError::from(e).into_response(),
    }
}

pub async fn list(State(state): State<AppState>, Query(params): Query<HashMap<String, String>>) -> Result<Json<Value>, ApiError> {
    if let Some(k) = params.keys().next() {
        return Err(ApiError::validation(k, "unknown_parameter", "Unknown query parameter"));
    }
    let today = activities::today(&state.cfg);
    let list = seasons::list(&state.db).await?;
    let default = seasons::current_or_last(&list, today).map(|s| s.id.clone());
    Ok(Json(json!({
        "data": list.iter().map(|s| s.to_json(today)).collect::<Vec<_>>(),
        "meta": { "total": list.len(), "default_season_id": default, "today": today.to_string() },
    })))
}

pub async fn get_one(State(state): State<AppState>, Path(id): Path<String>) -> Result<Json<Value>, ApiError> {
    match seasons::get(&state.db, &id).await? {
        Some(s) => Ok(Json(json!({ "data": s.to_json(activities::today(&state.cfg)) }))),
        None => Err(ApiError::SEASON_NOT_FOUND),
    }
}

pub async fn create(State(state): State<AppState>, body: Bytes) -> Response {
    let input: Input = match parse_json(&body) {
        Ok(i) => i,
        Err(e) => return e.into_response(),
    };
    match seasons::create(&state.db, &input).await {
        Ok(s) => (StatusCode::CREATED, Json(json!({ "data": s.to_json(activities::today(&state.cfg)) }))).into_response(),
        Err(e) => error(e),
    }
}

pub async fn update(State(state): State<AppState>, Path(id): Path<String>, body: Bytes) -> Response {
    let input: Input = match parse_json(&body) {
        Ok(i) => i,
        Err(e) => return e.into_response(),
    };
    match seasons::update(&state.db, &id, &input).await {
        Ok(s) => Json(json!({ "data": s.to_json(activities::today(&state.cfg)) })).into_response(),
        Err(e) => error(e),
    }
}

pub async fn delete(State(state): State<AppState>, Path(id): Path<String>) -> Result<StatusCode, ApiError> {
    if seasons::delete(&state.db, &id).await? { Ok(StatusCode::NO_CONTENT) } else { Err(ApiError::SEASON_NOT_FOUND) }
}
