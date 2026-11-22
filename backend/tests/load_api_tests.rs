//! Belastung und Form (E7): HF-Zonensätze, Belastung je Aktivität, Einstellungen und Formkurve mit Vorschau.

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

async fn counter(f: &Fixture) -> i64 {
    sqlx::query_scalar("SELECT change_counter FROM data_state").fetch_one(&f.db.reader).await.unwrap()
}

#[tokio::test]
async fn zones_drive_activity_load() {
    let f = fixture().await;
    // 361 Punkte à 10 s = 60 min bei HF 128; dazu 30 min ohne HF
    let ride = import(&f, &gpx("cycling", T0, 361, Some(128))).await;
    let studio = import(&f, &gpx("cycling", T0 + 86_400, 181, None)).await;

    // Ohne Zonensatz wird alles geschätzt (Radfahren 50 je Stunde).
    assert_eq!(refresh(&f).await, 2);
    assert_eq!(refresh(&f).await, 0);
    let a = get(&f, &format!("/api/v1/activities/{ride}")).await;
    assert_eq!(a["training_load"]["method"], "estimated");
    assert_eq!(a["training_load"]["trimp"], 50.0);
    assert_eq!(a["training_load"]["zone_set"], Value::Null);

    // Allgemeiner Satz ab später gilt rückwirkend (ältester Satz).
    let before = counter(&f).await;
    let body = json!({ "valid_from": "2026-09-01", "max_hr": 180, "resting_hr": 50, "model": "reserve" });
    let (s, v) = send(&f, Method::POST, "/api/v1/hr-zones", Some(body)).await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    let general = v["data"]["id"].as_str().unwrap().to_string();
    assert_eq!(v["data"]["zones_bpm"], json!([[115, 127], [128, 140], [141, 153], [154, 166], [167, null]]));
    assert_eq!(v["data"]["anaerobic_from_bpm"], 154);
    assert_eq!(counter(&f).await, before + 1);

    assert_eq!(refresh(&f).await, 1, "nur die Aktivität mit HF");
    let a = get(&f, &format!("/api/v1/activities/{ride}")).await;
    let l = &a["training_load"];
    assert_eq!(l["method"], "samples");
    // 60 min bei 60 % Reserve ≈ 72,9
    assert_eq!(l["trimp"], 72.9, "{l}");
    assert_eq!((l["aerobic"].as_f64(), l["anaerobic"].as_f64()), (Some(72.9), Some(0.0)));
    assert_eq!(l["zone_seconds"], json!([0, 3600, 0, 0, 0]));
    assert_eq!(l["zone_set"]["id"], general.as_str());
    let s = get(&f, &format!("/api/v1/activities/{studio}")).await;
    assert_eq!((s["training_load"]["method"].as_str(), s["training_load"]["trimp"].as_f64()), (Some("estimated"), Some(25.0)));

    // Eigener Satz fürs Radfahren ab einem Datum danach ändert nichts, ab einem Datum davor schon.
    let body = json!({ "sport_class": "cycling", "valid_from": "2026-08-01", "max_hr": 170, "resting_hr": 50, "threshold_hr": 125, "model": "threshold" });
    let (s, v) = send(&f, Method::POST, "/api/v1/hr-zones", Some(body)).await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    let cycling = v["data"]["id"].as_str().unwrap().to_string();
    assert_eq!(v["data"]["bounds"], json!([68, 85, 90, 95, 100]));
    assert_eq!(refresh(&f).await, 0);
    let (s, _) = send(&f, Method::PATCH, &format!("/api/v1/hr-zones/{cycling}"), Some(json!({ "valid_from": "2026-06-01" }))).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(refresh(&f).await, 1);
    let l = get(&f, &format!("/api/v1/activities/{ride}")).await["training_load"].clone();
    assert_eq!(l["zone_set"]["id"], cycling.as_str());
    // HF 128 liegt über der Schwelle 125: alles anaerob
    assert!(l["anaerobic"].as_f64().unwrap() > 0.0 && l["aerobic"] == 0.0, "{l}");

    // Liste zeigt die Belastung
    let list = get(&f, "/api/v1/activities?limit=10").await;
    assert!(list.as_array().unwrap().iter().all(|a| a["load"].is_number() && a["load_method"].is_string()));
    // Sortierung nach Belastung und Summe in der Summenzeile
    let (_, v) = send(&f, Method::GET, "/api/v1/activities?sort=-load", None).await;
    let loads: Vec<f64> = v["data"].as_array().unwrap().iter().map(|a| a["load"].as_f64().unwrap()).collect();
    assert!(loads.windows(2).all(|w| w[0] >= w[1]), "{loads:?}");
    assert_eq!(v["meta"]["summary"]["load"].as_f64(), Some(loads.iter().sum::<f64>().round()));
    let (_, v) = send(&f, Method::GET, "/api/v1/activities?sort=load", None).await;
    assert_eq!(v["data"][0]["id"], studio.as_str());

    // Löschen: wieder der allgemeine Satz
    let (s, _) = send(&f, Method::DELETE, &format!("/api/v1/hr-zones/{cycling}"), None).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, v) = send(&f, Method::DELETE, &format!("/api/v1/hr-zones/{cycling}"), None).await;
    assert_eq!((s, v["error"]["code"].as_str()), (StatusCode::NOT_FOUND, Some("hr_zone_set_not_found")));
    assert_eq!(refresh(&f).await, 1);
}

#[tokio::test]
async fn zone_set_validation() {
    let f = fixture().await;
    let post = |b: Value| send(&f, Method::POST, "/api/v1/hr-zones", Some(b));
    let field = |v: &Value| v["error"]["details"]["fields"][0]["field"].as_str().unwrap_or("").to_string();

    let (s, v) = post(json!({ "valid_from": "2026-01-01", "max_hr": 180, "resting_hr": 50, "model": "threshold" })).await;
    assert_eq!((s, field(&v)), (StatusCode::BAD_REQUEST, "threshold_hr".into()));
    let (s, v) = post(json!({ "valid_from": "2026-01-01", "max_hr": 180, "resting_hr": 50, "bounds": [50, 60, 60, 80, 90] })).await;
    assert_eq!((s, field(&v)), (StatusCode::BAD_REQUEST, "bounds".into()));
    let (s, v) = post(json!({ "valid_from": "2026-01-01", "max_hr": 180, "resting_hr": 170 })).await;
    assert_eq!((s, field(&v)), (StatusCode::BAD_REQUEST, "resting_hr".into()));
    let (s, v) = post(json!({ "sport_class": "golf", "valid_from": "2026-01-01", "max_hr": 180, "resting_hr": 50 })).await;
    assert_eq!((s, field(&v)), (StatusCode::BAD_REQUEST, "sport_class".into()));
    let (s, _) = post(json!({ "valid_from": "2026-01-01", "max_hr": 180, "resting_hr": 50 })).await;
    assert_eq!(s, StatusCode::CREATED);
    let (s, v) = post(json!({ "valid_from": "2026-01-01", "max_hr": 175, "resting_hr": 50 })).await;
    assert_eq!((s, v["error"]["code"].as_str()), (StatusCode::CONFLICT, Some("hr_zone_set_date_taken")));
    // Gleiches Datum für eine Sportklasse ist erlaubt
    let (s, _) = post(json!({ "sport_class": "running", "valid_from": "2026-01-01", "max_hr": 185, "resting_hr": 50 })).await;
    assert_eq!(s, StatusCode::CREATED);

    // Vorschlag aus dem Geburtstag
    let (s, _) = send(&f, Method::PATCH, "/api/v1/profile", Some(json!({ "birthday": "1968-01-01" }))).await;
    assert_eq!(s, StatusCode::OK);
    let (_, v) = send(&f, Method::GET, "/api/v1/hr-zones", None).await;
    assert_eq!(v["data"].as_array().unwrap().len(), 2);
    assert!(v["meta"]["suggestion"]["max_hr"].as_u64().unwrap() < 175);
}

#[tokio::test]
async fn settings_and_form_curve_with_forecast() {
    let f = fixture().await;
    let s = get(&f, "/api/v1/form/settings").await;
    assert_eq!((s["sex_factor"].as_f64(), s["fitness_days"].as_u64(), s["fatigue_days"].as_u64()), (Some(1.92), Some(42), Some(7)));
    assert_eq!(s["estimate_per_hour"]["studio"], 40.0);

    let (st, v) = send(&f, Method::PATCH, "/api/v1/form/settings", Some(json!({ "fitness_days": 20, "fatigue_days": 25 }))).await;
    assert_eq!((st, v["error"]["details"]["fields"][0]["code"].as_str()), (StatusCode::BAD_REQUEST, Some("not_below_fitness")));
    let (st, _) = send(&f, Method::PATCH, "/api/v1/form/settings", Some(json!({ "sex_factor": 2.5 }))).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    let (st, _) = send(&f, Method::PATCH, "/api/v1/form/settings", Some(json!({ "estimate_per_hour": { "golf": 3 } }))).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);

    // Zwei Fahrten ohne HF à 60 min
    import(&f, &gpx("cycling", T0, 361, None)).await;
    import(&f, &gpx("cycling", T0 + 2 * 86_400, 361, None)).await;
    let v = get(&f, "/api/v1/form?from=2026-06-30&to=2026-07-04").await;
    assert_eq!(v["pending"], 2);
    refresh(&f).await;

    let v = get(&f, "/api/v1/form?from=2026-06-30&to=2026-07-04").await;
    assert_eq!(v["pending"], 0);
    assert_eq!(v["zones_configured"], false);
    assert_eq!(v["first_date"], "2026-07-01");
    let days = v["days"].as_array().unwrap();
    // Der erste Tag ist der erste mit Belastung.
    assert_eq!(days.len(), 4);
    assert_eq!(
        (days[0]["date"].as_str(), days[0]["load"].as_f64(), days[0]["estimated"].as_f64()),
        (Some("2026-07-01"), Some(50.0), Some(50.0))
    );
    assert_eq!(days[0]["form"], 0.0);
    assert!(days[1]["form"].as_f64().unwrap() < 0.0);
    assert_eq!(v["forecast"]["days"], json!([]), "Zeitraum endet vor heute");

    // Faktor ändern rechnet beide neu.
    let (st, v) = send(&f, Method::PATCH, "/api/v1/form/settings", Some(json!({ "estimate_per_hour": { "cycling": 80 } }))).await;
    assert_eq!(st, StatusCode::OK, "{v}");
    assert_eq!(refresh(&f).await, 2);
    let v = get(&f, "/api/v1/form?from=2026-07-01&to=2026-07-01").await;
    assert_eq!(v["days"][0]["load"], 80.0);

    // Vorschau ab morgen
    let v = get(&f, "/api/v1/form?forecast=rest&forecast_days=14").await;
    let fc = v["forecast"]["days"].as_array().unwrap();
    assert_eq!((fc.len(), v["forecast"]["mode"].as_str(), v["forecast"]["daily_load"].as_f64()), (14, Some("rest"), Some(0.0)));
    assert!(fc[0]["date"].as_str().unwrap() > v["today"].as_str().unwrap());
    let v = get(&f, "/api/v1/form?forecast=steady&forecast_days=0").await;
    assert_eq!(v["forecast"]["days"], json!([]));

    let (st, _) = send(&f, Method::GET, "/api/v1/form?forecast_days=91", None).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    let (st, _) = send(&f, Method::GET, "/api/v1/form?from=2026-07-02&to=2026-07-01", None).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    let (st, _) = send(&f, Method::GET, "/api/v1/form?season=1", None).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
}
