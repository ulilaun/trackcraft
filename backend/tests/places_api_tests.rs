//! Ortsvorschlag (#22): eigene Daten zuerst, sonst OpenStreetMap (hier ein nachgebauter Nominatim-Dienst), abschaltbar.

mod common;

use std::sync::{Arc, Mutex};

use axum::Router;
use axum::body::Body;
use axum::extract::{RawQuery, State};
use axum::http::{Method, Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;
use trackcraft::api::{self, AppState};
use trackcraft::config;
use trackcraft::startup;

const HOST: &str = "trackcraft.test";
const ORIGIN: &str = "https://trackcraft.test";
const T0: i64 = 1_782_892_800; // 2026-07-01T08:00:00Z
const DAY: i64 = 86_400;

type Queries = Arc<Mutex<Vec<String>>>;

async fn reverse(State(q): State<Queries>, RawQuery(query): RawQuery) -> axum::Json<Value> {
    q.lock().unwrap().push(query.unwrap_or_default());
    axum::Json(json!({ "place_id": 1, "address": { "hamlet": "Unterweiler", "village": "Musterdorf", "country": "Deutschland" } }))
}

struct Fixture {
    _dir: tempfile::TempDir,
    app: Router,
    queries: Queries,
}

async fn fixture() -> Fixture {
    let queries: Queries = Default::default();
    let mock = Router::new().route("/reverse", axum::routing::get(reverse)).with_state(queries.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, mock).await.unwrap() });
    let dir = common::temp();
    let text = common::config_text(dir.path(), "", &format!("[places]\nnominatim_url = \"http://{addr}\"\n"));
    let cfg = Arc::new(config::parse(&dir.path().join("t.toml"), &text).unwrap());
    let started = startup::prepare(&cfg).await.unwrap();
    let db = started.db.clone();
    drop(started);
    let app = api::router(AppState { cfg, db, garmin: Default::default() });
    Fixture { _dir: dir, app, queries }
}

async fn send(f: &Fixture, method: Method, path: &str, body: Option<Value>) -> (StatusCode, Value) {
    let mut req = Request::builder().method(method.clone()).uri(path).header("host", HOST);
    if method != Method::GET {
        req = req.header("origin", ORIGIN).header("content-type", "application/json");
    }
    let req = req.body(body.map(|b| Body::from(b.to_string())).unwrap_or_else(Body::empty)).unwrap();
    let res = f.app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

fn gpx(lat0: f64, start: i64) -> Vec<u8> {
    let mut s = String::from(
        "<?xml version=\"1.0\"?>\n<gpx version=\"1.1\" creator=\"test\" xmlns=\"http://www.topografix.com/GPX/1/1\">\n<trk><type>cycling</type><trkseg>\n",
    );
    for i in 0..20 {
        let t = chrono::DateTime::from_timestamp(start + i * 10, 0).unwrap();
        s.push_str(&format!(
            "<trkpt lat=\"{:.6}\" lon=\"11.000000\"><ele>500</ele><time>{}</time></trkpt>\n",
            lat0 + i as f64 * 0.0003,
            t.format("%Y-%m-%dT%H:%M:%SZ")
        ));
    }
    s.push_str("</trkseg></trk></gpx>\n");
    s.into_bytes()
}

const BOUNDARY: &str = "----trackcraft-test";

async fn import(f: &Fixture, lat0: f64, start: i64) -> String {
    let mut body = format!(
        "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"t.gpx\"\r\nContent-Type: application/octet-stream\r\n\r\n"
    )
    .into_bytes();
    body.extend_from_slice(&gpx(lat0, start));
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

async fn suggestion(f: &Fixture, id: &str) -> Value {
    let (s, v) = send(f, Method::GET, &format!("/api/v1/activities/{id}/location-suggestion"), None).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    v["data"].clone()
}

#[tokio::test]
async fn own_places_first_then_openstreetmap_unless_switched_off() {
    let f = fixture().await;
    let a = import(&f, 47.0, T0).await;
    let b = import(&f, 47.0005, T0 + DAY).await;
    let (s, _) = send(&f, Method::PATCH, &format!("/api/v1/activities/{b}"), Some(json!({ "location": "Heimatort" }))).await;
    assert_eq!(s, StatusCode::OK);

    // Bekannter Ort 56 m entfernt: aus eigenen Daten, ohne Netz
    let v = suggestion(&f, &a).await;
    assert_eq!((v["location"].as_str(), v["source"].as_str()), (Some("Heimatort"), Some("own")), "{v}");
    assert_eq!(v["distance_m"], 56.0);
    assert!(f.queries.lock().unwrap().is_empty());

    // Weit weg: OpenStreetMap, Gemeinde vor Ortsteil, Koordinate auf vier Stellen
    let far = import(&f, 46.123456, T0 + 2 * DAY).await;
    let v = suggestion(&f, &far).await;
    assert_eq!((v["location"].as_str(), v["source"].as_str()), (Some("Musterdorf"), Some("osm")), "{v}");
    {
        let q = f.queries.lock().unwrap();
        assert_eq!(q.len(), 1);
        assert!(q[0].contains("lat=46.1235&lon=11.0000"), "{}", q[0]);
    }
    // Zweites Mal aus dem Zwischenspeicher
    suggestion(&f, &far).await;
    assert_eq!(f.queries.lock().unwrap().len(), 1);

    // Abgeschaltet: kein Abruf, kein Vorschlag
    let (s, v) = send(&f, Method::GET, "/api/v1/places/settings", None).await;
    assert_eq!((s, v["data"]["osm_enabled"].as_bool()), (StatusCode::OK, Some(true)));
    let (s, v) = send(&f, Method::PATCH, "/api/v1/places/settings", Some(json!({ "osm_enabled": false }))).await;
    assert_eq!((s, v["data"]["osm_enabled"].as_bool()), (StatusCode::OK, Some(false)), "{v}");
    let other = import(&f, 45.5, T0 + 3 * DAY).await;
    assert_eq!(suggestion(&f, &other).await, Value::Null);
    assert_eq!(f.queries.lock().unwrap().len(), 1);
    // Eigene Daten gelten weiter
    assert_eq!(suggestion(&f, &a).await["location"], "Heimatort");

    // Fehler
    let (s, _) = send(&f, Method::GET, "/api/v1/activities/unbekannt/location-suggestion", None).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, _) = send(&f, Method::PATCH, "/api/v1/places/settings", Some(json!({ "osm_enabled": "ja" }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = send(&f, Method::PATCH, "/api/v1/places/settings", Some(json!({ "osm_enabled": true, "foo": 1 }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}
