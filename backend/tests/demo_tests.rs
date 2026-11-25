//! Demo (Issue #41): Kennzeichen, Verschiebung auf heute, Hinweis in `/config`, Schutz echter Daten.

mod common;

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use chrono::{NaiveDate, TimeZone};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::Row;
use tower::ServiceExt;
use trackcraft::api::{self, AppState};
use trackcraft::config::{self, Validated};
use trackcraft::db::Db;
use trackcraft::domain::track;
use trackcraft::{demo, startup};

const HOST: &str = "trackcraft.test";
const ORIGIN: &str = "https://trackcraft.test";
const T0: i64 = 1_782_892_800; // 2026-07-01T08:00:00Z = 10:00 MESZ
const BERLIN: chrono_tz::Tz = chrono_tz::Europe::Berlin;

struct Fixture {
    dir: tempfile::TempDir,
    app: Router,
    db: Db,
}

fn cfg(dir: &std::path::Path, extra: &str) -> Validated {
    config::parse(&dir.join("t.toml"), &common::config_text(dir, "", extra)).unwrap()
}

async fn fixture() -> Fixture {
    let dir = common::temp();
    let cfg = cfg(dir.path(), "");
    let started = startup::prepare(&cfg).await.unwrap();
    let db = started.db.clone();
    drop(started);
    let app = api::router(AppState { cfg: Arc::new(cfg), db: db.clone(), garmin: Default::default(), backup: Default::default() });
    Fixture { dir, app, db }
}

async fn send(f: &Fixture, method: Method, path: &str, body: Option<Value>) -> (StatusCode, Value) {
    let mut req = Request::builder().method(method).uri(path).header("host", HOST).header("origin", ORIGIN);
    if body.is_some() {
        req = req.header("content-type", "application/json");
    }
    let req = req.body(body.map(|b| Body::from(b.to_string())).unwrap_or_else(Body::empty)).unwrap();
    let res = f.app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

fn gpx(start: i64, n: usize) -> Vec<u8> {
    let mut s = String::from(
        "<?xml version=\"1.0\"?>\n<gpx version=\"1.1\" creator=\"test\" xmlns=\"http://www.topografix.com/GPX/1/1\">\n\
         <trk><name>Test</name><type>cycling</type><trkseg>\n",
    );
    for i in 0..n {
        let t = chrono::DateTime::from_timestamp(start + i as i64 * 10, 0).unwrap();
        s.push_str(&format!(
            "<trkpt lat=\"{:.6}\" lon=\"11.000000\"><ele>500</ele><time>{}</time></trkpt>\n",
            47.0 + i as f64 * 0.0003,
            t.format("%Y-%m-%dT%H:%M:%SZ")
        ));
    }
    s.push_str("</trkseg></trk></gpx>\n");
    s.into_bytes()
}

const BOUNDARY: &str = "----trackcraft-test";

async fn import(f: &Fixture, bytes: &[u8]) -> String {
    let mut body = format!(
        "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"a.gpx\"\r\nContent-Type: application/octet-stream\r\n\r\n"
    )
    .into_bytes();
    body.extend_from_slice(bytes);
    body.extend_from_slice(format!("\r\n--{BOUNDARY}--\r\n").as_bytes());
    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/activities/import")
        .header("host", HOST)
        .header("origin", ORIGIN)
        .header("content-type", format!("multipart/form-data; boundary={BOUNDARY}"))
        .body(Body::from(body))
        .unwrap();
    let res = f.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let v: Value = serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let id = v["data"]["id"].as_str().unwrap().to_string();
    let decision = json!({ "state_token": v["data"]["state_token"], "sessions": [{ "session_index": 0, "action": "import" }] });
    let (s, v) = send(f, Method::POST, &format!("/api/v1/import/jobs/{id}/confirm"), Some(decision)).await;
    assert_eq!(s, StatusCode::ACCEPTED, "{v}");
    v["data"]["imported_activity_ids"][0].as_str().unwrap().to_string()
}

async fn scalar_i64(db: &Db, sql: &'static str) -> i64 {
    sqlx::query_scalar(sql).fetch_one(&db.reader).await.unwrap()
}

async fn scalar_text(db: &Db, sql: &'static str) -> String {
    sqlx::query_scalar(sql).fetch_one(&db.reader).await.unwrap()
}

#[tokio::test]
async fn shift_moves_everything_to_yesterday() {
    let f = fixture().await;
    let id = import(&f, &gpx(T0, 50)).await;
    let (s, v) = send(
        &f,
        Method::POST,
        "/api/v1/plan/units",
        Some(
            json!({ "date": "2026-07-02", "sport_class": "cycling", "title": "Morgen", "mode": "zones", "zone_minutes": [0, 60, 0, 0, 0] }),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    let (s, v) =
        send(&f, Method::POST, "/api/v1/seasons", Some(json!({ "name": "Sommer", "start_date": "2026-06-01", "end_date": "2026-07-31" })))
            .await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    let (_, v) = send(&f, Method::GET, "/api/v1/config", None).await;
    assert_eq!(v["data"]["demo"], false);

    let today = NaiveDate::from_ymd_opt(2026, 11, 10).unwrap();
    // Ohne Kennzeichen: Abbruch, nichts verschoben
    let err = demo::shift_to(&f.db, BERLIN, today).await.unwrap_err();
    assert!(err.to_string().contains("keine Demo-Datenbank"), "{err}");
    assert_eq!(scalar_i64(&f.db, "SELECT start_time_ms FROM activities").await, T0 * 1000);

    assert_eq!(demo::mark(&f.db, BERLIN).await.unwrap(), NaiveDate::from_ymd_opt(2026, 7, 1).unwrap());
    let revision = scalar_text(&f.db, "SELECT revision_id FROM data_state").await;

    let s = demo::shift_to(&f.db, BERLIN, today).await.unwrap().expect("verschoben");
    assert_eq!(s.days, 131);
    assert_eq!(s.activities, 1);
    assert_eq!(s.last_day, NaiveDate::from_ymd_opt(2026, 11, 9).unwrap());

    // 10:00 MESZ bleibt 10:00, jetzt MEZ (09:00 UTC)
    let start = BERLIN.with_ymd_and_hms(2026, 11, 9, 10, 0, 0).unwrap().timestamp_millis();
    assert_eq!(scalar_i64(&f.db, "SELECT start_time_ms FROM activities").await, start);
    let row = sqlx::query("SELECT point_count, first_time_ms, last_time_ms, data FROM track_sections WHERE activity_id = ?")
        .bind(&id)
        .fetch_one(&f.db.reader)
        .await
        .unwrap();
    let count: i64 = row.get("point_count");
    let points = track::decode(&row.get::<Vec<u8>, _>("data"), Some(count as usize)).unwrap();
    assert_eq!(points[0].t_ms, Some(start));
    assert_eq!(points[49].t_ms, Some(start + 490_000));
    assert_eq!(row.get::<i64, _>("first_time_ms"), start);
    assert_eq!(row.get::<i64, _>("last_time_ms"), start + 490_000);

    assert_eq!(scalar_text(&f.db, "SELECT date FROM planned_units").await, "2026-11-10");
    assert_eq!(scalar_text(&f.db, "SELECT start_date || '/' || end_date FROM seasons").await, "2026-10-10/2026-12-09");
    assert_eq!(scalar_text(&f.db, "SELECT last_day FROM demo_state").await, "2026-11-09");
    assert_ne!(scalar_text(&f.db, "SELECT revision_id FROM data_state").await, revision, "neue Revision");

    // Zweiter Start am selben Tag: nichts zu tun
    assert!(demo::shift_to(&f.db, BERLIN, today).await.unwrap().is_none());
    f.db.close().await;
}

#[tokio::test]
async fn startup_with_demo_refuses_real_data_and_shifts_demo_data() {
    let f = fixture().await;
    import(&f, &gpx(T0, 20)).await;
    f.db.close().await;

    let demo_cfg = cfg(f.dir.path(), "[demo]\nenabled = true\n");
    let err = startup::prepare(&demo_cfg).await.err().expect("Abbruch ohne Kennzeichen");
    assert!(format!("{err:#}").contains("keine Demo-Datenbank"), "{err:#}");

    let plain = startup::prepare(&cfg(f.dir.path(), "")).await.unwrap();
    demo::mark(&plain.db, BERLIN).await.unwrap();
    plain.db.close().await;
    drop(plain);

    let started = startup::prepare(&demo_cfg).await.unwrap();
    let start: i64 = sqlx::query_scalar("SELECT start_time_ms FROM activities").fetch_one(&started.db.reader).await.unwrap();
    let yesterday = chrono::Utc::now().with_timezone(&BERLIN).date_naive().pred_opt().unwrap();
    let day = chrono::Utc.timestamp_millis_opt(start).unwrap().with_timezone(&BERLIN).date_naive();
    assert_eq!(day, yesterday);

    let app =
        api::router(AppState { cfg: Arc::new(demo_cfg), db: started.db.clone(), garmin: Default::default(), backup: Default::default() });
    let req = Request::builder().uri("/api/v1/config").header("host", HOST).body(Body::empty()).unwrap();
    let v: Value = serde_json::from_slice(&app.oneshot(req).await.unwrap().into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(v["data"]["demo"], true);
    started.db.close().await;
}

#[test]
fn workspace_copies_demo_data_once_and_writes_valid_config() {
    let tmp = tempfile::tempdir().unwrap();
    let source = tmp.path().join("demo-daten");
    std::fs::create_dir_all(source.join("database")).unwrap();
    std::fs::create_dir_all(source.join("activities").join("2026")).unwrap();
    std::fs::write(source.join("database").join("trackcraft.db"), b"demo").unwrap();
    std::fs::write(source.join("activities").join("2026").join("a.gpx"), b"<gpx/>").unwrap();
    let web = tmp.path().join("web");
    std::fs::create_dir_all(&web).unwrap();
    std::fs::write(web.join("index.html"), b"<html/>").unwrap();
    let ws = demo::Workspace { dir: tmp.path().join("Demo Ordner"), source, web, port: 18999, container: false };

    ws.prepare(false).unwrap();
    let db = ws.dir.join("data").join("database").join("trackcraft.db");
    assert_eq!(std::fs::read(&db).unwrap(), b"demo");
    assert!(ws.dir.join("data").join("activities").join("2026").join("a.gpx").is_file());
    assert!(ws.dir.join("tls").join("server.pem").is_file());
    let cfg = config::load(&ws.config_file()).unwrap();
    assert!(cfg.raw.demo.enabled);
    assert_eq!(cfg.raw.server.allowed_origins, vec!["https://localhost:18999".to_string()]);
    assert_eq!(cfg.raw.server.listen, "127.0.0.1:18999");
    let in_container = demo::Workspace { container: true, ..ws.clone() };
    in_container.prepare(false).unwrap();
    assert_eq!(config::load(&ws.config_file()).unwrap().raw.server.listen, "0.0.0.0:18999");
    ws.prepare(false).unwrap();

    // Zweiter Start behält die Demo-Daten (samt Verschiebung und eigenen Änderungen), --reset beginnt neu
    std::fs::write(&db, b"geaendert").unwrap();
    ws.prepare(false).unwrap();
    assert_eq!(std::fs::read(&db).unwrap(), b"geaendert");
    ws.prepare(true).unwrap();
    assert_eq!(std::fs::read(&db).unwrap(), b"demo");
}

#[test]
fn workspace_without_demo_data_fails_clearly() {
    let tmp = tempfile::tempdir().unwrap();
    let web = tmp.path().join("web");
    std::fs::create_dir_all(&web).unwrap();
    std::fs::write(web.join("index.html"), b"<html/>").unwrap();
    let ws = demo::Workspace { dir: tmp.path().join("demo"), source: tmp.path().join("fehlt"), web, port: 18999, container: false };
    let e = ws.prepare(false).unwrap_err().to_string();
    assert!(e.contains("Demo-Datenbank fehlt"), "{e}");
}
