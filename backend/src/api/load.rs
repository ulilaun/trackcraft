//! Belastung und Form (E7, OpenAPI „Load“): `/hr-zones`, `/form`, `/form/settings`.

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
use crate::load::{self, Error, Forecast, FormQuery, SettingsInput, ZoneSetInput};

fn error(e: Error) -> Response {
    match e {
        Error::NotFound => ApiError::HR_ZONE_SET_NOT_FOUND.into_response(),
        Error::Conflict => ApiError::HR_ZONE_SET_DATE_TAKEN
            .field("valid_from", "taken", "Another zone set of this sport class starts on this date")
            .into_response(),
        Error::Validation { field, code, message } => ApiError::validation(field, code, message).into_response(),
        Error::Internal(e) => ApiError::from(e).into_response(),
    }
}

pub async fn list_zones(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let sets = load::zone_sets(&state.db).await?;
    let profile = crate::profile::get(&state.db).await?;
    Ok(Json(json!({
        "data": sets.iter().map(|s| s.to_json()).collect::<Vec<_>>(),
        "meta": { "suggestion": load::suggestion(profile.birthday, activities::today(&state.cfg)) },
    })))
}

pub async fn create_zone_set(State(state): State<AppState>, body: Bytes) -> Response {
    let input: ZoneSetInput = match parse_json(&body) {
        Ok(i) => i,
        Err(e) => return e.into_response(),
    };
    match load::create_set(&state.db, &input).await {
        Ok(s) => (StatusCode::CREATED, Json(json!({ "data": s.to_json() }))).into_response(),
        Err(e) => error(e),
    }
}

pub async fn update_zone_set(State(state): State<AppState>, Path(id): Path<String>, body: Bytes) -> Response {
    let input: ZoneSetInput = match parse_json(&body) {
        Ok(i) => i,
        Err(e) => return e.into_response(),
    };
    match load::update_set(&state.db, &id, &input).await {
        Ok(s) => Json(json!({ "data": s.to_json() })).into_response(),
        Err(e) => error(e),
    }
}

pub async fn delete_zone_set(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    match load::delete_set(&state.db, &id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => error(e),
    }
}

pub async fn get_settings(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!({ "data": load::settings(&state.db).await?.to_json() })))
}

pub async fn update_settings(State(state): State<AppState>, body: Bytes) -> Response {
    let input: SettingsInput = match parse_json(&body) {
        Ok(i) => i,
        Err(e) => return e.into_response(),
    };
    match load::update_settings(&state.db, &input).await {
        Ok(s) => Json(json!({ "data": s.to_json() })).into_response(),
        Err(e) => error(e),
    }
}

fn day_param(params: &HashMap<String, String>, key: &str) -> Result<Option<chrono::NaiveDate>, ApiError> {
    params.get(key).map(|v| v.parse().map_err(|_| ApiError::validation(key, "invalid", "Date must be YYYY-MM-DD"))).transpose()
}

pub async fn form(State(state): State<AppState>, Query(params): Query<HashMap<String, String>>) -> Result<Json<Value>, ApiError> {
    if let Some(k) = params.keys().find(|k| !["from", "to", "forecast", "forecast_days"].contains(&k.as_str())) {
        return Err(ApiError::validation(k, "unknown_parameter", "Unknown query parameter"));
    }
    let (from, to) = (day_param(&params, "from")?, day_param(&params, "to")?);
    if let (Some(f), Some(t)) = (from, to)
        && f > t
    {
        return Err(ApiError::validation("from", "after_to", "Start must not be after end"));
    }
    let forecast = match params.get("forecast") {
        Some(f) => Forecast::parse(f).ok_or_else(|| ApiError::validation("forecast", "invalid", "Forecast must be rest or steady"))?,
        None => Forecast::Rest,
    };
    let forecast_days = match params.get("forecast_days") {
        Some(d) => d
            .parse::<u32>()
            .ok()
            .filter(|d| *d <= load::FORECAST_MAX_DAYS)
            .ok_or_else(|| ApiError::validation("forecast_days", "out_of_range", "Forecast days must be 0 to 90"))?,
        None => 28,
    };
    let q = FormQuery { from, to, forecast, forecast_days };
    Ok(Json(json!({ "data": load::form(&state.cfg, &state.db, &q, activities::today(&state.cfg)).await? })))
}
