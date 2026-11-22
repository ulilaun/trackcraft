//! Sportart-Zuordnung und Untertyp-Katalog (OpenAPI „Sports“).

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
use crate::sports::{self, Error, MappingInput, SubSportInput};

fn error(e: Error) -> Response {
    match e {
        Error::MappingNotFound => ApiError::SPORT_MAPPING_NOT_FOUND.into_response(),
        Error::SubSportNotFound => ApiError::SUB_SPORT_NOT_FOUND.into_response(),
        Error::Validation { field, code, message } => ApiError::validation(field, code, message).into_response(),
        Error::NameTaken => {
            ApiError::SUB_SPORT_NAME_TAKEN.field("name", "taken", "Sub sport already exists in this sport class").into_response()
        }
        Error::InUse { activity_count, mapping_count } => ApiError::SUB_SPORT_IN_USE
            .with_details(json!({ "activity_count": activity_count, "mapping_count": mapping_count }))
            .into_response(),
        Error::Internal(e) => ApiError::from(e).into_response(),
    }
}

fn no_params(params: &HashMap<String, String>) -> Result<(), ApiError> {
    match params.keys().next() {
        Some(k) => Err(ApiError::validation(k, "unknown_parameter", "Unknown query parameter")),
        None => Ok(()),
    }
}

pub async fn list_mappings(State(state): State<AppState>, Query(params): Query<HashMap<String, String>>) -> Result<Json<Value>, ApiError> {
    no_params(&params)?;
    let rows = sports::list_mappings(&state.db).await?;
    Ok(Json(json!({ "data": rows.iter().map(|r| r.to_json()).collect::<Vec<_>>(), "meta": { "total": rows.len() } })))
}

pub async fn put_mapping(State(state): State<AppState>, Path(source): Path<String>, body: Bytes) -> Response {
    let input: MappingInput = match parse_json(&body) {
        Ok(i) => i,
        Err(e) => return e.into_response(),
    };
    match sports::put_mapping(&state.db, &source, &input).await {
        Ok(r) => Json(json!({ "data": r.to_json() })).into_response(),
        Err(e) => error(e),
    }
}

pub async fn delete_mapping(State(state): State<AppState>, Path(source): Path<String>) -> Response {
    match sports::delete_mapping(&state.db, &source).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => error(Error::MappingNotFound),
        Err(e) => error(e.into()),
    }
}

pub async fn list_sub_sports(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    no_params(&params)?;
    let rows = sports::list_sub_sports(&state.db).await?;
    Ok(Json(json!({ "data": rows.iter().map(|r| r.to_json()).collect::<Vec<_>>(), "meta": { "total": rows.len() } })))
}

pub async fn create_sub_sport(State(state): State<AppState>, body: Bytes) -> Response {
    let input: SubSportInput = match parse_json(&body) {
        Ok(i) => i,
        Err(e) => return e.into_response(),
    };
    match sports::create_sub_sport(&state.db, &input).await {
        Ok(r) => (StatusCode::CREATED, Json(json!({ "data": r.to_json() }))).into_response(),
        Err(e) => error(e),
    }
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenameInput {
    /// eigener Name; `null` setzt einen eingebauten Untertyp auf die übersetzte Bezeichnung zurück
    #[serde(default, deserialize_with = "crate::activities::nullable")]
    name: Option<Option<String>>,
    /// Symbol aus dem festen Satz; `null` = Vorgabe
    #[serde(default, deserialize_with = "crate::activities::nullable")]
    icon: Option<Option<String>>,
}

pub async fn rename_sub_sport(State(state): State<AppState>, Path((class, key)): Path<(String, String)>, body: Bytes) -> Response {
    let input: RenameInput = match parse_json(&body) {
        Ok(i) => i,
        Err(e) => return e.into_response(),
    };
    if input.name.is_none() && input.icon.is_none() {
        return ApiError::validation("name", "required", "name or icon is required (null resets to the default)").into_response();
    }
    let name = input.name.as_ref().map(|n| n.as_deref());
    let icon = input.icon.as_ref().map(|i| i.as_deref());
    match sports::update_sub_sport(&state.db, &class, &key, name, icon).await {
        Ok(r) => Json(json!({ "data": r.to_json() })).into_response(),
        Err(e) => error(e),
    }
}

pub async fn delete_sub_sport(State(state): State<AppState>, Path((class, key)): Path<(String, String)>) -> Response {
    match sports::delete_sub_sport(&state.db, &class, &key).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => error(e),
    }
}
