//! Segmente über die API (M3b): Anlegen aus einer Aktivität, Auswertung als Aufträge, Rangliste mit abgeleiteten
//! Bestzeiten, Strecken, Ändern, Löschen (RFC-0016, RFC-0009, RFC-0007, OpenAPI „Segments“, „Jobs“).

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
use trackcraft::{jobs, startup};

const HOST: &str = "trackcraft.test";
const ORIGIN: &str = "https://trackcraft.test";
const T0: i64 = 1_782_892_800; // 2026-07-01T08:00:00Z
const DAY: i64 = 86_400;

struct Fixture {
    _dir: tempfile::TempDir,
    cfg: Arc<Validated>,
    app: Router,
    db: Db,
}

async fn fixture_with(extra: &str) -> Fixture {
    let dir = common::temp();
    let text = common::config_text(dir.path(), "", extra);
    let cfg = config::parse(&dir.path().join("t.toml"), &text).unwrap();
    let started = startup::prepare(&cfg).await.unwrap();
    let db = started.db.clone();
    drop(started);
    let cfg = Arc::new(cfg);
    let app = api::router(AppState { cfg: cfg.clone(), db: db.clone(), garmin: Default::default() });
    Fixture { _dir: dir, cfg, app, db }
}

async fn fixture() -> Fixture {
    fixture_with("").await
}

async fn send(app: &Router, req: Request<Body>) -> (StatusCode, Value) {
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

async fn get(f: &Fixture, path: &str) -> (StatusCode, Value) {
    send(&f.app, Request::builder().uri(path).header("host", HOST).body(Body::empty()).unwrap()).await
}

async fn with_body(f: &Fixture, method: Method, path: &str, body: Value) -> (StatusCode, Value) {
    let req = Request::builder()
        .method(method)
        .uri(path)
        .header("host", HOST)
        .header("origin", ORIGIN)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    send(&f.app, req).await
}

async fn post_empty(f: &Fixture, path: &str) -> (StatusCode, Value) {
    let req = Request::builder().method(Method::POST).uri(path).header("host", HOST).header("origin", ORIGIN).body(Body::empty()).unwrap();
    send(&f.app, req).await
}

async fn delete(f: &Fixture, path: &str) -> (StatusCode, Value) {
    let req =
        Request::builder().method(Method::DELETE).uri(path).header("host", HOST).header("origin", ORIGIN).body(Body::empty()).unwrap();
    send(&f.app, req).await
}

fn code(v: &Value) -> &str {
    v["error"]["code"].as_str().unwrap_or("")
}

/// Fahrt nach Norden: `n` Punkte im Abstand von gut 33 m und `interval` Sekunden, Höhe steigt je Punkt um 1 m.
fn gpx(name: &str, kind: &str, start: i64, n: usize, interval: i64) -> Vec<u8> {
    let mut s = format!(
        "<?xml version=\"1.0\"?>\n<gpx version=\"1.1\" creator=\"test\" xmlns=\"http://www.topografix.com/GPX/1/1\">\n<trk><name>{name}</name><type>{kind}</type><trkseg>\n"
    );
    for i in 0..n {
        let t = chrono::DateTime::from_timestamp(start + i as i64 * interval, 0).unwrap();
        s.push_str(&format!(
            "<trkpt lat=\"{:.6}\" lon=\"11.000000\"><ele>{}</ele><time>{}</time></trkpt>\n",
            47.0 + i as f64 * 0.0003,
            500 + i,
            t.format("%Y-%m-%dT%H:%M:%SZ")
        ));
    }
    s.push_str("</trkseg></trk></gpx>\n");
    s.into_bytes()
}

const BOUNDARY: &str = "----trackcraft-test";

async fn import(f: &Fixture, name: &str, bytes: &[u8]) -> String {
    let mut body = format!(
        "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{name}\"\r\nContent-Type: application/octet-stream\r\n\r\n"
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
    let (s, v) = send(&f.app, req).await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    let id = v["data"]["id"].as_str().unwrap().to_string();
    let decision = json!({ "state_token": v["data"]["state_token"], "sessions": [{ "session_index": 0, "action": "import" }] });
    let (s, v) = with_body(f, Method::POST, &format!("/api/v1/import/jobs/{id}/confirm"), decision).await;
    assert_eq!(s, StatusCode::ACCEPTED, "{v}");
    v["data"]["imported_activity_ids"][0].as_str().unwrap().to_string()
}

async fn run_jobs(f: &Fixture) {
    jobs::run_all(&f.cfg, &f.db).await.unwrap();
}

async fn create(f: &Fixture, body: Value) -> (StatusCode, Value) {
    with_body(f, Method::POST, "/api/v1/segments/from-activity", body).await
}

/// Drei Radfahrten über dieselbe Straße: A (200 s), B einen Tag später schneller (180 s), C langsamer (220 s).
async fn three_rides(f: &Fixture) -> (String, String, String) {
    let a = import(f, "a.gpx", &gpx("A", "cycling", T0, 40, 10)).await;
    let b = import(f, "b.gpx", &gpx("B", "cycling", T0 + DAY, 40, 9)).await;
    let c = import(f, "c.gpx", &gpx("C", "cycling", T0 + 2 * DAY, 40, 11)).await;
    run_jobs(f).await;
    (a, b, c)
}

#[tokio::test]
async fn create_evaluate_history_and_derive_records() {
    let f = fixture().await;
    let (a, b, c) = three_rides(&f).await;
    let run = import(&f, "lauf.gpx", &gpx("Lauf", "running", T0 + 3 * DAY, 40, 20)).await;
    run_jobs(&f).await;

    let (s, v) = create(
        &f,
        json!({ "activity_id": a, "section_index": 0, "start_index": 5, "end_index": 25, "name": " Anstieg ", "sport_class": "cycling" }),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    let seg = &v["data"];
    let id = seg["id"].as_str().unwrap().to_string();
    assert_eq!(seg["name"], "Anstieg");
    assert_eq!(seg["kind"], "segment");
    assert!((seg["length_m"].as_f64().unwrap() - 667.0).abs() < 2.0, "{}", seg["length_m"]);
    assert_eq!(seg["history"]["status"], "running");
    assert_eq!(seg["elevation_gain_m"], 18.0); // 20 m in 1-m-Schritten, Hysterese 3 m
    assert_eq!(seg["summary"]["attempt_count"], 0);

    run_jobs(&f).await;
    let (_, v) = get(&f, &format!("/api/v1/segments/{id}")).await;
    let seg = &v["data"];
    assert_eq!(seg["history"]["status"], "complete");
    assert_eq!(seg["history"]["total_items"], 3, "nur Radfahrten");
    assert_eq!(seg["summary"]["valid_attempt_count"], 3);
    assert_eq!(seg["summary"]["best"]["activity_id"], b.as_str());
    assert_eq!(seg["summary"]["best"]["duration_ms"], 180_000);
    assert_eq!(seg["summary"]["third"]["activity_id"], c.as_str());
    assert_eq!(seg["summary"]["average_duration_ms"], 200_000.0);
    assert_eq!(seg["summary"]["first_attempt_time"], "2026-07-01T08:00:50.000Z");

    let (s, v) = get(&f, &format!("/api/v1/segments/{id}/leaderboard")).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let rows = v["data"].as_array().unwrap();
    let order: Vec<(&str, i64)> = rows.iter().map(|r| (r["activity_id"].as_str().unwrap(), r["rank"].as_i64().unwrap())).collect();
    assert_eq!(order, [(b.as_str(), 1), (a.as_str(), 2), (c.as_str(), 3)]);
    // Bestzeitenart nach Versuchszeit: A erste Bestzeit, B Verbesserung um 20 s, C keine
    assert_eq!(rows[1]["record_kind"], "first_best");
    assert_eq!(rows[0]["record_kind"], "improvement");
    assert_eq!(rows[0]["improvement_ms"], 20_000);
    assert_eq!(rows[2]["record_kind"], Value::Null);
    assert_eq!(rows[0]["avg_heart_rate_bpm"], Value::Null);
    assert_eq!(v["meta"]["calculation"]["status"], "current");

    // Monats- und Jahreswertung nach lokalem Segmentstart
    let (_, v) = get(&f, &format!("/api/v1/segments/{id}/leaderboard?period=month&year=2026&month=7")).await;
    assert_eq!(v["meta"]["total"], 3);
    let (_, v) = get(&f, &format!("/api/v1/segments/{id}/leaderboard?period=year&year=2025")).await;
    assert_eq!(v["meta"]["total"], 0);
    let (s, v) = get(&f, &format!("/api/v1/segments/{id}/leaderboard?period=month&year=2026")).await;
    assert_eq!((s, code(&v)), (StatusCode::BAD_REQUEST, "validation_failed"));

    // Saisonwertung nach lokalem Segmentstart, letzter Tag eingeschlossen: B (02.07.) und C (03.07.)
    let (s, v) =
        with_body(&f, Method::POST, "/api/v1/seasons", json!({ "name": "Juli", "start_date": "2026-07-02", "end_date": "2026-07-03" }))
            .await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    let season = v["data"]["id"].as_str().unwrap().to_string();
    let (_, v) = get(&f, &format!("/api/v1/segments/{id}/leaderboard?period=season&season_id={season}")).await;
    let order: Vec<(&str, i64)> =
        v["data"].as_array().unwrap().iter().map(|r| (r["activity_id"].as_str().unwrap(), r["rank"].as_i64().unwrap())).collect();
    assert_eq!(order, [(b.as_str(), 1), (c.as_str(), 2)]);
    let (s, v) = get(&f, &format!("/api/v1/segments/{id}/leaderboard?period=season")).await;
    assert_eq!((s, code(&v)), (StatusCode::BAD_REQUEST, "validation_failed"));
    let (s, v) = get(&f, &format!("/api/v1/segments/{id}/leaderboard?period=season&season_id={a}")).await;
    assert_eq!((s, code(&v)), (StatusCode::NOT_FOUND, "season_not_found"));

    // Versuche der Aktivität und Segment-Erfolge in der Liste
    let (_, v) = get(&f, &format!("/api/v1/activities/{b}/efforts")).await;
    assert_eq!(v["data"][0]["segment_name"], "Anstieg");
    assert_eq!(v["data"][0]["rank"], 1);
    assert_eq!(v["data"][0]["record_kind"], "improvement");
    let (_, v) = get(&f, "/api/v1/activities?sport_class=cycling").await;
    let success: Vec<i64> = v["data"].as_array().unwrap().iter().map(|x| x["segment_success_count"].as_i64().unwrap()).collect();
    assert_eq!(success, [0, 1, 1], "C, B, A");
    assert_eq!(v["meta"]["summary"]["segment_success_count"], 2);
    let (_, v) = get(&f, &format!("/api/v1/activities/{run}/efforts")).await;
    assert_eq!(v["data"], json!([]));

    // Spätere Fahrt wird beim Import ausgewertet, ohne die Historie neu zu rechnen
    let d = import(&f, "d.gpx", &gpx("D", "cycling", T0 + 4 * DAY, 40, 8)).await;
    run_jobs(&f).await;
    let (_, v) = get(&f, &format!("/api/v1/segments/{id}/leaderboard?limit=1")).await;
    assert_eq!(v["data"][0]["activity_id"], d.as_str());
    assert_eq!(v["data"][0]["improvement_ms"], 20_000);
    assert_eq!(v["meta"]["total"], 4);

    // Sportklasse der Aktivität ändern: Wertung entfällt nach der Auswertung
    let (s, _) = with_body(&f, Method::PATCH, &format!("/api/v1/activities/{d}"), json!({ "sport_class": "running" })).await;
    assert_eq!(s, StatusCode::OK);
    run_jobs(&f).await;
    let (_, v) = get(&f, &format!("/api/v1/segments/{id}/attempts")).await;
    assert_eq!(v["meta"]["total"], 3);
    assert_eq!(v["data"][0]["activity_id"], c.as_str(), "neuester zuerst");
}

#[tokio::test]
async fn validation_of_new_segments() {
    let f = fixture_with("[segments]\nminimum_length_m = 100.0\n").await;
    let a = import(&f, "a.gpx", &gpx("A", "cycling", T0, 40, 10)).await;
    let base = json!({ "activity_id": a, "section_index": 0, "start_index": 5, "end_index": 25, "name": "S", "sport_class": "cycling" });
    let with = |k: &str, v: Value| {
        let mut b = base.clone();
        b[k] = v;
        b
    };

    let (s, v) = create(&f, with("activity_id", json!("0198c1f2-6b3a-7c1e-9d44-2f6a1b8e5a10"))).await;
    assert_eq!((s, code(&v)), (StatusCode::NOT_FOUND, "activity_not_found"));
    for (field, value, expected) in [
        ("section_index", json!(3), "section_index"),
        ("start_index", json!(99), "start_index"),
        ("end_index", json!(5), "end_index"),
        ("name", json!("  "), "name"),
        ("sport_class", json!("unknown"), "sport_class"),
        ("kind", json!("loop"), "kind"),
    ] {
        let (s, v) = create(&f, with(field, value)).await;
        assert_eq!((s, code(&v)), (StatusCode::BAD_REQUEST, "validation_failed"), "{field}: {v}");
        assert_eq!(v["error"]["details"]["fields"][0]["field"], expected);
    }
    // 3 Punkte = gut 67 m unter 100 m; 4 Punkte = 100,1 m genügen
    let (s, v) = create(&f, with("end_index", json!(7))).await;
    assert_eq!((s, code(&v)), (StatusCode::BAD_REQUEST, "segment_too_short"));
    let (s, v) = create(&f, with("end_index", json!(8))).await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    let (s, v) = create(&f, json!({ "activity_id": a, "section_index": 0, "start_index": 5, "end_index": 25, "name": "S", "sport_class": "cycling", "extra": 1 })).await;
    assert_eq!((s, code(&v)), (StatusCode::BAD_REQUEST, "validation_failed"));
}

#[tokio::test]
async fn route_update_filter_and_delete() {
    let f = fixture().await;
    let (a, b, _c) = three_rides(&f).await;
    // Strecke: Start und Ziel höchstens route_edge_m (1000 m) entlang der Aufzeichnung vom Rand; Punkte liegen 33,4 m auseinander
    let route_at = |s: i64, e: i64| json!({ "activity_id": a, "section_index": 0, "start_index": s, "end_index": e, "name": "Hausrunde", "sport_class": "cycling", "kind": "route" });
    let (s, v) = create(&f, route_at(30, 39)).await;
    assert_eq!((s, code(&v)), (StatusCode::BAD_REQUEST, "validation_failed"), "{v}");
    assert_eq!(v["error"]["details"]["fields"][0]["code"], "route_edge_exceeded", "{v}");
    let (s, v) = create(&f, route_at(0, 8)).await;
    assert_eq!(v["error"]["details"]["fields"][0]["field"], "end_index", "{s} {v}");
    // Ganze Aktivität als Strecke
    let (s, v) = create(&f, route_at(0, 39)).await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    let route = v["data"]["id"].as_str().unwrap().to_string();
    assert_eq!(v["data"]["kind"], "route");
    assert!((v["data"]["length_m"].as_f64().unwrap() - 1301.0).abs() < 3.0, "{}", v["data"]["length_m"]);
    let (_, v) = create(
        &f,
        json!({ "activity_id": a, "section_index": 0, "start_index": 2, "end_index": 12, "name": "Mitte", "sport_class": "cycling" }),
    )
    .await;
    let seg = v["data"]["id"].as_str().unwrap().to_string();
    run_jobs(&f).await;

    // Ähnliche Strecke über den Verlauf: Ränder etwas anders gewählt, gleiche Linie
    let similar = |s: i64, e: i64, sport: &str| json!({ "activity_id": b, "section_index": 0, "start_index": s, "end_index": e, "sport_class": sport });
    let (s, v) = with_body(&f, Method::POST, "/api/v1/segments/similar", similar(1, 38, "cycling")).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["data"].as_array().map(Vec::len), Some(1), "{v}");
    assert_eq!(v["data"][0]["id"], route.as_str());
    assert!(v["data"][0]["overlap"].as_f64().unwrap() >= 0.9, "{v}");
    // Nur die erste Hälfte: zu wenig Überdeckung; andere Sportklasse: keine Kandidaten
    let (_, v) = with_body(&f, Method::POST, "/api/v1/segments/similar", similar(0, 20, "cycling")).await;
    assert_eq!(v["data"], json!([]), "{v}");
    let (_, v) = with_body(&f, Method::POST, "/api/v1/segments/similar", similar(1, 38, "running")).await;
    assert_eq!(v["data"], json!([]), "{v}");
    let (s, v) = with_body(&f, Method::POST, "/api/v1/segments/similar", similar(20, 10, "cycling")).await;
    assert_eq!((s, code(&v)), (StatusCode::BAD_REQUEST, "validation_failed"), "{v}");

    // Passt eine Aktivität schon zu einer Strecke, gibt es keine weitere Strecke daraus (Uli 09.11.2026)
    let (s, v) = create(&f, json!({ "activity_id": b, "section_index": 0, "start_index": 0, "end_index": 39, "name": "Doppelt", "sport_class": "cycling", "kind": "route" })).await;
    assert_eq!((s, code(&v)), (StatusCode::CONFLICT, "route_already_matched"), "{v}");

    // Versuche der Aktivität tragen die Kurzangaben des Segments für die Segmenttabelle
    let (_, v) = get(&f, &format!("/api/v1/activities/{a}/efforts")).await;
    let r = v["data"].as_array().unwrap().iter().find(|e| e["segment_id"] == route.as_str()).expect("Streckenversuch");
    assert_eq!(r["segment"]["kind"], "route");
    assert_eq!(r["segment"]["sport_class"], "cycling");
    assert_eq!(r["segment"]["ranked_count"], 3);
    assert_eq!(r["segment"]["elevation_gain_m"], 39.0, "{r}");
    assert_eq!(r["segment"]["average_grade_pct"], 3.0, "{r}");

    // Liste: Reiter Segmente und Strecken, Suche, Sortierung
    let (_, v) = get(&f, "/api/v1/segments?kind=route").await;
    assert_eq!(v["meta"]["total"], 1);
    assert_eq!(v["data"][0]["name"], "Hausrunde");
    assert_eq!(v["data"][0]["average_grade_pct"], 3.0, "{v}");
    // Kennzahlen über alle gefilterten Segmente (RFC-0006 „Segmentliste“)
    let sum = &v["meta"]["summary"];
    assert_eq!((sum["count"].as_i64(), sum["attempt_count"].as_i64()), (Some(1), Some(3)), "{sum}");
    assert!((sum["length_m"].as_f64().unwrap() - 1301.0).abs() < 3.0, "{sum}");
    let route_improvements = sum["improvement_count"].as_i64().expect("improvement_count");
    let (_, v) = get(&f, "/api/v1/segments").await;
    let sum = &v["meta"]["summary"];
    assert_eq!((sum["count"].as_i64(), sum["attempt_count"].as_i64()), (Some(2), Some(6)), "{sum}");
    assert!(sum["improvement_count"].as_i64().unwrap() >= route_improvements, "{sum}");
    let (_, v) = get(&f, "/api/v1/segments?sort=-length_m").await;
    let names: Vec<&str> = v["data"].as_array().unwrap().iter().map(|s| s["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["Hausrunde", "Mitte"]);
    let (_, v) = get(&f, "/api/v1/segments?q=MIT").await;
    assert_eq!(v["meta"]["total"], 1);
    let (s, v) = get(&f, "/api/v1/segments?sport_class=unknown").await;
    assert_eq!((s, code(&v)), (StatusCode::BAD_REQUEST, "validation_failed"));

    // Strecken-Filter der Aktivitätenliste: alle mit gültigem Versuch, nach Zeit sortiert der Vergleich
    let (_, v) = get(&f, &format!("/api/v1/activities?route_id={route}&sort=-start_time")).await;
    assert_eq!(v["meta"]["total"], 3);

    // Referenz spaltenweise, ausgedünnt
    let (_, v) = get(&f, &format!("/api/v1/segments/{route}/track?max_points=5")).await;
    let t = &v["data"];
    assert_eq!((t["point_count"].as_i64(), t["returned_point_count"].as_i64(), t["simplified"].as_bool()), (Some(40), Some(5), Some(true)));
    assert_eq!(t["index"][4], 39);
    assert_eq!(t["ele_m"][0], 500.0);
    assert!((t["dist_m"][4].as_f64().unwrap() - 1301.0).abs() < 3.0);

    // Art und Name ändern; nur der Artwechsel wertet die Historie neu aus (Strecke: Aufzeichnungsrand zählt). Sportklasse ist fest
    let (s, v) = with_body(
        &f,
        Method::PATCH,
        &format!("/api/v1/segments/{seg}"),
        json!({ "name": "Mitte neu", "kind": "route", "description": "kurz" }),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(
        (v["data"]["name"].as_str(), v["data"]["kind"].as_str(), v["data"]["description"].as_str()),
        (Some("Mitte neu"), Some("route"), Some("kurz"))
    );
    let (_, jobs_v) = get(&f, "/api/v1/jobs?kind=segment_history&status=queued").await;
    assert_eq!(jobs_v["meta"]["total"], 1);
    let (_, v) = with_body(&f, Method::PATCH, &format!("/api/v1/segments/{seg}"), json!({ "name": "Mitte" })).await;
    assert_eq!(v["data"]["history"]["status"], "running", "noch nicht ausgewertet");
    run_jobs(&f).await;
    let (_, jobs_v) = get(&f, "/api/v1/jobs?kind=segment_history&status=queued").await;
    assert_eq!(jobs_v["meta"]["total"], 0);
    let (s, v) = with_body(&f, Method::PATCH, &format!("/api/v1/segments/{seg}"), json!({ "sport_class": "running" })).await;
    assert_eq!((s, code(&v)), (StatusCode::BAD_REQUEST, "validation_failed"));

    // Löschen mit Vorschau: Versuche weg, Aktivitäten bleiben
    let (_, v) = get(&f, &format!("/api/v1/segments/{seg}/deletion-preview")).await;
    assert_eq!(
        v["data"],
        json!({ "object_type": "segment", "effort_count": 3, "affected_segment_count": 0, "original_file_removed": null })
    );
    let (s, _) = delete(&f, &format!("/api/v1/segments/{seg}")).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, v) = get(&f, &format!("/api/v1/segments/{seg}")).await;
    assert_eq!((s, code(&v)), (StatusCode::NOT_FOUND, "segment_not_found"));
    let (_, v) = get(&f, &format!("/api/v1/activities/{b}/efforts")).await;
    assert_eq!(v["data"].as_array().unwrap().len(), 1, "nur noch die Strecke");
    let (s, _) = get(&f, &format!("/api/v1/activities/{b}")).await;
    assert_eq!(s, StatusCode::OK);

    // Quellaktivität löschen: Segment bleibt
    let (s, _) = delete(&f, &format!("/api/v1/activities/{a}")).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    run_jobs(&f).await;
    let (_, v) = get(&f, &format!("/api/v1/segments/{route}")).await;
    assert_eq!(v["data"]["summary"]["valid_attempt_count"], 2);
}

#[tokio::test]
async fn category_and_location() {
    let f = fixture().await;
    let (a, _b, _c) = three_rides(&f).await;
    // Fahrt nach Norden mit 3 % Steigung: Anstieg; als Strecke: Start und Ziel 1,3 km auseinander
    let (s, v) = create(
        &f,
        json!({ "activity_id": a, "section_index": 0, "start_index": 2, "end_index": 12, "name": "Mitte", "sport_class": "cycling" }),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    let seg = v["data"]["id"].as_str().unwrap().to_string();
    assert_eq!((v["data"]["category"].as_str(), v["data"]["category_auto"].as_str()), (Some("climb"), Some("climb")));
    assert_eq!(v["data"]["location"], Value::Null);
    let (s, v) = create(
        &f,
        json!({ "activity_id": a, "section_index": 0, "start_index": 0, "end_index": 39, "name": "Runde", "sport_class": "cycling",
                "kind": "route", "location": " Oberndorf ", "category": "loop" }),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    let route = v["data"]["id"].as_str().unwrap().to_string();
    assert_eq!(
        (v["data"]["category"].as_str(), v["data"]["category_auto"].as_str(), v["data"]["location"].as_str()),
        (Some("loop"), Some("point_to_point"), Some("Oberndorf"))
    );
    // Kategorie muss zur Art passen
    let (s, v) = create(
        &f,
        json!({ "activity_id": a, "section_index": 0, "start_index": 2, "end_index": 12, "name": "X", "sport_class": "cycling", "category": "loop" }),
    )
    .await;
    assert_eq!((s, code(&v)), (StatusCode::BAD_REQUEST, "validation_failed"), "{v}");

    // Filter nach wirksamer Kategorie, Suche auch im Ort
    let (_, v) = get(&f, "/api/v1/segments?category=loop").await;
    assert_eq!(v["meta"]["total"], 1);
    let (_, v) = get(&f, "/api/v1/segments?category=point_to_point").await;
    assert_eq!(v["meta"]["total"], 0);
    let (_, v) = get(&f, "/api/v1/segments?q=oberndorf").await;
    assert_eq!(v["data"][0]["id"], route.as_str());
    let (s, v) = get(&f, "/api/v1/segments?category=hill").await;
    assert_eq!((s, code(&v)), (StatusCode::BAD_REQUEST, "validation_failed"));
    let (s, _) = get(&f, "/api/v1/segments?sort=location").await;
    assert_eq!(s, StatusCode::OK);

    // Zurück zur automatischen Kategorie, Ort löschen
    let (s, v) = with_body(&f, Method::PATCH, &format!("/api/v1/segments/{route}"), json!({ "category": null, "location": null })).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!((v["data"]["category"].as_str(), v["data"]["location"].as_str()), (Some("point_to_point"), None));
    // Von Hand abweichend; Kategorie einer anderen Art abgelehnt
    let (_, v) = with_body(&f, Method::PATCH, &format!("/api/v1/segments/{seg}"), json!({ "category": "rolling" })).await;
    assert_eq!(v["data"]["category"], "rolling");
    let (_, v) = with_body(&f, Method::PATCH, &format!("/api/v1/segments/{seg}"), json!({ "category": "loop" })).await;
    assert_eq!(code(&v), "validation_failed", "{v}");
    // Artwechsel: Kategorie neu bestimmt, Wahl von Hand verworfen; im selben Aufruf gesetzte gilt
    let (_, v) = with_body(&f, Method::PATCH, &format!("/api/v1/segments/{seg}"), json!({ "kind": "route" })).await;
    assert_eq!((v["data"]["kind"].as_str(), v["data"]["category"].as_str()), (Some("route"), Some("point_to_point")), "{v}");
    let (_, v) =
        with_body(&f, Method::PATCH, &format!("/api/v1/segments/{seg}"), json!({ "kind": "segment", "category": "descent" })).await;
    assert_eq!((v["data"]["category"].as_str(), v["data"]["category_auto"].as_str()), (Some("descent"), Some("climb")), "{v}");

    // Segmenttabelle der Aktivität trägt die Kategorie
    run_jobs(&f).await;
    let (_, v) = get(&f, &format!("/api/v1/activities/{a}/efforts")).await;
    let r = v["data"].as_array().unwrap().iter().find(|e| e["segment_id"] == seg.as_str()).expect("Versuch");
    assert_eq!(r["segment"]["category"], "descent");
}

#[tokio::test]
async fn jobs_can_be_listed_and_only_failed_ones_retried() {
    let f = fixture().await;
    import(&f, "a.gpx", &gpx("A", "cycling", T0, 40, 10)).await;
    let (_, v) = get(&f, "/api/v1/jobs?status=queued").await;
    assert_eq!(v["meta"]["total"], 1);
    let job = v["data"][0]["id"].as_str().unwrap().to_string();
    assert_eq!(v["data"][0]["kind"], "activity_evaluation");
    run_jobs(&f).await;
    let (_, v) = get(&f, &format!("/api/v1/jobs/{job}")).await;
    assert_eq!(v["data"]["status"], "done");
    assert!(v["data"]["finished_at"].is_string());
    let (s, v) = post_empty(&f, &format!("/api/v1/jobs/{job}/retry")).await;
    assert_eq!((s, code(&v)), (StatusCode::CONFLICT, "job_not_retryable"));
    let (s, v) = post_empty(&f, "/api/v1/jobs/0198c1f2-6b3a-7c1e-9d44-2f6a1b8e5a10/retry").await;
    assert_eq!((s, code(&v)), (StatusCode::NOT_FOUND, "job_not_found"));
    sqlx::query("UPDATE jobs SET status = 'failed', error_message = 'x' WHERE id = ?").bind(&job).execute(&f.db.writer).await.unwrap();
    let (s, v) = post_empty(&f, &format!("/api/v1/jobs/{job}/retry")).await;
    assert_eq!(s, StatusCode::ACCEPTED, "{v}");
    assert_eq!(v["data"]["status"], "queued");
    let (s, v) = get(&f, "/api/v1/jobs?kind=import").await;
    assert_eq!((s, code(&v)), (StatusCode::BAD_REQUEST, "validation_failed"));
}

#[tokio::test]
async fn new_calculation_version_recomputes_efforts_and_drops_the_old_one() {
    let dir = common::temp();
    let text = common::config_text(dir.path(), "", "");
    let cfg = config::parse(&dir.path().join("t.toml"), &text).unwrap();
    let started = startup::prepare(&cfg).await.unwrap();
    let db = started.db.clone();
    drop(started);
    let cfg = Arc::new(cfg);
    let f = Fixture {
        _dir: common::temp(),
        cfg: cfg.clone(),
        app: api::router(AppState { cfg: cfg.clone(), db: db.clone(), garmin: Default::default() }),
        db: db.clone(),
    };
    let (a, _, _) = three_rides(&f).await;
    let (_, v) = create(
        &f,
        json!({ "activity_id": a, "section_index": 0, "start_index": 5, "end_index": 25, "name": "S", "sport_class": "cycling" }),
    )
    .await;
    let id = v["data"]["id"].as_str().unwrap().to_string();
    run_jobs(&f).await;
    db.close().await;

    // Toleranz geändert: neuer Stand beim Start mit allen Versuchen, alter gelöscht
    let text = common::config_text(dir.path(), "", "[segments.matching]\ntolerance_m = 12.0\n");
    let cfg2 = config::parse(&dir.path().join("t.toml"), &text).unwrap();
    let started = startup::prepare(&cfg2).await.unwrap();
    let db2 = started.db.clone();
    drop(started);
    let versions: i64 = sqlx::query_scalar("SELECT count(*) FROM calculation_versions").fetch_one(&db2.reader).await.unwrap();
    assert_eq!(versions, 1);
    let efforts: i64 =
        sqlx::query_scalar("SELECT count(*) FROM segment_efforts WHERE segment_id = ?").bind(&id).fetch_one(&db2.reader).await.unwrap();
    assert_eq!(efforts, 3);
}
