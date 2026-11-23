//! Trainingsplanung (E8, OpenAPI „Plan“): `/plan`, `/plan/units`, `/plan/goals`, `/activities/{id}/plan`.

use std::collections::HashMap;

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::{Value, json};

use super::AppState;
use super::error::ApiError;
use super::import::parse_json;
use crate::activities;
use crate::plan::{self, Error, GoalInput, Scope, UnitInput};
use crate::plan_assist::{self, Input as SuggestionInput};

fn error(e: Error) -> Response {
    match e {
        Error::NotFound => ApiError::PLANNED_UNIT_NOT_FOUND.into_response(),
        Error::GoalNotFound => ApiError::PLAN_GOAL_NOT_FOUND.into_response(),
        Error::ActivityNotFound => ApiError::ACTIVITY_NOT_FOUND.into_response(),
        Error::ActivityTaken => ApiError::ACTIVITY_ALREADY_PLANNED.into_response(),
        Error::Validation { field, code, message } => ApiError::validation(field, code, message).into_response(),
        Error::Internal(e) => ApiError::from(e).into_response(),
    }
}

fn data(r: Result<Value, Error>) -> Response {
    match r {
        Ok(v) => Json(json!({ "data": v })).into_response(),
        Err(e) => error(e),
    }
}

/// Einheiten im Zeitraum, Vorgabe: eine Woche zurück bis vier Wochen voraus.
pub async fn list(State(state): State<AppState>, Query(params): Query<HashMap<String, String>>) -> Result<Json<Value>, ApiError> {
    if let Some(k) = params.keys().find(|k| *k != "from" && *k != "to") {
        return Err(ApiError::validation(k, "unknown_parameter", "Unknown query parameter"));
    }
    let today = activities::today(&state.cfg);
    let day = |k: &str, default: chrono::NaiveDate| -> Result<chrono::NaiveDate, ApiError> {
        match params.get(k) {
            None => Ok(default),
            Some(v) => v.parse().map_err(|_| ApiError::validation(k, "invalid", "Date must be YYYY-MM-DD")),
        }
    };
    let from = day("from", today - chrono::Days::new(7))?;
    let to = day("to", today + chrono::Days::new(28))?;
    if from > to || (to - from).num_days() > 400 {
        return Err(ApiError::validation("to", "out_of_range", "Range must be 0 to 400 days"));
    }
    let mut v = plan::list(&state.db, from, to).await?;
    v["today"] = json!(today.to_string());
    Ok(Json(json!({ "data": v })))
}

pub async fn create(State(state): State<AppState>, body: Bytes) -> Response {
    let input: UnitInput = match parse_json(&body) {
        Ok(i) => i,
        Err(e) => return e.into_response(),
    };
    match plan::create(&state.db, &input).await {
        Ok(v) => (StatusCode::CREATED, Json(json!({ "data": v }))).into_response(),
        Err(e) => error(e),
    }
}

pub async fn get_one(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    data(plan::get_one(&state.db, &id).await)
}

/// `?scope=this|following` (Vorgabe `this`): bei Einheiten einer Serie auch die offenen danach.
fn scope(params: &HashMap<String, String>) -> Result<Scope, ApiError> {
    if let Some(k) = params.keys().find(|k| *k != "scope") {
        return Err(ApiError::validation(k, "unknown_parameter", "Unknown query parameter"));
    }
    Scope::parse(params.get("scope").map(String::as_str))
        .map_err(|_| ApiError::validation("scope", "invalid", "Scope must be this or following"))
}

pub async fn update(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
    body: Bytes,
) -> Response {
    let scope = match scope(&params) {
        Ok(s) => s,
        Err(e) => return e.into_response(),
    };
    let input: UnitInput = match parse_json(&body) {
        Ok(i) => i,
        Err(e) => return e.into_response(),
    };
    data(plan::update(&state.db, &id, &input, scope).await)
}

pub async fn delete(State(state): State<AppState>, Path(id): Path<String>, Query(params): Query<HashMap<String, String>>) -> Response {
    let scope = match scope(&params) {
        Ok(s) => s,
        Err(e) => return e.into_response(),
    };
    match plan::delete(&state.db, &id, scope).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => error(e),
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivityRef {
    activity_id: String,
}

pub async fn link(State(state): State<AppState>, Path(id): Path<String>, body: Bytes) -> Response {
    let input: ActivityRef = match parse_json(&body) {
        Ok(i) => i,
        Err(e) => return e.into_response(),
    };
    data(plan::link(&state.db, &id, &input.activity_id).await)
}

pub async fn unlink(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    data(plan::unlink(&state.db, &id).await)
}

pub async fn decline(State(state): State<AppState>, Path(id): Path<String>, body: Bytes) -> Response {
    let input: ActivityRef = match parse_json(&body) {
        Ok(i) => i,
        Err(e) => return e.into_response(),
    };
    match plan::decline(&state.db, &id, &input.activity_id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => error(e),
    }
}

/// Alle Ziele nach Datum.
pub async fn goals(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!({ "data": plan::goals_json(&state.db).await? })))
}

pub async fn create_goal(State(state): State<AppState>, body: Bytes) -> Response {
    let input: GoalInput = match parse_json(&body) {
        Ok(i) => i,
        Err(e) => return e.into_response(),
    };
    match plan::create_goal(&state.db, &input).await {
        Ok(v) => (StatusCode::CREATED, Json(json!({ "data": v }))).into_response(),
        Err(e) => error(e),
    }
}

pub async fn get_goal(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    data(plan::goal(&state.db, &id).await.map(|g| g.to_json()))
}

pub async fn update_goal(State(state): State<AppState>, Path(id): Path<String>, body: Bytes) -> Response {
    let input: GoalInput = match parse_json(&body) {
        Ok(i) => i,
        Err(e) => return e.into_response(),
    };
    data(plan::update_goal(&state.db, &id, &input).await)
}

pub async fn delete_goal(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    match plan::delete_goal(&state.db, &id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => error(e),
    }
}

/// Für die Aktivitätsseite: verknüpfte Einheit und Vorschläge.
pub async fn for_activity(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    data(plan::for_activity(&state.cfg, &state.db, &id).await)
}

/// Planungshilfe (Stufe 2): Vorschlag bis zum gewählten Ziel berechnen, ohne zu speichern.
pub async fn suggestion(State(state): State<AppState>, body: Bytes) -> Response {
    let input: SuggestionInput = match parse_json(&body) {
        Ok(i) => i,
        Err(e) => return e.into_response(),
    };
    let today = activities::today(&state.cfg);
    data(plan_assist::preview(&state.cfg, &state.db, &input, today).await)
}

/// Planungshilfe: Vorschlag neu berechnen und als normale Einheiten anlegen.
pub async fn apply_suggestion(State(state): State<AppState>, body: Bytes) -> Response {
    let input: SuggestionInput = match parse_json(&body) {
        Ok(i) => i,
        Err(e) => return e.into_response(),
    };
    let today = activities::today(&state.cfg);
    match plan_assist::apply(&state.cfg, &state.db, &input, today).await {
        Ok(v) => (StatusCode::CREATED, Json(json!({ "data": v }))).into_response(),
        Err(e) => error(e),
    }
}
