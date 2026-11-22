//! Aktivitäten-Endpunkte (RFC-0003, OpenAPI „Activities“) und Dashboard-Zahlen (RFC-0018).

use std::collections::HashMap;

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use chrono::NaiveDate;
use serde_json::{Value, json};

use super::AppState;
use super::error::ApiError;
use super::import::{paging, parse_json};
use crate::activities::{self, ActivityUpdate, ListQuery, SPORT_CLASSES, Sort, UpdateError};
use crate::calculation;

fn date(params: &HashMap<String, String>, key: &str) -> Result<Option<NaiveDate>, ApiError> {
    match params.get(key) {
        None => Ok(None),
        Some(v) => {
            NaiveDate::parse_from_str(v, "%Y-%m-%d").map(Some).map_err(|_| ApiError::validation(key, "invalid_date", "Expected YYYY-MM-DD"))
        }
    }
}

pub async fn list(State(state): State<AppState>, Query(params): Query<HashMap<String, String>>) -> Result<Json<Value>, ApiError> {
    let (limit, offset) = paging(&params, &["sport_class", "sub_sport", "from", "to", "q", "route_id", "sort"])?;
    let sport_class = params.get("sport_class").cloned();
    if let Some(c) = &sport_class
        && !SPORT_CLASSES.contains(&c.as_str())
    {
        return Err(ApiError::validation("sport_class", "invalid_value", "Unknown sport class"));
    }
    let q = params.get("q").cloned();
    if q.as_deref().is_some_and(str::is_empty) {
        return Err(ApiError::validation("q", "must_not_be_empty", "Search text must not be empty"));
    }
    let route_id = params.get("route_id").cloned();
    if route_id.as_deref().is_some_and(|r| uuid::Uuid::parse_str(r).is_err()) {
        return Err(ApiError::validation("route_id", "invalid_uuid", "Expected a UUID"));
    }
    let sort = match params.get("sort") {
        None => Sort::default(),
        Some(s) => Sort::parse(s).ok_or_else(|| ApiError::validation("sort", "invalid_value", "Unknown sort field"))?,
    };
    let query = ListQuery {
        sport_class,
        sub_sport: params.get("sub_sport").cloned(),
        from: date(&params, "from")?,
        to: date(&params, "to")?,
        q,
        route_id,
        sort,
        limit,
        offset,
    };
    let (data, total, summary) = activities::list(&state.cfg, &state.db, &query).await?;
    let calculation = calculation::meta(&state.db).await?;
    Ok(Json(json!({
        "data": data,
        "meta": { "total": total, "limit": limit, "offset": offset, "summary": summary, "calculation": calculation },
    })))
}

pub async fn get_one(State(state): State<AppState>, Path(id): Path<String>) -> Result<Json<Value>, ApiError> {
    match activities::get(&state.db, &id).await? {
        Some(a) => Ok(Json(json!({ "data": a }))),
        None => Err(ApiError::ACTIVITY_NOT_FOUND),
    }
}

pub async fn update(State(state): State<AppState>, Path(id): Path<String>, body: Bytes) -> Response {
    let u: ActivityUpdate = match parse_json(&body) {
        Ok(u) => u,
        Err(e) => return e.into_response(),
    };
    match activities::update(&state.cfg, &state.db, &id, &u).await {
        Ok((a, notices)) => {
            let mut out = json!({ "data": a });
            if !notices.is_empty() {
                out["meta"] = json!({ "notices": notices });
            }
            Json(out).into_response()
        }
        Err(UpdateError::NotFound) => ApiError::ACTIVITY_NOT_FOUND.into_response(),
        Err(UpdateError::Validation { field, code, message }) => ApiError::validation(field, code, message).into_response(),
        Err(UpdateError::LocalTimeAmbiguous) => {
            ApiError::LOCAL_TIME_AMBIGUOUS.field("start_time", "occurrence_required", "Local time occurs twice").into_response()
        }
        Err(UpdateError::Internal(e)) => ApiError::from(e).into_response(),
    }
}

pub async fn delete(State(state): State<AppState>, Path(id): Path<String>) -> Result<StatusCode, ApiError> {
    if activities::delete(&state.cfg, &state.db, &id).await? { Ok(StatusCode::NO_CONTENT) } else { Err(ApiError::ACTIVITY_NOT_FOUND) }
}

pub async fn deletion_preview(State(state): State<AppState>, Path(id): Path<String>) -> Result<Json<Value>, ApiError> {
    match activities::deletion_preview(&state.db, &id).await? {
        Some(p) => Ok(Json(json!({ "data": p }))),
        None => Err(ApiError::ACTIVITY_NOT_FOUND),
    }
}

pub async fn track(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    if let Some(k) = params.keys().find(|k| *k != "section" && *k != "max_points") {
        return Err(ApiError::validation(k, "unknown_parameter", "Unknown query parameter"));
    }
    let num = |k: &str, min: usize, max: usize| -> Result<Option<usize>, ApiError> {
        match params.get(k) {
            None => Ok(None),
            Some(v) => match v.parse::<usize>() {
                Ok(n) if (min..=max).contains(&n) => Ok(Some(n)),
                _ => Err(ApiError::validation(k, "out_of_range", "Value out of range")),
            },
        }
    };
    let section = num("section", 0, usize::MAX)?;
    let max_points = num("max_points", 2, 100_000)?;
    match activities::track(&state.db, &id, section, max_points).await? {
        Some(t) => Ok(Json(t)),
        None => Err(ApiError::ACTIVITY_NOT_FOUND),
    }
}

pub async fn efforts(State(state): State<AppState>, Path(id): Path<String>) -> Result<Json<Value>, ApiError> {
    let Some(data) = activities::efforts(&state.cfg, &state.db, &id).await? else {
        return Err(ApiError::ACTIVITY_NOT_FOUND);
    };
    let calculation = calculation::meta(&state.db).await?;
    Ok(Json(json!({ "data": data, "meta": { "total": data.len(), "calculation": calculation } })))
}

pub async fn dashboard(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let today = activities::today(&state.cfg);
    let data = activities::dashboard(&state.cfg, &state.db, today).await?;
    Ok(Json(json!({ "data": data })))
}
