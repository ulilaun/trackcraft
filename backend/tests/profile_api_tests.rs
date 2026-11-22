//! Benutzerprofil `GET/PATCH /profile`.

mod common;

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;
use trackcraft::api::{self, AppState};
use trackcraft::config;
use trackcraft::startup;

const HOST: &str = "trackcraft.test";
const ORIGIN: &str = "https://trackcraft.test";

struct Fixture {
    _dir: tempfile::TempDir,
    app: Router,
    db: trackcraft::db::Db,
}

async fn fixture() -> Fixture {
    let dir = common::temp();
    let cfg = config::parse(&dir.path().join("t.toml"), &common::config_text(dir.path(), "", "")).unwrap();
    let started = startup::prepare(&cfg).await.unwrap();
    let db = started.db.clone();
    drop(started);
    let app = api::router(AppState { cfg: Arc::new(cfg), db: db.clone(), garmin: Default::default() });
    Fixture { _dir: dir, app, db }
}

async fn send(f: &Fixture, method: Method, body: Option<Value>) -> (StatusCode, Value) {
    let mut req = Request::builder().method(method).uri("/api/v1/profile").header("host", HOST).header("origin", ORIGIN);
    if body.is_some() {
        req = req.header("content-type", "application/json");
    }
    let req = req.body(body.map(|b| Body::from(b.to_string())).unwrap_or_else(Body::empty)).unwrap();
    let res = f.app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

async fn counter(f: &Fixture) -> i64 {
    sqlx::query_scalar("SELECT change_counter FROM data_state").fetch_one(&f.db.reader).await.unwrap()
}

#[tokio::test]
async fn read_and_update_profile() {
    let f = fixture().await;
    let (s, v) = send(&f, Method::GET, None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["data"]["nickname"], "Testperson");
    assert_eq!(v["data"]["first_name"], Value::Null);
    assert_eq!(v["data"]["birthday"], Value::Null);

    let body =
        json!({ "nickname": " Uli ", "first_name": "Ulrich", "last_name": "Muster", "birthday": "1965-04-03", "avatar_color": "teal" });
    let (s, v) = send(&f, Method::PATCH, Some(body)).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let d = &v["data"];
    assert_eq!((d["nickname"].as_str(), d["first_name"].as_str(), d["last_name"].as_str()), (Some("Uli"), Some("Ulrich"), Some("Muster")));
    assert_eq!((d["birthday"].as_str(), d["avatar_color"].as_str()), (Some("1965-04-03"), Some("teal")));
    assert_eq!(counter(&f).await, 1);

    // Gleiche Werte sind keine Änderung; null leert, fehlende Felder bleiben.
    let (_, _) = send(&f, Method::PATCH, Some(json!({ "nickname": "Uli" }))).await;
    assert_eq!(counter(&f).await, 1);
    let (_, v) = send(&f, Method::PATCH, Some(json!({ "last_name": null, "birthday": "" }))).await;
    assert_eq!((v["data"]["last_name"].clone(), v["data"]["birthday"].clone()), (Value::Null, Value::Null));
    assert_eq!(v["data"]["first_name"], "Ulrich");
    assert_eq!(counter(&f).await, 2);

    // /config zeigt den Spitznamen aus der Datenbank
    let req = Request::builder().uri("/api/v1/config").header("host", HOST).body(Body::empty()).unwrap();
    let bytes = f.app.clone().oneshot(req).await.unwrap().into_body().collect().await.unwrap().to_bytes();
    let c: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(c["data"]["user"]["display_name"], "Uli");
    f.db.close().await;
}

#[tokio::test]
async fn invalid_profile_values_are_rejected() {
    let f = fixture().await;
    for (body, field, code) in [
        (json!({ "nickname": "  " }), "nickname", "required"),
        (json!({ "nickname": null }), "nickname", "required"),
        (json!({ "birthday": "03.04.1965" }), "birthday", "invalid"),
        (json!({ "birthday": "2999-01-01" }), "birthday", "out_of_range"),
        (json!({ "avatar_color": "#ff0000" }), "avatar_color", "invalid"),
        (json!({}), "body", "empty"),
        (json!({ "email": "x" }), "body", "invalid_json"),
    ] {
        let (s, v) = send(&f, Method::PATCH, Some(body.clone())).await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{body}: {v}");
        let e = &v["error"]["details"]["fields"][0];
        assert_eq!((e["field"].as_str(), e["code"].as_str()), (Some(field), Some(code)), "{body}");
    }
    assert_eq!(counter(&f).await, 0);
    f.db.close().await;
}
