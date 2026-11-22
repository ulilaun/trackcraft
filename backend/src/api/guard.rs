//! Browserschutz nach RFC-0011: Host, Fetch-Metadaten, Origin und Content-Type, vor dem Routing.

use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{HeaderMap, Method, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use super::error::ApiError;
use crate::config::Origin;

pub const API_PREFIX: &str = "/api/v1";

#[derive(Debug)]
pub struct GuardConfig {
    pub origins: Vec<Origin>,
}

pub async fn browser_guard(State(guard): State<Arc<GuardConfig>>, req: Request<Body>, next: Next) -> Response {
    // Bei HTTP/2 steht der Host in `:authority` (URI), nicht im Header `Host`.
    let authority = req.uri().authority().map(|a| a.as_str().to_string());
    match check(&guard, req.method(), req.uri().path(), req.headers(), authority.as_deref()) {
        Ok(()) => next.run(req).await,
        Err(e) => {
            tracing::debug!(code = e.code, path = req.uri().path(), "Request abgewiesen");
            e.into_response()
        }
    }
}

/// Die Prüfungen als reine Funktion, damit sie ohne Server testbar sind.
pub fn check(guard: &GuardConfig, method: &Method, path: &str, headers: &HeaderMap, authority: Option<&str>) -> Result<(), ApiError> {
    let is_api = path == API_PREFIX || path.starts_with(&format!("{API_PREFIX}/")) || path == "/api" || path.starts_with("/api/");
    let health = method == Method::GET && (path == "/api/v1/health" || path == "/api/v1/ready");

    // 1. Host, auch für die Frontend-Dateien (Windows). Ausnahme: Healthcheck.
    let host = headers.get(header::HOST).and_then(|v| v.to_str().ok()).or(authority);
    if !health && !host_allowed(guard, host) {
        return Err(ApiError::HOST_NOT_ALLOWED);
    }
    if !is_api {
        return Ok(());
    }

    // 2. Fetch-Metadaten, auch beim Lesen
    if let Some(site) = headers.get("sec-fetch-site") {
        let site = site.to_str().unwrap_or("");
        if site != "same-origin" && site != "none" {
            return Err(ApiError::ORIGIN_NOT_ALLOWED);
        }
    }

    // 3. Origin für ändernde Methoden
    let changing = matches!(*method, Method::POST | Method::PATCH | Method::DELETE | Method::PUT);
    if changing {
        let origin = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok());
        match origin {
            Some(o) if guard.origins.iter().any(|a| a.origin == o) => {}
            _ => return Err(ApiError::ORIGIN_NOT_ALLOWED),
        }
    }

    // 4. Content-Type
    if let Some(ct) = headers.get(header::CONTENT_TYPE) {
        let ct = ct.to_str().unwrap_or("").to_ascii_lowercase();
        let mime = ct.split(';').next().unwrap_or("").trim();
        let upload = *method == Method::POST && path == "/api/v1/activities/import";
        let ok = mime == "application/json" || (upload && mime == "multipart/form-data");
        if !ok {
            return Err(ApiError::UNSUPPORTED_MEDIA_TYPE);
        }
    } else if has_body(headers) {
        return Err(ApiError::UNSUPPORTED_MEDIA_TYPE);
    }
    Ok(())
}

fn has_body(headers: &HeaderMap) -> bool {
    let len = headers.get(header::CONTENT_LENGTH).and_then(|v| v.to_str().ok()).and_then(|v| v.parse::<u64>().ok());
    matches!(len, Some(n) if n > 0) || headers.contains_key(header::TRANSFER_ENCODING)
}

/// `Host` muss Host und Port einer erlaubten Herkunft entsprechen; der Standardport 443 darf fehlen.
fn host_allowed(guard: &GuardConfig, host: Option<&str>) -> bool {
    let Some(host) = host else {
        return false;
    };
    let host = host.to_ascii_lowercase();
    let (name, port) = match host.rsplit_once(':') {
        Some((n, p)) => match p.parse::<u16>() {
            Ok(p) => (n.to_string(), p),
            Err(_) => return false,
        },
        None => (host.clone(), 443),
    };
    guard.origins.iter().any(|o| o.host == name && o.port == port)
}
