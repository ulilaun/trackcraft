//! HTTP-Schicht: Router unter `/api/v1`, Browserschutz, Security-Header und (Windows) Frontend-Auslieferung.

pub mod activities;
pub mod error;
pub mod garmin;
pub mod guard;
pub mod import;
pub mod load;
pub mod profile;
pub mod seasons;
pub mod segments;
pub mod sports;
pub mod stats;

use std::path::Path;
use std::sync::Arc;

use axum::extract::{DefaultBodyLimit, State};
use axum::http::{HeaderName, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{any, get, patch, post, put};
use axum::{Json, Router, middleware};
use serde_json::json;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::set_header::SetResponseHeaderLayer;

use crate::config::Validated;
use crate::db::{Db, SCHEMA_VERSION};
use crate::version;
use error::ApiError;
use guard::GuardConfig;

pub use crate::calculation::ALGORITHM_VERSION;

/// CSP aus RFC-0011; gilt auf beiden Plattformen gleich.
pub const CSP: &str = "default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data: https:; connect-src 'self'; \
font-src 'self'; worker-src 'self'; manifest-src 'self'; object-src 'none'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'";

/// Security-Header aus RFC-0011, die das Backend setzt, wenn es das Frontend selbst ausliefert.
pub const SECURITY_HEADERS: &[(&str, &str)] = &[
    ("content-security-policy", CSP),
    ("strict-transport-security", "max-age=31536000"),
    ("x-content-type-options", "nosniff"),
    ("x-frame-options", "DENY"),
    ("referrer-policy", "strict-origin-when-cross-origin"),
    ("cross-origin-opener-policy", "same-origin"),
    ("permissions-policy", "camera=(), microphone=(), geolocation=(), payment=(), usb=()"),
];

#[derive(Clone)]
pub struct AppState {
    pub cfg: Arc<Validated>,
    pub db: Db,
    /// Garmin-Abruf
    pub garmin: Arc<crate::garmin::Runner>,
}

pub fn router(state: AppState) -> Router {
    let guard = Arc::new(GuardConfig { origins: state.cfg.origins.clone() });
    let static_dir = state.cfg.raw.server.static_dir.clone();

    let api = Router::new()
        .route("/health", get(health))
        .route("/ready", get(ready))
        .route("/version", get(version_info))
        .route("/config", get(public_config))
        .route("/profile", get(profile::get).patch(profile::update))
        // Größe und Dauer prüft der Upload selbst, mit eigenen Fehlercodes (RFC-0008 Abschnitt 2).
        .route("/activities/import", post(import::upload).layer(DefaultBodyLimit::disable()))
        .route("/activities", get(activities::list))
        .route("/activities/{id}", get(activities::get_one).patch(activities::update).delete(activities::delete))
        .route("/activities/{id}/deletion-preview", get(activities::deletion_preview))
        .route("/activities/{id}/track", get(activities::track))
        .route("/activities/{id}/efforts", get(activities::efforts))
        .route("/stats/dashboard", get(activities::dashboard))
        .route("/stats", get(stats::overview))
        .route("/stats/periods", get(stats::periods))
        .route("/segments", get(segments::list))
        .route("/segments/from-activity", post(segments::create))
        .route("/segments/similar", post(segments::similar))
        .route("/segments/{id}", get(segments::get_one).patch(segments::update).delete(segments::delete))
        .route("/segments/{id}/deletion-preview", get(segments::deletion_preview))
        .route("/segments/{id}/track", get(segments::track))
        .route("/segments/{id}/leaderboard", get(segments::leaderboard))
        .route("/segments/{id}/attempts", get(segments::attempts))
        .route("/segments/{id}/name-transfer", get(segments::name_transfer_preview).post(segments::name_transfer))
        .route("/hr-zones", get(load::list_zones).post(load::create_zone_set))
        .route("/hr-zones/{id}", patch(load::update_zone_set).delete(load::delete_zone_set))
        .route("/form", get(load::form))
        .route("/form/settings", get(load::get_settings).patch(load::update_settings))
        .route("/seasons", get(seasons::list).post(seasons::create))
        .route("/seasons/{id}", get(seasons::get_one).patch(seasons::update).delete(seasons::delete))
        .route("/sport-mappings", get(sports::list_mappings))
        .route("/sport-mappings/{source_sport}", put(sports::put_mapping).delete(sports::delete_mapping))
        .route("/sub-sports", get(sports::list_sub_sports).post(sports::create_sub_sport))
        .route("/sub-sports/{sport_class}/{key}", patch(sports::rename_sub_sport).delete(sports::delete_sub_sport))
        .route("/jobs", get(segments::list_jobs))
        .route("/jobs/{id}", get(segments::get_job))
        .route("/jobs/{id}/retry", post(segments::retry_job))
        .route("/import/jobs", get(import::list))
        .route("/garmin", get(garmin::get).patch(garmin::update))
        .route("/garmin/sync", post(garmin::sync_now))
        .route("/garmin/activities", get(garmin::list))
        .route("/garmin/activities/retry", post(garmin::retry))
        .route("/import/jobs/{id}", get(import::get_one).delete(import::cancel))
        .route("/import/jobs/{id}/confirm", post(import::confirm))
        .fallback(|| async { ApiError::NOT_FOUND })
        .method_not_allowed_fallback(|| async { ApiError::METHOD_NOT_ALLOWED })
        .with_state(state)
        // Auf API-Antworten immer, damit der Schutz nicht allein an Nginx hängt (RFC-0011).
        .layer(SetResponseHeaderLayer::overriding(header::CACHE_CONTROL, HeaderValue::from_static("no-store")))
        .layer(SetResponseHeaderLayer::overriding(
            HeaderName::from_static("cross-origin-resource-policy"),
            HeaderValue::from_static("same-origin"),
        ))
        .layer(SetResponseHeaderLayer::overriding(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff")));

    let mut app = Router::new()
        .nest(guard::API_PREFIX, api)
        // Andere API-Pfade nie auf die Single-Page-App umleiten.
        .route("/api", any(|| async { ApiError::NOT_FOUND }))
        .route("/api/{*rest}", any(|| async { ApiError::NOT_FOUND }));

    app = match &static_dir {
        Some(dir) => app.fallback_service(static_files(dir)),
        None => app.fallback(|| async { ApiError::NOT_FOUND }),
    };
    app = app.layer(middleware::from_fn_with_state(guard, guard::browser_guard));

    // Außerhalb des Browserschutzes, damit auch dessen Abweisungen die Header tragen.
    if static_dir.is_some() {
        for (name, value) in SECURITY_HEADERS {
            app = app.layer(SetResponseHeaderLayer::overriding(HeaderName::from_static(name), HeaderValue::from_static(value)));
        }
    }
    app
}

/// Frontend-Build mit Rückfall auf `index.html` für Routen der Single-Page-App.
fn static_files(dir: &Path) -> ServeDir<ServeFile> {
    ServeDir::new(dir).fallback(ServeFile::new(dir.join("index.html")))
}

async fn ready() -> Json<serde_json::Value> {
    // Der Port öffnet erst nach Migration und Profilabgleich (RFC-0015), daher ist jede Antwort „bereit“.
    Json(json!({ "data": { "ready": true } }))
}

/// Öffentliche Teilmenge der Konfiguration (RFC-0021 Abschnitt 4), keine Pfade und Betriebswerte.
async fn public_config(State(state): State<AppState>) -> Result<Json<serde_json::Value>, ApiError> {
    let c = &state.cfg.raw;
    // Untertyp-Katalog aus der Datenbank
    let catalog = crate::sports::catalog(&state.db).await?;
    // Spitzname aus der Datenbank
    let user = crate::profile::get(&state.db).await?;
    Ok(Json(json!({
        "data": {
            "timezone": state.cfg.timezone.name(),
            "user": { "display_name": user.nickname },
            "map": { "tile_url": c.maps.tile_url, "attribution": c.maps.attribution, "track_max_points": c.maps.track_max_points },
            "import_limits": {
                "max_file_size_mb": c.import.max_file_size_mb,
                "max_points_per_session": c.import.max_points_per_session,
            },
            "segments": { "minimum_length_m": c.segments.minimum_length_m, "route_edge_m": c.segments.route_edge_m },
            "sub_sport_catalog": catalog,
        }
    })))
}

async fn version_info(State(state): State<AppState>) -> Response {
    let schema = state.db.schema_version().await.unwrap_or(SCHEMA_VERSION);
    Json(json!({
        "data": {
            "app_version": version::APP_VERSION,
            "api_version": "v1",
            "schema_version": schema,
            "algorithm_version": ALGORITHM_VERSION,
        }
    }))
    .into_response()
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum CheckStatus {
    Ok,
    Warn,
    Fail,
}

impl CheckStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Warn => "warn",
            Self::Fail => "fail",
        }
    }
}

async fn health(State(state): State<AppState>) -> Response {
    let mut checks: Vec<(&str, CheckStatus, Option<String>)> = vec![("backend", CheckStatus::Ok, None)];

    checks.push(match state.db.ping().await {
        Ok(()) => ("database", CheckStatus::Ok, None),
        Err(_) => ("database", CheckStatus::Fail, Some("database not reachable".into())),
    });

    let paths = &state.cfg.raw.paths;
    let dirs = [&paths.database_dir, &paths.activities_dir, &paths.logs_dir];
    let missing = dirs.iter().any(|d| !d.is_dir());
    checks.push(if missing {
        ("filesystem", CheckStatus::Fail, Some("data directory missing".into()))
    } else {
        ("filesystem", CheckStatus::Ok, None)
    });

    let limit = (state.cfg.raw.monitoring.low_disk_free_gb * 1024.0 * 1024.0 * 1024.0) as u64;
    checks.push(match fs4::available_space(&paths.database_dir) {
        Ok(free) if free < limit => ("storage", CheckStatus::Warn, Some(format!("low disk space: {} MB free", free / 1024 / 1024))),
        Ok(_) => ("storage", CheckStatus::Ok, None),
        Err(_) => ("storage", CheckStatus::Warn, Some("free space unknown".into())),
    });

    let worst = checks.iter().map(|c| c.1).max().unwrap_or(CheckStatus::Ok);
    let (status, overall) = match worst {
        CheckStatus::Ok => (StatusCode::OK, "ok"),
        CheckStatus::Warn => (StatusCode::OK, "degraded"),
        CheckStatus::Fail => (StatusCode::SERVICE_UNAVAILABLE, "fail"),
    };
    let checks: Vec<_> =
        checks.into_iter().map(|(name, s, message)| json!({ "name": name, "status": s.as_str(), "message": message })).collect();
    (status, Json(json!({ "data": { "status": overall, "checks": checks } }))).into_response()
}
