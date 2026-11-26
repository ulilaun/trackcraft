//! Browserschutz, Security-Header und Systemendpunkte (RFC-0011, RFC-0013, RFC-0014 Fall 35).

mod common;

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use trackcraft::api::{self, AppState, SECURITY_HEADERS};
use trackcraft::config;
use trackcraft::startup;

const HOST: &str = "trackcraft.test";
const ORIGIN: &str = "https://trackcraft.test";

struct Fixture {
    _dir: tempfile::TempDir,
    app: Router,
    db: trackcraft::db::Db,
}

async fn fixture(with_static: bool) -> Fixture {
    let dir = common::temp();
    let mut text = common::config_text(dir.path(), "", "");
    if with_static {
        let web = dir.path().join("web");
        std::fs::create_dir_all(web.join("assets")).unwrap();
        std::fs::write(web.join("index.html"), "<!doctype html><title>TrackCraft</title>").unwrap();
        std::fs::write(web.join("assets/app.js"), "console.log(1)").unwrap();
        let server = format!("static_dir = {}", common::lit(&web));
        let tls =
            format!("[tls]\ncert_file = {}\nkey_file = {}", common::lit(&dir.path().join("c.pem")), common::lit(&dir.path().join("k.pem")));
        text = common::config_text(dir.path(), &server, &tls);
    }
    let cfg = config::parse(&dir.path().join("t.toml"), &text).unwrap();
    let started = startup::prepare(&cfg).await.unwrap();
    let db = started.db.clone();
    // Sperre freigeben, damit der Temp-Ordner aufgeräumt werden kann; die Verbindungen bleiben offen.
    drop(started);
    let app = api::router(AppState {
        cfg: Arc::new(cfg),
        db: db.clone(),
        garmin: Default::default(),
        backup: Default::default(),
        secrets: Default::default(),
    });
    Fixture { _dir: dir, app, db }
}

async fn send(app: &Router, req: Request<Body>) -> (StatusCode, axum::http::HeaderMap, serde_json::Value) {
    let (status, headers, bytes) = common::contract::call(app, req).await;
    let json = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::String(String::from_utf8_lossy(&bytes).into()));
    (status, headers, json)
}

fn get(path: &str) -> axum::http::request::Builder {
    Request::builder().method(Method::GET).uri(path).header("host", HOST)
}

fn code(v: &serde_json::Value) -> &str {
    v["error"]["code"].as_str().unwrap_or("")
}

#[tokio::test]
async fn version_and_ready() {
    let f = fixture(false).await;
    let (s, h, v) = send(&f.app, get("/api/v1/version").body(Body::empty()).unwrap()).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["data"]["api_version"], "v1");
    assert_eq!(v["data"]["schema_version"], trackcraft::db::SCHEMA_VERSION);
    assert_eq!(h["cache-control"], "no-store");
    assert_eq!(h["cross-origin-resource-policy"], "same-origin");
    assert_eq!(h["x-content-type-options"], "nosniff");
    assert!(h.get("access-control-allow-origin").is_none());

    let (s, _, v) = send(&f.app, get("/api/v1/ready").body(Body::empty()).unwrap()).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["data"]["ready"], true);
    f.db.close().await;
}

#[tokio::test]
async fn config_is_public_subset() {
    let f = fixture(false).await;
    let (s, _, v) = send(&f.app, get("/api/v1/config").body(Body::empty()).unwrap()).await;
    assert_eq!(s, StatusCode::OK);
    let d = &v["data"];
    assert_eq!(d["timezone"], "Europe/Berlin");
    assert_eq!(d["user"]["display_name"], "Testperson");
    assert_eq!(d["import_limits"]["max_file_size_mb"], 50);
    assert_eq!(d["map"]["track_max_points"], 2000);
    assert_eq!(
        d["sub_sport_catalog"]["cycling"],
        serde_json::json!([{"key": "road", "name": null, "icon": null, "builtin": true}, {"key": "mtb", "name": null, "icon": null, "builtin": true}, {"key": "indoor", "name": null, "icon": null, "builtin": true}])
    );
    // Keine Pfade oder Betriebswerte
    let text = v.to_string();
    assert!(!text.contains("database") && !text.contains("allowed_origins") && !text.contains("backup"), "{text}");
    f.db.close().await;
}

#[tokio::test]
async fn health_reports_checks() {
    let f = fixture(false).await;
    let (s, _, v) = send(&f.app, get("/api/v1/health").body(Body::empty()).unwrap()).await;
    assert_eq!(s, StatusCode::OK);
    let names: Vec<&str> = v["data"]["checks"].as_array().unwrap().iter().map(|c| c["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["backend", "database", "filesystem", "storage"]);
    assert!(["ok", "degraded"].contains(&v["data"]["status"].as_str().unwrap()));
    f.db.close().await;
}

#[tokio::test]
async fn foreign_host_is_rejected_except_health() {
    let f = fixture(false).await;
    let req = Request::get("/api/v1/version").header("host", "evil.example").body(Body::empty()).unwrap();
    let (s, _, v) = send(&f.app, req).await;
    assert_eq!((s, code(&v)), (StatusCode::FORBIDDEN, "host_not_allowed"));

    let req = Request::get("/api/v1/version").header("host", "trackcraft.test:8443").body(Body::empty()).unwrap();
    assert_eq!(send(&f.app, req).await.0, StatusCode::FORBIDDEN, "falscher Port");
    let req = Request::get("/api/v1/version").header("host", "trackcraft.test:443").body(Body::empty()).unwrap();
    assert_eq!(send(&f.app, req).await.0, StatusCode::OK, "Standardport ausgeschrieben");
    let req = Request::get("/api/v1/version").header("host", "pc.home.arpa:8443").body(Body::empty()).unwrap();
    assert_eq!(send(&f.app, req).await.0, StatusCode::OK, "zweite Herkunft mit Port");

    for path in ["/api/v1/health", "/api/v1/ready"] {
        let req = Request::get(path).header("host", "172.17.0.3:8080").body(Body::empty()).unwrap();
        assert_eq!(send(&f.app, req).await.0, StatusCode::OK, "{path} für Healthcheck");
    }
    let req = Request::post("/api/v1/health").header("host", "evil.example").body(Body::empty()).unwrap();
    assert_eq!(send(&f.app, req).await.0, StatusCode::FORBIDDEN, "Ausnahme nur für GET");
    f.db.close().await;
}

#[tokio::test]
async fn cross_site_fetch_is_rejected_even_for_reads() {
    let f = fixture(false).await;
    for site in ["cross-site", "same-site"] {
        let req = get("/api/v1/version").header("sec-fetch-site", site).body(Body::empty()).unwrap();
        let (s, _, v) = send(&f.app, req).await;
        assert_eq!((s, code(&v)), (StatusCode::FORBIDDEN, "origin_not_allowed"), "{site}");
    }
    for site in ["same-origin", "none"] {
        let req = get("/api/v1/version").header("sec-fetch-site", site).body(Body::empty()).unwrap();
        assert_eq!(send(&f.app, req).await.0, StatusCode::OK, "{site}");
    }
    f.db.close().await;
}

#[tokio::test]
async fn changing_requests_need_allowed_origin_and_json() {
    let f = fixture(false).await;
    let post = |origin: Option<&str>, ct: Option<&str>, body: &'static str| {
        let mut b = Request::post("/api/v1/activities/42").header("host", HOST);
        if let Some(o) = origin {
            b = b.header("origin", o);
        }
        if let Some(c) = ct {
            b = b.header("content-type", c);
        }
        b.header("content-length", body.len()).body(Body::from(body)).unwrap()
    };
    let (s, _, v) = send(&f.app, post(None, Some("application/json"), "{}")).await;
    assert_eq!((s, code(&v)), (StatusCode::FORBIDDEN, "origin_not_allowed"), "ohne Origin");
    let (s, _, v) = send(&f.app, post(Some("https://evil.example"), Some("application/json"), "{}")).await;
    assert_eq!((s, code(&v)), (StatusCode::FORBIDDEN, "origin_not_allowed"), "fremder Origin");
    for ct in ["text/plain", "application/x-www-form-urlencoded", "multipart/form-data; boundary=x"] {
        let (s, _, v) = send(&f.app, post(Some(ORIGIN), Some(ct), "a=b")).await;
        assert_eq!((s, code(&v)), (StatusCode::UNSUPPORTED_MEDIA_TYPE, "unsupported_media_type"), "{ct}");
    }
    let (s, _, v) = send(&f.app, post(Some(ORIGIN), None, "a=b")).await;
    assert_eq!((s, code(&v)), (StatusCode::UNSUPPORTED_MEDIA_TYPE, "unsupported_media_type"), "Body ohne Content-Type");
    // Erlaubter Request kommt bis zum Routing (POST ist für eine Aktivität nicht vorgesehen).
    let (s, _, v) = send(&f.app, post(Some(ORIGIN), Some("application/json; charset=utf-8"), "{}")).await;
    assert_eq!((s, code(&v)), (StatusCode::METHOD_NOT_ALLOWED, "method_not_allowed"));

    // multipart nur beim Upload
    let req = Request::post("/api/v1/activities/import")
        .header("host", HOST)
        .header("origin", ORIGIN)
        .header("content-type", "multipart/form-data; boundary=x")
        .body(Body::from("--x--"))
        .unwrap();
    assert_ne!(send(&f.app, req).await.0, StatusCode::UNSUPPORTED_MEDIA_TYPE);
    f.db.close().await;
}

#[tokio::test]
async fn preflight_gets_no_cors() {
    let f = fixture(false).await;
    let req = Request::builder()
        .method(Method::OPTIONS)
        .uri("/api/v1/version")
        .header("host", HOST)
        .header("origin", "https://evil.example")
        .header("access-control-request-method", "DELETE")
        .body(Body::empty())
        .unwrap();
    let (_, h, _) = send(&f.app, req).await;
    assert!(h.keys().all(|k| !k.as_str().starts_with("access-control-")), "{h:?}");
    f.db.close().await;
}

#[tokio::test]
async fn unknown_api_paths_are_json_404() {
    let f = fixture(true).await;
    for path in ["/api/v1/gibtsnicht", "/api/v2/version", "/api"] {
        let (s, _, v) = send(&f.app, get(path).body(Body::empty()).unwrap()).await;
        assert_eq!((s, code(&v)), (StatusCode::NOT_FOUND, "not_found"), "{path}");
    }
    f.db.close().await;
}

#[tokio::test]
async fn backend_serves_frontend_with_security_headers() {
    let f = fixture(true).await;
    for path in ["/", "/aktivitaeten/123", "/assets/app.js"] {
        let (s, h, body) = send(&f.app, get(path).body(Body::empty()).unwrap()).await;
        assert_eq!(s, StatusCode::OK, "{path}");
        for (name, value) in SECURITY_HEADERS {
            assert_eq!(h[*name], *value, "{name} auf {path}");
        }
        if path != "/assets/app.js" {
            assert!(body.as_str().unwrap().contains("TrackCraft"), "SPA-Rückfall auf index.html für {path}");
        }
    }
    // Auch auf API- und Fehlerantworten
    let (_, h, _) = send(&f.app, get("/api/v1/nichts").body(Body::empty()).unwrap()).await;
    assert_eq!(h["content-security-policy"], api::CSP);
    let (s, h, _) = send(&f.app, Request::get("/").header("host", "evil.example").body(Body::empty()).unwrap()).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "Host-Prüfung auch für Frontend-Dateien");
    assert_eq!(h["x-frame-options"], "DENY");
    // Navigation von einer fremden Seite zur App ist erlaubt (nur die API prüft Sec-Fetch-Site).
    let req = get("/").header("sec-fetch-site", "cross-site").body(Body::empty()).unwrap();
    assert_eq!(send(&f.app, req).await.0, StatusCode::OK);
    f.db.close().await;
}

#[tokio::test]
async fn container_mode_sets_no_frontend_headers() {
    let f = fixture(false).await;
    let (s, h, _) = send(&f.app, get("/").body(Body::empty()).unwrap()).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    assert!(h.get("content-security-policy").is_none(), "Nginx setzt die Header im Container");
    f.db.close().await;
}

/// Zertifikat mit Ablauf in `days` Tagen (PEM).
fn cert_expiring_in(days: i64) -> String {
    let mut params = rcgen::CertificateParams::new(vec![HOST.to_string()]).unwrap();
    params.not_before = time::OffsetDateTime::now_utc() - time::Duration::days(1);
    params.not_after = time::OffsetDateTime::now_utc() + time::Duration::days(days);
    let key = rcgen::KeyPair::generate().unwrap();
    params.self_signed(&key).unwrap().pem()
}

fn warning_codes(v: &serde_json::Value) -> Vec<String> {
    v["data"]["warnings"].as_array().unwrap().iter().map(|w| w["code"].as_str().unwrap().to_string()).collect()
}

#[tokio::test]
async fn system_reports_state() {
    let f = fixture(false).await;
    let (s, _, v) = send(&f.app, get("/api/v1/system").body(Body::empty()).unwrap()).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let d = &v["data"];
    let expected: &[&str] = if cfg!(windows) { &["windows_console"] } else { &["linux_console", "linux_service", "container"] };
    assert!(expected.contains(&d["platform"].as_str().unwrap()), "{d}");
    assert_eq!(d["tzdata_version"], chrono_tz::IANA_TZDB_VERSION);
    // Ohne [tls] kein Zertifikat
    assert!(d["tls_certificate_expires_at"].is_null());
    assert_eq!(d["counts"], serde_json::json!({ "activities": 0, "segments": 0, "efforts": 0 }));
    assert_eq!(d["jobs"], serde_json::json!({ "queued": 0, "running": 0, "failed": 0 }));
    assert!(d["storage"]["data_free_bytes"].as_u64().unwrap() > 0);
    assert!(!warning_codes(&v).iter().any(|c| c == "certificate_expiring" || c == "job_failed"), "{v}");

    // Fehlgeschlagener Auftrag: Zähler und Warnung seit seinem Ende
    sqlx::query(
        "INSERT INTO jobs (id, kind, status, error_message, created_at_ms, finished_at_ms) \
         VALUES ('j1', 'recalculation', 'failed', 'kaputt', 1000, 1782892800500)",
    )
    .execute(&f.db.writer)
    .await
    .unwrap();
    let (_, _, v) = send(&f.app, get("/api/v1/system").body(Body::empty()).unwrap()).await;
    assert_eq!(v["data"]["jobs"]["failed"], 1);
    let w = v["data"]["warnings"].as_array().unwrap().iter().find(|w| w["code"] == "job_failed").unwrap().clone();
    assert_eq!(w["since"], "2026-07-01T08:00:00.500Z");
    f.db.close().await;
}

#[tokio::test]
async fn system_warns_before_certificate_expires() {
    let f = fixture(true).await;
    let cert = f._dir.path().join("c.pem");
    std::fs::write(&cert, cert_expiring_in(10)).unwrap();
    let (s, _, v) = send(&f.app, get("/api/v1/system").body(Body::empty()).unwrap()).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert!(v["data"]["tls_certificate_expires_at"].is_string(), "{v}");
    let w = v["data"]["warnings"].as_array().unwrap().iter().find(|w| w["code"] == "certificate_expiring").unwrap().clone();
    assert!(w["message"].as_str().unwrap().contains("10 days"), "{w}");

    // Mehr als 30 Tage: keine Warnung
    std::fs::write(&cert, cert_expiring_in(60)).unwrap();
    let (_, _, v) = send(&f.app, get("/api/v1/system").body(Body::empty()).unwrap()).await;
    assert!(v["data"]["tls_certificate_expires_at"].is_string());
    assert!(!warning_codes(&v).contains(&"certificate_expiring".to_string()), "{v}");
    f.db.close().await;
}
