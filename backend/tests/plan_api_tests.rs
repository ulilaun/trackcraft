//! Trainingsplanung (E8): Einheiten, Belastung aus Zonenminuten, Ziel, Vorschau „Plan“, Verknüpfen mit Aktivitäten.

mod common;

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;
use trackcraft::api::{self, AppState};
use trackcraft::config::{self, Validated};
use trackcraft::db::Db;
use trackcraft::startup;

const HOST: &str = "trackcraft.test";
const ORIGIN: &str = "https://trackcraft.test";
const T0: i64 = 1_782_892_800; // 2026-07-01T08:00:00Z

struct Fixture {
    _dir: tempfile::TempDir,
    cfg: Arc<Validated>,
    app: Router,
    db: Db,
}

async fn fixture() -> Fixture {
    let dir = common::temp();
    let cfg = config::parse(&dir.path().join("t.toml"), &common::config_text(dir.path(), "", "")).unwrap();
    let started = startup::prepare(&cfg).await.unwrap();
    let db = started.db.clone();
    drop(started);
    let cfg = Arc::new(cfg);
    let app = api::router(AppState { cfg: cfg.clone(), db: db.clone(), garmin: Default::default() });
    Fixture { _dir: dir, cfg, app, db }
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

async fn get(f: &Fixture, path: &str) -> Value {
    let (s, v) = send(f, Method::GET, path, None).await;
    assert_eq!(s, StatusCode::OK, "{path}: {v}");
    v["data"].clone()
}

/// GPX mit `n` Punkten im Abstand von 10 s; `hr` = None ergibt eine Datei ohne Herzfrequenz.
fn gpx(kind: &str, start: i64, n: usize, hr: Option<u32>) -> Vec<u8> {
    let mut s = format!(
        "<?xml version=\"1.0\"?>\n<gpx version=\"1.1\" creator=\"test\" xmlns=\"http://www.topografix.com/GPX/1/1\" \
         xmlns:gpxtpx=\"http://www.garmin.com/xmlschemas/TrackPointExtension/v1\">\n<trk><name>Test</name><type>{kind}</type><trkseg>\n"
    );
    for i in 0..n {
        let t = chrono::DateTime::from_timestamp(start + i as i64 * 10, 0).unwrap();
        let ext = hr
            .map(|h| {
                format!("<extensions><gpxtpx:TrackPointExtension><gpxtpx:hr>{h}</gpxtpx:hr></gpxtpx:TrackPointExtension></extensions>")
            })
            .unwrap_or_default();
        s.push_str(&format!(
            "<trkpt lat=\"{:.6}\" lon=\"11.000000\"><ele>500</ele><time>{}</time>{ext}</trkpt>\n",
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

async fn refresh(f: &Fixture) -> usize {
    trackcraft::load::refresh(&f.cfg, &f.db).await.unwrap()
}

fn unit_body(date: &str, title: &str) -> Value {
    json!({ "date": date, "sport_class": "cycling", "title": title, "mode": "zones", "zone_minutes": [0, 60, 0, 10, 0] })
}

async fn new_unit(f: &Fixture, body: Value) -> String {
    let (s, v) = send(f, Method::POST, "/api/v1/plan/units", Some(body)).await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    v["data"]["id"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn units_goal_and_plan_forecast() {
    let f = fixture().await;
    let today: chrono::NaiveDate = get(&f, "/api/v1/plan").await["today"].as_str().unwrap().parse().unwrap();
    let day = |n: i64| (today + chrono::Duration::days(n)).to_string();

    // Ohne Zonensatz: Zonenminuten werden aus der Dauer geschätzt (Radfahren 50 je Stunde)
    let (s, v) = send(&f, Method::POST, "/api/v1/plan/units", Some(unit_body(&day(2), "Intervalle"))).await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    let id = v["data"]["id"].as_str().unwrap().to_string();
    assert_eq!((v["data"]["load"]["method"].as_str(), v["data"]["load"]["minutes"].as_u64()), (Some("estimated"), Some(70)));
    assert!((v["data"]["load"]["total"].as_f64().unwrap() - 58.3).abs() < 0.05, "{v}");
    assert_eq!(v["data"]["status"], "planned");

    // Mit Zonensatz: Banister mit HF der Zonenmitte, ab Zone 4 anaerob
    let zones = json!({ "valid_from": "2020-01-01", "max_hr": 180, "resting_hr": 50, "model": "reserve" });
    let (s, _) = send(&f, Method::POST, "/api/v1/hr-zones", Some(zones)).await;
    assert_eq!(s, StatusCode::CREATED);
    let u = get(&f, &format!("/api/v1/plan/units/{id}")).await;
    assert_eq!(u["load"]["method"], "zones");
    let b = |min: f64, hr: f64| {
        let x = (hr - 50.0) / 130.0;
        min * x * 0.64 * (1.92 * x).exp()
    };
    assert!((u["load"]["aerobic"].as_f64().unwrap() - b(60.0, 134.5)).abs() < 0.1, "{u}");
    assert!((u["load"]["anaerobic"].as_f64().unwrap() - b(10.0, 160.5)).abs() < 0.1, "{u}");

    // Zielbelastung mit Anteil intensiv; Wechsel der Art verlangt die passenden Werte
    let path = format!("/api/v1/plan/units/{id}");
    let (s, v) = send(&f, Method::PATCH, &path, Some(json!({ "mode": "target" }))).await;
    assert_eq!((s, v["error"]["details"]["fields"][0]["field"].as_str()), (StatusCode::BAD_REQUEST, Some("duration_min")));
    let body = json!({ "mode": "target", "duration_min": 90, "target_load": 80, "intensive_pct": 25, "note": "  locker  " });
    let (s, v) = send(&f, Method::PATCH, &path, Some(body)).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!((v["data"]["load"]["total"].as_f64(), v["data"]["load"]["anaerobic"].as_f64()), (Some(80.0), Some(20.0)));
    assert_eq!((v["data"]["note"].as_str(), v["data"]["zone_minutes"].clone()), (Some("locker"), Value::Null));

    // Prüfungen
    for (body, field) in [
        (json!({ "date": "gestern" }), "date"),
        (json!({ "sport_class": "unknown" }), "sport_class"),
        (json!({ "title": " " }), "title"),
        (json!({ "status": "done" }), "status"),
        (json!({ "sub_sport": "gibtsnicht" }), "sub_sport"),
        (json!({ "mode": "zones", "zone_minutes": [0, 0, 0, 0, 0] }), "zone_minutes"),
        (json!({ "target_load": 5000 }), "target_load"),
    ] {
        let (s, v) = send(&f, Method::PATCH, &path, Some(body.clone())).await;
        assert_eq!((s, v["error"]["details"]["fields"][0]["field"].as_str()), (StatusCode::BAD_REQUEST, Some(field)), "{body}");
    }
    let (s, _) = send(&f, Method::POST, "/api/v1/plan/units", Some(json!({ "date": day(1) }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    // Ziel und Vorschau „Plan“: reicht über das Ziel hinaus, Tage ohne Einheit sind Ruhe, ausgelassene zählen nicht
    let (s, v) = send(&f, Method::PUT, "/api/v1/plan/goal", Some(json!({ "date": day(40), "name": " Tour " }))).await;
    assert_eq!((s, v["data"]["name"].as_str()), (StatusCode::OK, Some("Tour")), "{v}");
    let skipped = new_unit(&f, unit_body(&day(3), "Fällt aus")).await;
    let (s, _) = send(&f, Method::PATCH, &format!("/api/v1/plan/units/{skipped}"), Some(json!({ "status": "skipped" }))).await;
    assert_eq!(s, StatusCode::OK);
    let v = get(&f, "/api/v1/form?forecast=plan&forecast_days=28").await;
    let fc = v["forecast"]["days"].as_array().unwrap();
    assert_eq!(fc.len(), 43, "Ziel in 40 Tagen + 3");
    assert_eq!(v["forecast"]["mode"], "plan");
    assert_eq!(v["plan"]["goal"]["date"].as_str(), Some(day(40).as_str()));
    assert_eq!(v["plan"]["last_date"].as_str(), Some(day(2).as_str()));
    let load_on = |d: &str| fc.iter().find(|x| x["date"] == d).unwrap()["load"].as_f64().unwrap();
    assert_eq!((load_on(&day(1)), load_on(&day(2)), load_on(&day(3))), (0.0, 80.0, 0.0));
    assert!(v["current"]["compare"]["week_ahead"]["fitness"].as_f64().unwrap() > 0.0);
    // Ohne Vorschau keine Verlängerung
    assert_eq!(get(&f, "/api/v1/form?forecast=plan&forecast_days=0").await["forecast"]["days"], json!([]));

    // Liste im Zeitraum, Ziel entfernen, Einheit löschen
    let list = get(&f, &format!("/api/v1/plan?from={}&to={}", day(0), day(10))).await;
    assert_eq!(list["units"].as_array().unwrap().len(), 2);
    assert_eq!(list["goal"]["name"], "Tour");
    let (s, _) = send(&f, Method::DELETE, "/api/v1/plan/goal", None).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    assert_eq!(get(&f, "/api/v1/plan").await["goal"], Value::Null);
    let (s, _) = send(&f, Method::DELETE, &format!("/api/v1/plan/units/{skipped}"), None).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, v) = send(&f, Method::GET, &format!("/api/v1/plan/units/{skipped}"), None).await;
    assert_eq!((s, v["error"]["code"].as_str()), (StatusCode::NOT_FOUND, Some("planned_unit_not_found")));
    let (s, _) = send(&f, Method::GET, "/api/v1/plan?from=2026-01-01&to=2028-01-01", None).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn link_planned_unit_with_activity() {
    let f = fixture().await;
    // Fahrt am 01.07.2026, 60 min mit HF 128
    let ride = import(&f, &gpx("cycling", T0, 361, Some(128))).await;
    refresh(&f).await;
    let target = |date: &str, class: &str, title: &str| json!({ "date": date, "sport_class": class, "title": title, "mode": "target", "duration_min": 75, "target_load": 70 });
    let a = new_unit(&f, target("2026-07-01", "cycling", "Grundlage")).await;
    let b = new_unit(&f, target("2026-07-01", "cycling", "Alternative")).await;
    new_unit(&f, target("2026-07-01", "running", "Lauf")).await;
    new_unit(&f, target("2026-07-02", "cycling", "Morgen")).await;

    // Vorschläge: gleicher Tag, gleiche Sportklasse
    let p = get(&f, &format!("/api/v1/activities/{ride}/plan")).await;
    assert_eq!(p["linked"], Value::Null);
    let titles: Vec<&str> = p["candidates"].as_array().unwrap().iter().map(|u| u["title"].as_str().unwrap()).collect();
    assert_eq!(titles, vec!["Grundlage", "Alternative"]);

    // „Nicht verknüpfen“ blendet den Vorschlag aus
    let (s, _) = send(&f, Method::POST, &format!("/api/v1/plan/units/{b}/decline"), Some(json!({ "activity_id": ride }))).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    assert_eq!(get(&f, &format!("/api/v1/activities/{ride}/plan")).await["candidates"].as_array().unwrap().len(), 1);

    // Verknüpfen: erledigt, Vergleich mit den Ist-Werten
    let link = |unit: &str| format!("/api/v1/plan/units/{unit}/activity");
    let (s, v) = send(&f, Method::PUT, &link(&a), Some(json!({ "activity_id": ride }))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["data"]["status"], "done");
    assert_eq!(v["data"]["actual"]["total_time_ms"], 3_600_000);
    assert!(v["data"]["actual"]["load"].as_f64().unwrap() > 0.0);
    let p = get(&f, &format!("/api/v1/activities/{ride}/plan")).await;
    assert_eq!((p["linked"]["id"].as_str(), p["candidates"].as_array().unwrap().len()), (Some(a.as_str()), 0));
    // Eine Aktivität gehört zu höchstens einer Einheit
    let (s, v) = send(&f, Method::PUT, &link(&b), Some(json!({ "activity_id": ride }))).await;
    assert_eq!((s, v["error"]["code"].as_str()), (StatusCode::CONFLICT, Some("activity_already_planned")));
    // Erledigte Einheiten zählen nicht in die Vorschau: Alternative und Lauf am 01.07., Morgen am 02.07.
    let s = trackcraft::load::settings(&f.db).await.unwrap();
    let (l, last) = trackcraft::plan::day_loads(&f.db, chrono::NaiveDate::from_ymd_opt(2026, 6, 30).unwrap(), &s).await.unwrap();
    assert_eq!(l.values().map(|d| d.total).sum::<f64>(), 210.0);
    assert_eq!(last, chrono::NaiveDate::from_ymd_opt(2026, 7, 2));

    // Lösen; gelöschte Aktivität macht die Einheit wieder offen
    let (s, v) = send(&f, Method::DELETE, &link(&a), None).await;
    assert_eq!((s, v["data"]["status"].as_str()), (StatusCode::OK, Some("planned")));
    send(&f, Method::PUT, &link(&a), Some(json!({ "activity_id": ride }))).await;
    let (s, _) = send(&f, Method::DELETE, &format!("/api/v1/activities/{ride}"), None).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    assert_eq!(get(&f, &format!("/api/v1/plan/units/{a}")).await["status"], "planned");
    let (s, _) = send(&f, Method::GET, &format!("/api/v1/activities/{ride}/plan"), None).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}
