//! Segment-Endpunkte (RFC-0003, OpenAPI „Segments“) und Auswertungsaufträge („Jobs“).

use std::collections::HashMap;

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};

use super::AppState;
use super::error::ApiError;
use super::import::{paging, parse_json};
use crate::segments::evaluation::RANKED;
use crate::segments::store::{self, CreateError, KINDS, ListQuery, Period};
use crate::{calculation, jobs, seasons};

fn create_error(e: CreateError) -> Response {
    match e {
        CreateError::ActivityNotFound => ApiError::ACTIVITY_NOT_FOUND.into_response(),
        CreateError::Validation { field, code, message } => ApiError::validation(field, code, message).into_response(),
        CreateError::TooShort => ApiError::SEGMENT_TOO_SHORT.into_response(),
        CreateError::Direction => ApiError::SEGMENT_DIRECTION_UNDETERMINABLE.into_response(),
        CreateError::RouteAlreadyMatched => ApiError::ROUTE_ALREADY_MATCHED.into_response(),
        CreateError::Internal(e) => ApiError::from(e).into_response(),
    }
}

async fn with_meta(state: &AppState, data: Value) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!({ "data": data, "meta": { "calculation": calculation::meta(&state.db).await? } })))
}

pub async fn list(State(state): State<AppState>, Query(params): Query<HashMap<String, String>>) -> Result<Json<Value>, ApiError> {
    let (limit, offset) = paging(&params, &["q", "sport_class", "kind", "category", "sort"])?;
    let q = params.get("q").cloned();
    if q.as_deref().is_some_and(str::is_empty) {
        return Err(ApiError::validation("q", "must_not_be_empty", "Search text must not be empty"));
    }
    let sport_class = params.get("sport_class").cloned();
    if sport_class.as_deref().is_some_and(|c| !RANKED.contains(&c)) {
        return Err(ApiError::validation("sport_class", "invalid_value", "Unknown sport class"));
    }
    let kind = params.get("kind").cloned();
    if kind.as_deref().is_some_and(|k| !KINDS.contains(&k)) {
        return Err(ApiError::validation("kind", "invalid_value", "Kind must be segment or route"));
    }
    let category = params.get("category").cloned();
    if category.as_deref().is_some_and(|c| !store::SEGMENT_CATEGORIES.contains(&c) && !store::ROUTE_CATEGORIES.contains(&c)) {
        return Err(ApiError::validation("category", "invalid_value", "Unknown category"));
    }
    let order = match params.get("sort") {
        None => "",
        Some(s) => store::order_by(s).ok_or_else(|| ApiError::validation("sort", "invalid_value", "Unknown sort field"))?,
    };
    let (data, total, summary) =
        store::list(&state.cfg, &state.db, &ListQuery { q, sport_class, kind, category, order, limit, offset }).await?;
    Ok(Json(json!({ "data": data, "meta": { "total": total, "limit": limit, "offset": offset, "summary": summary } })))
}

pub async fn create(State(state): State<AppState>, body: Bytes) -> Response {
    let c: store::Create = match parse_json(&body) {
        Ok(c) => c,
        Err(e) => return e.into_response(),
    };
    match store::create(&state.cfg, &state.db, &c).await {
        Ok(s) => match with_meta(&state, s).await {
            Ok(j) => (StatusCode::CREATED, j).into_response(),
            Err(e) => e.into_response(),
        },
        Err(e) => create_error(e),
    }
}

/// Strecken mit fast gleichem Verlauf wie eine Auswahl, für den Hinweis im Erstellen-Dialog.
pub async fn similar(State(state): State<AppState>, body: Bytes) -> Response {
    let q: store::SimilarQuery = match parse_json(&body) {
        Ok(q) => q,
        Err(e) => return e.into_response(),
    };
    match store::similar_routes(&state.cfg, &state.db, &q).await {
        Ok(v) => Json(json!({ "data": v, "meta": {} })).into_response(),
        Err(e) => create_error(e),
    }
}

pub async fn get_one(State(state): State<AppState>, Path(id): Path<String>) -> Result<Json<Value>, ApiError> {
    match store::get(&state.cfg, &state.db, &id).await? {
        Some(s) => with_meta(&state, s).await,
        None => Err(ApiError::SEGMENT_NOT_FOUND),
    }
}

pub async fn update(State(state): State<AppState>, Path(id): Path<String>, body: Bytes) -> Response {
    let u: store::Update = match parse_json(&body) {
        Ok(u) => u,
        Err(e) => return e.into_response(),
    };
    match store::update(&state.cfg, &state.db, &id, &u).await {
        Ok(Some(s)) => with_meta(&state, s).await.into_response(),
        Ok(None) => ApiError::SEGMENT_NOT_FOUND.into_response(),
        Err(e) => create_error(e),
    }
}

pub async fn delete(State(state): State<AppState>, Path(id): Path<String>) -> Result<StatusCode, ApiError> {
    if store::delete(&state.db, &id).await? { Ok(StatusCode::NO_CONTENT) } else { Err(ApiError::SEGMENT_NOT_FOUND) }
}

pub async fn deletion_preview(State(state): State<AppState>, Path(id): Path<String>) -> Result<Json<Value>, ApiError> {
    match store::deletion_preview(&state.db, &id).await? {
        Some(p) => Ok(Json(json!({ "data": p }))),
        None => Err(ApiError::SEGMENT_NOT_FOUND),
    }
}

pub async fn track(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    if let Some(k) = params.keys().find(|k| *k != "max_points") {
        return Err(ApiError::validation(k, "unknown_parameter", "Unknown query parameter"));
    }
    let max_points = match params.get("max_points") {
        None => None,
        Some(v) => match v.parse::<usize>() {
            Ok(n) if (2..=100_000).contains(&n) => Some(n),
            _ => return Err(ApiError::validation("max_points", "out_of_range", "Value out of range")),
        },
    };
    match store::track(&state.db, &id, max_points).await? {
        Some(t) => Ok(Json(t)),
        None => Err(ApiError::SEGMENT_NOT_FOUND),
    }
}

pub async fn leaderboard(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    let (limit, offset) = paging(&params, &["period", "year", "month", "season_id"])?;
    let num = |k: &str, min: i64, max: i64| -> Result<Option<i64>, ApiError> {
        match params.get(k) {
            None => Ok(None),
            Some(v) => match v.parse::<i64>() {
                Ok(n) if (min..=max).contains(&n) => Ok(Some(n)),
                _ => Err(ApiError::validation(k, "out_of_range", "Value out of range")),
            },
        }
    };
    let (year, month) = (num("year", 1970, 9999)?, num("month", 1, 12)?);
    let required = |k: &str| ApiError::validation(k, "required", "Required for this period");
    let period = match params.get("period").map(String::as_str) {
        None | Some("all") => Period::All,
        Some("year") => Period::Year(year.ok_or_else(|| required("year"))? as i32),
        Some("month") => Period::Month(year.ok_or_else(|| required("year"))? as i32, month.ok_or_else(|| required("month"))? as u32),
        Some("season") => {
            let sid = params.get("season_id").ok_or_else(|| required("season_id"))?;
            let s = seasons::get(&state.db, sid).await?.ok_or(ApiError::SEASON_NOT_FOUND)?;
            Period::Days(s.start, s.end)
        }
        Some(_) => return Err(ApiError::validation("period", "invalid_value", "Period must be all, year, month or season")),
    };
    let Some((data, total)) = store::leaderboard(&state.cfg, &state.db, &id, period, limit, offset).await? else {
        return Err(ApiError::SEGMENT_NOT_FOUND);
    };
    let calculation = calculation::meta(&state.db).await?;
    Ok(Json(json!({ "data": data, "meta": { "total": total, "limit": limit, "offset": offset, "calculation": calculation } })))
}

pub async fn attempts(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    let (limit, offset) = paging(&params, &["status", "sort"])?;
    let status = params.get("status").map(String::as_str);
    if status.is_some_and(|s| s != "valid" && s != "not_ranked") {
        return Err(ApiError::validation("status", "invalid_value", "Status must be valid or not_ranked"));
    }
    let order = store::attempts_order(params.get("sort").map_or("-start_time", String::as_str))
        .ok_or_else(|| ApiError::validation("sort", "invalid_value", "Unknown sort field"))?;
    let Some((data, total)) = store::attempts(&state.db, &id, status, order, limit, offset).await? else {
        return Err(ApiError::SEGMENT_NOT_FOUND);
    };
    let calculation = calculation::meta(&state.db).await?;
    Ok(Json(json!({ "data": data, "meta": { "total": total, "limit": limit, "offset": offset, "calculation": calculation } })))
}

// ---------------------------------------------------------------------------------------------------------
// Aufträge

const JOB_KINDS: [&str; 3] = ["segment_history", "activity_evaluation", "recalculation"];
const JOB_STATUSES: [&str; 4] = ["queued", "running", "done", "failed"];

pub async fn list_jobs(State(state): State<AppState>, Query(params): Query<HashMap<String, String>>) -> Result<Json<Value>, ApiError> {
    let (limit, offset) = paging(&params, &["kind", "status"])?;
    let kind = params.get("kind").map(String::as_str);
    if kind.is_some_and(|k| !JOB_KINDS.contains(&k)) {
        return Err(ApiError::validation("kind", "invalid_value", "Unknown job kind"));
    }
    let status = params.get("status").map(String::as_str);
    if status.is_some_and(|s| !JOB_STATUSES.contains(&s)) {
        return Err(ApiError::validation("status", "invalid_value", "Unknown job status"));
    }
    let (data, total) = jobs::list(&state.db, kind, status, limit, offset).await?;
    Ok(Json(json!({ "data": data, "meta": { "total": total, "limit": limit, "offset": offset } })))
}

pub async fn get_job(State(state): State<AppState>, Path(id): Path<String>) -> Result<Json<Value>, ApiError> {
    match jobs::get(&state.db, &id).await? {
        Some(j) => Ok(Json(json!({ "data": j }))),
        None => Err(ApiError::JOB_NOT_FOUND),
    }
}

pub async fn retry_job(State(state): State<AppState>, Path(id): Path<String>) -> Result<Response, ApiError> {
    match jobs::retry(&state.db, &id).await? {
        jobs::Retry::Queued(j) => Ok((StatusCode::ACCEPTED, Json(json!({ "data": j }))).into_response()),
        jobs::Retry::NotFound => Err(ApiError::JOB_NOT_FOUND),
        jobs::Retry::NotRetryable => Err(ApiError::JOB_NOT_RETRYABLE),
    }
}
