//! Statistik-Endpunkte (OpenAPI „Statistics“, RFC-0009, RFC-0006 „Statistik“).

use std::collections::HashMap;

use axum::Json;
use axum::extract::{Query, State};
use chrono::NaiveDate;
use serde_json::{Value, json};

use super::AppState;
use super::error::ApiError;
use crate::activities::{self, SPORT_CLASSES};
use crate::stats::{self, Filter};
use crate::{calculation, seasons};

fn filter(params: &HashMap<String, String>, allowed: &[&str]) -> Result<Filter, ApiError> {
    if let Some(k) = params.keys().find(|k| !allowed.contains(&k.as_str())) {
        return Err(ApiError::validation(k, "unknown_parameter", "Unknown query parameter"));
    }
    let date = |k: &str| -> Result<Option<NaiveDate>, ApiError> {
        match params.get(k) {
            None => Ok(None),
            Some(v) if v.len() == 10 => NaiveDate::parse_from_str(v, "%Y-%m-%d")
                .map(Some)
                .map_err(|_| ApiError::validation(k, "invalid_date", "Expected YYYY-MM-DD")),
            Some(_) => Err(ApiError::validation(k, "invalid_date", "Expected YYYY-MM-DD")),
        }
    };
    let (from, to) = (date("from")?, date("to")?);
    if let (Some(a), Some(b)) = (from, to)
        && b < a
    {
        return Err(ApiError::validation("to", "before_start", "End date must not be before start date"));
    }
    let sport_class = params.get("sport_class").cloned();
    if sport_class.as_deref().is_some_and(|c| !SPORT_CLASSES.contains(&c)) {
        return Err(ApiError::validation("sport_class", "invalid_value", "Unknown sport class"));
    }
    let sub_sport = params.get("sub_sport").cloned();
    if sub_sport.as_deref().is_some_and(str::is_empty) {
        return Err(ApiError::validation("sub_sport", "must_not_be_empty", "Sub sport must not be empty"));
    }
    Ok(Filter { from, to, sport_class, sub_sport })
}

/// Kennzahlen, Balken je Monat und ISO-Woche und Rekorde für Zeitraum, Sportklasse und Untertyp.
pub async fn overview(State(state): State<AppState>, Query(params): Query<HashMap<String, String>>) -> Result<Json<Value>, ApiError> {
    let f = filter(&params, &["from", "to", "sport_class", "sub_sport"])?;
    let data = stats::overview(&state.cfg, &state.db, &f).await?;
    let calculation = calculation::meta(&state.db).await?;
    Ok(Json(json!({ "data": data, "meta": { "calculation": calculation } })))
}

/// Kurzsummen je Saison und Jahr für die Kacheln der Statistikseite.
pub async fn periods(State(state): State<AppState>, Query(params): Query<HashMap<String, String>>) -> Result<Json<Value>, ApiError> {
    let f = filter(&params, &["sport_class", "sub_sport"])?;
    let today = activities::today(&state.cfg);
    let list = seasons::list(&state.db).await?;
    let data = stats::periods(&state.cfg, &state.db, &f, &list, today).await?;
    Ok(Json(json!({ "data": data })))
}
