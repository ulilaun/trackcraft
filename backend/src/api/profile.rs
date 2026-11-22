//! Profil-Endpunkte (OpenAPI „Profile“): `GET/PATCH /profile`.

use axum::Json;
use axum::body::Bytes;
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};

use super::AppState;
use super::error::ApiError;
use super::import::parse_json;
use crate::activities;
use crate::profile::{self, Error, Input};

pub async fn get(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!({ "data": profile::get(&state.db).await?.to_json() })))
}

pub async fn update(State(state): State<AppState>, body: Bytes) -> Response {
    let input: Input = match parse_json(&body) {
        Ok(i) => i,
        Err(e) => return e.into_response(),
    };
    match profile::update(&state.db, &input, activities::today(&state.cfg)).await {
        Ok(p) => Json(json!({ "data": p.to_json() })).into_response(),
        Err(Error::Validation { field, code, message }) => ApiError::validation(field, code, message).into_response(),
        Err(Error::Internal(e)) => ApiError::from(e).into_response(),
    }
}
