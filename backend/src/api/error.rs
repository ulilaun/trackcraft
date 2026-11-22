//! Fehlerantworten nach `ErrorResponse` (OpenAPI), Codes aus RFC-0003 Abschnitt 7.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};

#[derive(Debug, Clone)]
pub struct ApiError {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: &'static str,
    pub details: Option<Value>,
}

impl ApiError {
    pub const fn new(status: StatusCode, code: &'static str, message: &'static str) -> Self {
        Self { status, code, message, details: None }
    }

    pub fn with_details(mut self, details: Value) -> Self {
        self.details = Some(details);
        self
    }

    /// Fehler zu einem Feld (`details.fields`).
    pub fn field(self, field: &str, code: &str, message: &str) -> Self {
        self.with_details(json!({ "fields": [{ "field": field, "code": code, "message": message }] }))
    }

    pub fn validation(field: &str, code: &str, message: &str) -> Self {
        Self::VALIDATION_FAILED.field(field, code, message)
    }

    pub const HOST_NOT_ALLOWED: Self = Self::new(StatusCode::FORBIDDEN, "host_not_allowed", "Host is not an allowed origin");
    pub const ORIGIN_NOT_ALLOWED: Self = Self::new(StatusCode::FORBIDDEN, "origin_not_allowed", "Request origin is not allowed");
    pub const UNSUPPORTED_MEDIA_TYPE: Self =
        Self::new(StatusCode::UNSUPPORTED_MEDIA_TYPE, "unsupported_media_type", "Unsupported request content type");
    pub const NOT_FOUND: Self = Self::new(StatusCode::NOT_FOUND, "not_found", "Resource not found");
    pub const METHOD_NOT_ALLOWED: Self = Self::new(StatusCode::METHOD_NOT_ALLOWED, "method_not_allowed", "Method not allowed");
    pub const INTERNAL: Self = Self::new(StatusCode::INTERNAL_SERVER_ERROR, "internal_error", "Internal error");
    pub const VALIDATION_FAILED: Self = Self::new(StatusCode::BAD_REQUEST, "validation_failed", "Request validation failed");

    // Import (RFC-0003 Abschnitt 8)
    pub const INVALID_FILE: Self = Self::new(StatusCode::BAD_REQUEST, "invalid_file", "File is damaged or unreadable");
    pub const GPX_WITHOUT_TIME: Self =
        Self::new(StatusCode::BAD_REQUEST, "gpx_without_time", "GPX file has no timestamps (planned route, not an activity)");
    pub const UNSUPPORTED_FORMAT: Self = Self::new(StatusCode::UNSUPPORTED_MEDIA_TYPE, "unsupported_format", "Neither FIT nor GPX");
    pub const FILE_TOO_LARGE: Self = Self::new(StatusCode::PAYLOAD_TOO_LARGE, "file_too_large", "File exceeds the size limit");
    pub const UPLOAD_TIMEOUT: Self = Self::new(StatusCode::REQUEST_TIMEOUT, "upload_timeout", "Upload took too long and was aborted");
    pub const INVALID_IMPORT_DECISION: Self = Self::new(StatusCode::BAD_REQUEST, "invalid_import_decision", "Import decision is not valid");
    pub const TIME_INPUT_REQUIRED: Self =
        Self::new(StatusCode::BAD_REQUEST, "time_input_required", "Start and total time are required for this session");
    pub const LOCAL_TIME_AMBIGUOUS: Self =
        Self::new(StatusCode::BAD_REQUEST, "local_time_ambiguous", "Local time occurs twice, occurrence is required");
    pub const ACTIVITY_NOT_FOUND: Self = Self::new(StatusCode::NOT_FOUND, "activity_not_found", "Activity not found");
    pub const SEGMENT_NOT_FOUND: Self = Self::new(StatusCode::NOT_FOUND, "segment_not_found", "Segment not found");
    pub const JOB_NOT_FOUND: Self = Self::new(StatusCode::NOT_FOUND, "job_not_found", "Job not found");
    pub const JOB_NOT_RETRYABLE: Self = Self::new(StatusCode::CONFLICT, "job_not_retryable", "Job has not failed");
    pub const SEGMENT_TOO_SHORT: Self =
        Self::new(StatusCode::BAD_REQUEST, "segment_too_short", "Segment is shorter than the minimum length");
    pub const ROUTE_ALREADY_MATCHED: Self =
        Self::new(StatusCode::CONFLICT, "route_already_matched", "Activity already has an attempt on a route");
    pub const SEGMENT_DIRECTION_UNDETERMINABLE: Self =
        Self::new(StatusCode::BAD_REQUEST, "segment_direction_undeterminable", "Direction at start or end cannot be determined");
    pub const HR_ZONE_SET_NOT_FOUND: Self = Self::new(StatusCode::NOT_FOUND, "hr_zone_set_not_found", "Heart rate zone set not found");
    pub const HR_ZONE_SET_DATE_TAKEN: Self =
        Self::new(StatusCode::CONFLICT, "hr_zone_set_date_taken", "Another zone set of this sport class starts on this date");
    pub const SEASON_NOT_FOUND: Self = Self::new(StatusCode::NOT_FOUND, "season_not_found", "Season not found");
    pub const SEASON_OVERLAP: Self = Self::new(StatusCode::CONFLICT, "season_overlap", "Season overlaps another season");
    pub const SEASON_NAME_TAKEN: Self = Self::new(StatusCode::CONFLICT, "season_name_taken", "Another season has this name");
    pub const SPORT_MAPPING_NOT_FOUND: Self = Self::new(StatusCode::NOT_FOUND, "sport_mapping_not_found", "Sport mapping not found");
    pub const SUB_SPORT_NOT_FOUND: Self = Self::new(StatusCode::NOT_FOUND, "sub_sport_not_found", "Sub sport not found");
    pub const SUB_SPORT_NAME_TAKEN: Self =
        Self::new(StatusCode::CONFLICT, "sub_sport_name_taken", "Sub sport already exists in this sport class");
    pub const SUB_SPORT_IN_USE: Self =
        Self::new(StatusCode::CONFLICT, "sub_sport_in_use", "Sub sport is still used by activities or mappings");
    pub const IMPORT_JOB_NOT_FOUND: Self = Self::new(StatusCode::NOT_FOUND, "import_job_not_found", "Import job not found");
    pub const IMPORT_STATE_CHANGED: Self =
        Self::new(StatusCode::CONFLICT, "import_state_changed", "Import state changed, reload and confirm again");
    pub const IMPORT_NOT_CONFIRMABLE: Self =
        Self::new(StatusCode::CONFLICT, "import_not_confirmable", "Import job is not awaiting a decision");
    pub const IMPORT_NOT_CANCELLABLE: Self =
        Self::new(StatusCode::CONFLICT, "import_not_cancellable", "Import job is already running or finished");
}

impl From<anyhow::Error> for ApiError {
    fn from(e: anyhow::Error) -> Self {
        tracing::error!(error = format!("{e:#}"), "Interner Fehler");
        Self::INTERNAL
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let mut error = json!({ "code": self.code, "message": self.message });
        if let Some(d) = self.details {
            error["details"] = d;
        }
        (self.status, Json(json!({ "error": error }))).into_response()
    }
}
