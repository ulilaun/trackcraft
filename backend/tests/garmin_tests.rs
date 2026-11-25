//! Garmin-Abruf gegen einen nachgebildeten Garmin-Server: Schlüsselprüfung, Gesamtimport mit
//! Garmin-Typ, Verknüpfen statt Duplikat, Abweisungen, inkrementeller Abruf und Anfangsdatum.

mod common;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, Method, Request, StatusCode};
use axum::response::{IntoResponse, Response};
use http_body_util::BodyExt;
use rustyfit::profile::mesgdef;
use rustyfit::profile::typedef::{self, DateTime};
use rustyfit::proto::{FIT, Message};
use serde_json::{Value, json};
use tower::ServiceExt;
use trackcraft::api::{self, AppState};
use trackcraft::config::{self, Validated};
use trackcraft::db::Db;
use trackcraft::garmin::{self, Runner};
use trackcraft::startup;

const HOST: &str = "trackcraft.test";
const ORIGIN: &str = "https://trackcraft.test";
const KEY: &str = "good-key-123";
const DAY: i64 = 86_400;
const T0: i64 = 1_782_892_800; // 2026-07-01T08:00:00Z

/// Inhalt einer nachgebildeten Garmin-Aktivität.
#[derive(Clone)]
enum Body_ {
    Fit(Vec<u8>),
    Garbage,
    Missing,
}

#[derive(Clone)]
struct Act {
    id: i64,
    start_s: i64,
    type_key: &'static str,
    body: Body_,
}

#[derive(Clone, Default)]
struct Mock {
    acts: Arc<Mutex<Vec<Act>>>,
    downloads: Arc<AtomicUsize>,
    list_calls: Arc<AtomicUsize>,
}

fn fit(start_s: i64, hr: u8) -> Vec<u8> {
    let mut file_id = mesgdef::FileId::new();
    file_id.r#type = typedef::File::ACTIVITY;
    let mut messages: Vec<Message> = vec![file_id.into()];
    for i in 0..60 {
        let mut r = mesgdef::Record::new();
        r.timestamp = DateTime::from_unix_timestamp(start_s + i * 10);
        r.position_lat = 570_000_000 + i as i32 * 3000;
        r.position_long = 140_000_000;
        r.heart_rate = hr;
        r.distance = (i * 10 * 500) as u32;
        messages.push(r.into());
    }
    let mut s = mesgdef::Session::new();
    s.start_time = DateTime::from_unix_timestamp(start_s);
    s.timestamp = DateTime::from_unix_timestamp(start_s + 590);
    s.total_elapsed_time = 590_000;
    s.total_timer_time = 590_000;
    s.total_distance = 295_000;
    s.sport = typedef::Sport::GENERIC;
    messages.push(s.into());
    let mut enc = rustyfit::EncoderBuilder::new().build();
    let mut fit = FIT { messages, ..Default::default() };
    let mut out = embedded_io_adapters::std::FromStd::new(std::io::Cursor::new(Vec::<u8>::new()));
    enc.encode(&mut out, &mut fit).unwrap();
    out.into_inner().into_inner()
}

fn zip_of(name: &str, data: &[u8]) -> Vec<u8> {
    use std::io::Write;
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut w = zip::ZipWriter::new(&mut buf);
        w.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
        w.write_all(data).unwrap();
        w.finish().unwrap();
    }
    buf.into_inner()
}

fn authorized(h: &HeaderMap) -> bool {
    h.get("authorization").and_then(|v| v.to_str().ok()) == Some("Bearer tok-1")
}

async fn token(h: HeaderMap) -> Response {
    if h.get("authorization").and_then(|v| v.to_str().ok()) == Some(&format!("ApiKey {KEY}")) {
        "tok-1".into_response()
    } else {
        StatusCode::UNAUTHORIZED.into_response()
    }
}

async fn list(State(m): State<Mock>, h: HeaderMap, Query(q): Query<std::collections::HashMap<String, usize>>) -> Response {
    if !authorized(&h) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    m.list_calls.fetch_add(1, Ordering::SeqCst);
    let mut acts = m.acts.lock().unwrap().clone();
    acts.sort_by_key(|a| -a.start_s);
    let page: Vec<Value> = acts
        .iter()
        .skip(q["start"])
        .take(q["limit"])
        .map(|a| json!({ "activityId": a.id, "beginTimestamp": a.start_s * 1000, "activityType": { "typeKey": a.type_key }, "other": 1 }))
        .collect();
    axum::Json(page).into_response()
}

async fn download(State(m): State<Mock>, h: HeaderMap, Path(id): Path<i64>) -> Response {
    if !authorized(&h) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    m.downloads.fetch_add(1, Ordering::SeqCst);
    let act = m.acts.lock().unwrap().iter().find(|a| a.id == id).cloned();
    match act.map(|a| a.body) {
        Some(Body_::Fit(f)) => zip_of(&format!("{id}_ACTIVITY.fit"), &f).into_response(),
        Some(Body_::Garbage) => b"PK this is no zip".to_vec().into_response(),
        Some(Body_::Missing) | None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn mock_server(m: Mock) -> String {
    let app = Router::new()
        .route("/personalapitoken-service/personal-api/personal-api-key/token", axum::routing::post(token))
        .route("/activitylist-service/activities/search/activities", axum::routing::get(list))
        .route("/download-service/files/activity/{id}", axum::routing::get(download))
        .with_state(m);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{addr}")
}

struct Fixture {
    _dir: tempfile::TempDir,
    cfg: Arc<Validated>,
    db: Db,
    app: Router,
    runner: Arc<Runner>,
    mock: Mock,
}

async fn fixture(acts: Vec<Act>) -> Fixture {
    let mock = Mock { acts: Arc::new(Mutex::new(acts)), ..Default::default() };
    let base = mock_server(mock.clone()).await;
    let dir = common::temp();
    let extra = format!("[garmin]\napi_base_url = \"{base}\"\nrequest_delay_ms = 0\n");
    let text = common::config_text(dir.path(), "", &extra);
    let cfg = Arc::new(config::parse(&dir.path().join("t.toml"), &text).unwrap());
    let started = startup::prepare(&cfg).await.unwrap();
    let db = started.db.clone();
    drop(started);
    let runner = Arc::new(Runner::default());
    let app = api::router(AppState { cfg: cfg.clone(), db: db.clone(), garmin: runner.clone(), backup: Default::default() });
    Fixture { _dir: dir, cfg, db, app, runner, mock }
}

async fn call(f: &Fixture, method: Method, path: &str, body: Option<Value>) -> (StatusCode, Value) {
    let mut req = Request::builder().method(method.clone()).uri(format!("/api/v1{path}")).header("host", HOST);
    if method != Method::GET {
        req = req.header("origin", ORIGIN);
    }
    let req = match body {
        Some(b) => req.header("content-type", "application/json").body(Body::from(b.to_string())).unwrap(),
        None => req.body(Body::empty()).unwrap(),
    };
    let res = f.app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

async fn run(f: &Fixture) -> garmin::Progress {
    f.runner.run_once(&f.cfg, &f.db).await.expect("kein Lauf aktiv").expect("Lauf erfolgreich")
}

async fn scalar(db: &Db, sql: &str) -> Value {
    let v: Option<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(sql.to_string())).fetch_one(&db.reader).await.unwrap();
    json!(v)
}

fn history() -> Vec<Act> {
    vec![
        Act { id: 103, start_s: T0 + 2 * DAY, type_key: "road_biking", body: Body_::Fit(fit(T0 + 2 * DAY, 130)) },
        Act { id: 102, start_s: T0 + DAY, type_key: "mountain_biking", body: Body_::Fit(fit(T0 + DAY, 140)) },
        Act { id: 101, start_s: T0, type_key: "running", body: Body_::Garbage },
        Act { id: 100, start_s: T0 - DAY, type_key: "running", body: Body_::Missing },
    ]
}

#[tokio::test]
async fn settings_check_key_and_never_return_it() {
    let f = fixture(vec![]).await;
    let (s, v) = call(&f, Method::GET, "/garmin", None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["data"]["configured"], false);
    assert_eq!(v["data"]["interval_minutes"], 30);

    let (s, v) = call(&f, Method::POST, "/garmin/sync", None).await;
    assert_eq!((s, v["error"]["code"].as_str()), (StatusCode::CONFLICT, Some("garmin_not_configured")));

    let (s, v) = call(&f, Method::PATCH, "/garmin", Some(json!({ "api_key": "wrong-key-1" }))).await;
    assert_eq!((s, v["error"]["code"].as_str()), (StatusCode::BAD_REQUEST, Some("garmin_key_invalid")));

    let (s, v) = call(&f, Method::PATCH, "/garmin", Some(json!({ "interval_minutes": 2 }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{v}");
    let (s, _) = call(&f, Method::PATCH, "/garmin", Some(json!({ "start_date": "2026-13-01" }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = call(&f, Method::PATCH, "/garmin", Some(json!({ "password": "x" }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    let (s, v) = call(
        &f,
        Method::PATCH,
        "/garmin",
        Some(json!({ "api_key": KEY, "enabled": true, "start_date": "2020-01-01", "interval_minutes": 60 })),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let d = &v["data"];
    assert_eq!((d["configured"].clone(), d["enabled"].clone(), d["interval_minutes"].clone()), (json!(true), json!(true), json!(60)));
    assert_eq!(d["start_date"], "2020-01-01");
    assert!(!v.to_string().contains(KEY), "Schlüssel darf nie zurückkommen: {v}");

    let (s, v) = call(&f, Method::PATCH, "/garmin", Some(json!({ "api_key": null }))).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["data"]["configured"], false);
}

#[tokio::test]
async fn backfill_imports_links_rejects_and_then_only_fetches_new() {
    let f = fixture(history()).await;
    // Vorhandene Aktivität zur selben Zeit wie 102, aber andere Datei (andere HF): Duplikatkandidat.
    let pre = garmin::sync::import(&f.cfg, &f.db, 0, fit(T0 + DAY, 150), None).await.unwrap();
    assert_eq!(pre.status, "imported");
    let pre_id = pre.activity_id.unwrap();

    let (s, _) = call(&f, Method::PATCH, "/garmin", Some(json!({ "api_key": KEY }))).await;
    assert_eq!(s, StatusCode::OK);
    let p = run(&f).await;
    assert_eq!((p.total, p.processed, p.imported, p.linked, p.rejected, p.failed), (4, 4, 1, 1, 2, 0), "{p:?}");
    assert_eq!(f.mock.downloads.load(Ordering::SeqCst), 4);

    // Garmin-Typ ersetzt den generischen Sport der Datei; Titel nach TrackCraft-Regel.
    let (title, source, class, sub): (String, String, String, Option<String>) = sqlx::query_as(
        "SELECT a.title, a.source_sport, a.sport_class, a.sub_sport FROM activities a \
         JOIN garmin_activities g ON g.activity_id = a.id WHERE g.garmin_id = 103",
    )
    .fetch_one(&f.db.reader)
    .await
    .unwrap();
    assert_eq!((source.as_str(), class.as_str(), sub.as_deref()), ("road_biking", "cycling", Some("road")));
    assert_eq!(title, "Rennrad 03.07.2026 10:00");
    // 102 nur verknüpft, nicht doppelt angelegt
    assert_eq!(scalar(&f.db, "SELECT activity_id FROM garmin_activities WHERE garmin_id = 102").await, json!(pre_id));
    assert_eq!(scalar(&f.db, "SELECT CAST(count(*) AS TEXT) FROM activities").await, json!("2"));
    assert_eq!(scalar(&f.db, "SELECT reason_code FROM garmin_activities WHERE garmin_id = 100").await, json!("garmin_not_found"));
    assert_eq!(scalar(&f.db, "SELECT reason_code FROM garmin_activities WHERE garmin_id = 101").await, json!("invalid_archive"));

    let (_, v) = call(&f, Method::GET, "/garmin", None).await;
    let d = &v["data"];
    assert_eq!(d["backfill_done"], true);
    assert_eq!(d["last_error"], Value::Null);
    assert_eq!(d["counts"], json!({ "imported": 1, "linked": 1, "locked": 0, "rejected": 2, "failed": 0 }));
    let (_, v) = call(&f, Method::GET, "/garmin/activities?status=rejected", None).await;
    assert_eq!(v["meta"]["total"], 2);
    assert_eq!(v["data"][0]["garmin_id"], "101");
    let (_, v) = call(&f, Method::GET, "/garmin/activities?status=linked,rejected", None).await;
    assert_eq!(v["meta"]["total"], 3);
    let (s, _) = call(&f, Method::GET, "/garmin/activities?status=rejected,nope", None).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    // Zweiter Lauf: nichts Neues, keine Downloads, nur eine Listenseite.
    let calls = f.mock.list_calls.load(Ordering::SeqCst);
    let p = run(&f).await;
    assert_eq!((p.total, p.processed), (0, 0));
    assert_eq!(f.mock.downloads.load(Ordering::SeqCst), 4);
    assert_eq!(f.mock.list_calls.load(Ordering::SeqCst), calls + 1);

    // Neue Aktivität bei Garmin: nur sie wird geholt.
    f.mock.acts.lock().unwrap().push(Act { id: 104, start_s: T0 + 3 * DAY, type_key: "running", body: Body_::Fit(fit(T0 + 3 * DAY, 120)) });
    let p = run(&f).await;
    assert_eq!((p.total, p.processed, p.imported), (1, 1, 1));
    assert_eq!(f.mock.downloads.load(Ordering::SeqCst), 5);

    // Erneut versuchen: Abweisungen vergessen, nächster Lauf holt nur sie.
    let (s, v) = call(&f, Method::POST, "/garmin/activities/retry", None).await;
    assert_eq!((s, v["data"]["reset"].clone()), (StatusCode::OK, json!(2)));
    let p = run(&f).await;
    assert_eq!((p.total, p.processed, p.rejected), (2, 2, 2));
}

#[tokio::test]
async fn start_date_limits_backfill_and_expired_key_is_reported() {
    let f = fixture(history()).await;
    let (s, _) = call(&f, Method::PATCH, "/garmin", Some(json!({ "api_key": KEY, "start_date": "2026-07-02" }))).await;
    assert_eq!(s, StatusCode::OK);
    let p = run(&f).await;
    assert_eq!((p.processed, p.imported), (2, 2), "{p:?}");
    assert_eq!(scalar(&f.db, "SELECT CAST(backfill_done AS TEXT) FROM garmin_settings").await, json!("1"));

    // Früheres Anfangsdatum: Gesamtimport läuft weiter zurück.
    let (s, v) = call(&f, Method::PATCH, "/garmin", Some(json!({ "start_date": null }))).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["data"]["backfill_done"], false);
    let p = run(&f).await;
    assert_eq!(p.processed, 2);

    // Schlüssel abgelaufen: Lauf bricht ab, Status nennt den Grund.
    sqlx::query("UPDATE garmin_settings SET api_key = 'expired-key-9'").execute(&f.db.writer).await.unwrap();
    let r = f.runner.run_once(&f.cfg, &f.db).await.unwrap();
    assert!(r.is_err());
    let (_, v) = call(&f, Method::GET, "/garmin", None).await;
    assert_eq!(v["data"]["last_error"]["code"], "garmin_key_invalid");
    assert_eq!(v["data"]["running"], false);
}
