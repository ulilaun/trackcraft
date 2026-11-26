//! Aktivitäten-API (M2a): Liste mit Summen und Filtern, Detail, Ändern, Löschen, Trackdaten, Dashboard und
//! Berechnungsstand mit Hysterese (RFC-0003, RFC-0006, RFC-0017, RFC-0018, RFC-0007).

mod common;

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use chrono::NaiveDate;
use serde_json::{Value, json};
use trackcraft::api::{self, AppState};
use trackcraft::config::{self, Validated};
use trackcraft::db::Db;
use trackcraft::{activities, calculation, startup};

const HOST: &str = "trackcraft.test";
const ORIGIN: &str = "https://trackcraft.test";
const T0: i64 = 1_782_892_800; // 2026-07-01T08:00:00Z, Mittwoch

struct Fixture {
    dir: tempfile::TempDir,
    cfg: Arc<Validated>,
    app: Router,
    db: Db,
}

async fn fixture_in(dir: tempfile::TempDir, extra: &str) -> Fixture {
    let text = common::config_text(dir.path(), "", extra);
    let cfg = config::parse(&dir.path().join("t.toml"), &text).unwrap();
    let started = startup::prepare(&cfg).await.unwrap();
    let db = started.db.clone();
    drop(started);
    let cfg = Arc::new(cfg);
    let app = api::router(AppState { cfg: cfg.clone(), db: db.clone(), garmin: Default::default(), backup: Default::default() });
    Fixture { dir, cfg, app, db }
}

async fn fixture() -> Fixture {
    fixture_in(common::temp(), "").await
}

async fn send(app: &Router, req: Request<Body>) -> (StatusCode, Value) {
    common::contract::call_json(app, req).await
}

fn code(v: &Value) -> &str {
    v["error"]["code"].as_str().unwrap_or("")
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

async fn delete(f: &Fixture, path: &str) -> (StatusCode, Value) {
    let req =
        Request::builder().method(Method::DELETE).uri(path).header("host", HOST).header("origin", ORIGIN).body(Body::empty()).unwrap();
    send(&f.app, req).await
}

/// GPX mit `n` Punkten im Abstand von 10 s und gut 33 m, Höhe steigt je Punkt um 1 m; ohne Aufstieg aus der Datei.
fn gpx(name: &str, kind: &str, start: i64, n: usize) -> Vec<u8> {
    let mut s = format!(
        "<?xml version=\"1.0\"?>\n<gpx version=\"1.1\" creator=\"test\" xmlns=\"http://www.topografix.com/GPX/1/1\">\n<trk><name>{name}</name><type>{kind}</type><trkseg>\n"
    );
    for i in 0..n {
        let t = chrono::DateTime::from_timestamp(start + i as i64 * 10, 0).unwrap();
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

/// Hochladen und bestätigen; liefert die ID der Aktivität.
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

async fn count(db: &Db, sql: &str) -> i64 {
    sqlx::query_scalar(sqlx::AssertSqlSafe(sql.to_string())).fetch_one(&db.reader).await.unwrap()
}

#[tokio::test]
async fn list_with_summary_filters_and_sort() {
    let f = fixture().await;
    let a = import(&f, "a.gpx", &gpx("Morgenrunde", "cycling", T0, 60)).await; // 590 s
    let b = import(&f, "b.gpx", &gpx("Abendrunde am See", "cycling", T0 + 86_400, 121)).await; // 1200 s
    let c = import(&f, "c.gpx", &gpx("Lauf", "running", T0 + 2 * 86_400, 31)).await; // 300 s

    let (s, v) = get(&f, "/api/v1/activities").await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let ids: Vec<&str> = v["data"].as_array().unwrap().iter().map(|a| a["id"].as_str().unwrap()).collect();
    assert_eq!(ids, [c.as_str(), b.as_str(), a.as_str()]);
    let first = &v["data"][2];
    assert_eq!(first["title"], "Morgenrunde");
    assert_eq!(first["start_time"], "2026-07-01T08:00:00.000Z");
    assert_eq!(first["total_time_ms"], 590_000);
    assert_eq!(first["distance_origin"], "gps_points");
    // Kein Dateiwert: Aufstieg aus der Hysterese (59 m in 1-m-Schritten, Schwelle 3 m: 19 × 3 = 57 m)
    assert_eq!(first["elevation_gain_m"], 57.0);
    assert_eq!(first["elevation_origin"], "hysteresis");
    assert_eq!(first["has_position"], true);
    assert_eq!(first["section_count"], 1);
    assert_eq!(first["point_count"], 60);
    assert!((first["avg_speed_kmh"].as_f64().unwrap() - 11.9).abs() < 0.3, "{}", first["avg_speed_kmh"]);

    let m = &v["meta"];
    assert_eq!((m["total"].as_i64(), m["limit"].as_i64(), m["offset"].as_i64()), (Some(3), Some(50), Some(0)));
    assert_eq!(m["summary"]["count"], 3);
    assert_eq!(m["summary"]["total_time_ms"], 2_090_000);
    assert_eq!(m["summary"]["mixed_sport_classes"], true);
    assert_eq!(m["summary"]["avg_speed_kmh"], Value::Null);
    assert_eq!(m["calculation"]["status"], "current");

    // Filter: Sportklasse, Suche, Zeitraum (lokale Tage), Paging
    let (_, v) = get(&f, "/api/v1/activities?sport_class=cycling").await;
    assert_eq!(v["meta"]["total"], 2);
    assert_eq!(v["meta"]["summary"]["mixed_sport_classes"], false);
    assert!(v["meta"]["summary"]["avg_speed_kmh"].as_f64().unwrap() > 10.0);
    let (_, v) = get(&f, "/api/v1/activities?q=see").await;
    assert_eq!(v["data"][0]["id"], b.as_str());
    let (_, v) = get(&f, "/api/v1/activities?from=2026-07-02&to=2026-07-02").await;
    assert_eq!(v["meta"]["total"], 1);
    assert_eq!(v["data"][0]["id"], b.as_str());
    let (_, v) = get(&f, "/api/v1/activities?limit=1&offset=1").await;
    assert_eq!((v["data"].as_array().unwrap().len(), v["meta"]["total"].as_i64()), (1, Some(3)));
    assert_eq!(v["meta"]["summary"]["count"], 3, "Summen über alle gefilterten, nicht nur die Seite");

    // Sortierung
    let (_, v) = get(&f, "/api/v1/activities?sort=-total_time_ms").await;
    assert_eq!(v["data"][0]["id"], b.as_str());
    let (_, v) = get(&f, "/api/v1/activities?sort=elevation_gain_m").await;
    assert_eq!(v["data"][0]["id"], c.as_str());

    // Ungültige Parameter
    for q in ["sort=title", "sport_class=walking", "from=gestern", "foo=1", "route_id=x", "q="] {
        let (s, e) = get(&f, &format!("/api/v1/activities?{q}")).await;
        assert_eq!((s, code(&e)), (StatusCode::BAD_REQUEST, "validation_failed"), "{q}");
    }
    f.db.close().await;
}

#[tokio::test]
async fn update_metadata_and_correct_times() {
    let f = fixture().await;
    let id = import(&f, "a.gpx", &gpx("Morgenrunde", "cycling", T0, 60)).await;
    let path = format!("/api/v1/activities/{id}");

    let (s, v) = with_body(
        &f,
        Method::PATCH,
        &path,
        json!({ "title": "  Kelheim-Runde ", "location": "Kelheim", "description": "Mit Wind", "sub_sport": "road" }),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let a = &v["data"];
    assert_eq!(
        (a["title"].as_str(), a["location"].as_str(), a["sub_sport"].as_str()),
        (Some("Kelheim-Runde"), Some("Kelheim"), Some("road"))
    );
    assert_eq!(a["revision"], 2);

    // null löscht nullbare Felder, leere Zeichenkette ebenso
    let (_, v) = with_body(&f, Method::PATCH, &path, json!({ "description": null, "location": "" })).await;
    assert_eq!((v["data"]["description"].clone(), v["data"]["location"].clone()), (Value::Null, Value::Null));
    assert_eq!(v["data"]["title"], "Kelheim-Runde");

    // Start korrigieren: Original bleibt, Herkunft manuell korrigiert; zweite Korrektur behält das erste Original
    let (_, v) =
        with_body(&f, Method::PATCH, &path, json!({ "start_time": { "local": "2026-07-01T11:00:00" }, "total_time_ms": 600_000 })).await;
    let a = &v["data"];
    assert_eq!(a["start_time"], "2026-07-01T09:00:00.000Z");
    assert_eq!(a["start_time_origin"], "manual_corrected");
    assert_eq!(a["start_time_original"], "2026-07-01T08:00:00.000Z");
    assert_eq!((a["total_time_ms"].as_i64(), a["total_time_original_ms"].as_i64()), (Some(600_000), Some(590_000)));
    let (_, v) = with_body(&f, Method::PATCH, &path, json!({ "start_time": { "local": "2026-07-01T12:00:00" } })).await;
    assert_eq!(v["data"]["start_time_original"], "2026-07-01T08:00:00.000Z");

    // Sportklasse ändern stellt die Aktivität neu in die Auswertung
    sqlx::query("UPDATE jobs SET status = 'done'").execute(&f.db.writer).await.unwrap();
    let (_, v) = with_body(&f, Method::PATCH, &path, json!({ "sport_class": "ebike" })).await;
    assert_eq!(v["data"]["sport_class"], "ebike");
    assert_eq!(v["data"]["processing_status"], "pending");
    assert_eq!(count(&f.db, "SELECT count(*) FROM jobs WHERE status = 'queued' AND kind = 'activity_evaluation'").await, 1);

    // Fehler
    let cases = [
        (json!({}), "validation_failed"),
        (json!({ "distance_m": 1 }), "validation_failed"),
        (json!({ "title": " " }), "validation_failed"),
        (json!({ "sport_class": "walking" }), "validation_failed"),
        (json!({ "total_time_ms": -1 }), "validation_failed"),
        (json!({ "start_time": { "local": "2026-10-25T02:30:00" } }), "local_time_ambiguous"),
    ];
    for (body, expected) in cases {
        let (s, e) = with_body(&f, Method::PATCH, &path, body.clone()).await;
        assert_eq!((s, code(&e)), (StatusCode::BAD_REQUEST, expected), "{body}");
    }
    let (s, e) = with_body(&f, Method::PATCH, "/api/v1/activities/0198c200-0000-7000-8000-000000000009", json!({ "title": "x" })).await;
    assert_eq!((s, code(&e)), (StatusCode::NOT_FOUND, "activity_not_found"));
    // Frühjahrslücke mit Hinweis
    let (_, v) = with_body(&f, Method::PATCH, &path, json!({ "start_time": { "local": "2026-03-29T02:30:00" } })).await;
    assert_eq!(v["meta"]["notices"][0]["code"], "local_time_shifted");
    f.db.close().await;
}

#[tokio::test]
async fn delete_removes_activity_and_unreferenced_original() {
    let f = fixture().await;
    let id = import(&f, "a.gpx", &gpx("Morgenrunde", "cycling", T0, 60)).await;
    let rel: String = sqlx::query_scalar("SELECT relative_path FROM original_files").fetch_one(&f.db.reader).await.unwrap();
    let file = f.dir.path().join("data/activities").join(&rel);
    assert!(file.is_file());

    let (s, v) = get(&f, &format!("/api/v1/activities/{id}/deletion-preview")).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(
        v["data"],
        json!({ "object_type": "activity", "effort_count": 0, "affected_segment_count": 0, "original_file_removed": true })
    );

    let before = count(&f.db, "SELECT change_counter FROM data_state").await;
    let (s, _) = delete(&f, &format!("/api/v1/activities/{id}")).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    assert!(!file.exists());
    assert_eq!(count(&f.db, "SELECT count(*) FROM activities").await, 0);
    assert_eq!(count(&f.db, "SELECT count(*) FROM track_sections").await, 0);
    assert_eq!(count(&f.db, "SELECT count(*) FROM activity_calculations").await, 0);
    assert_eq!(count(&f.db, "SELECT count(*) FROM audit_log WHERE action = 'delete'").await, 1);
    assert_eq!(count(&f.db, "SELECT change_counter FROM data_state").await, before + 1);

    let (s, e) = get(&f, &format!("/api/v1/activities/{id}")).await;
    assert_eq!((s, code(&e)), (StatusCode::NOT_FOUND, "activity_not_found"));
    let (s, _) = delete(&f, &format!("/api/v1/activities/{id}")).await;
    assert_eq!(s, StatusCode::NOT_FOUND);

    // Danach ist ein vollständiger Neuimport derselben Datei möglich (RFC-0017).
    import(&f, "a.gpx", &gpx("Morgenrunde", "cycling", T0, 60)).await;
    f.db.close().await;
}

#[tokio::test]
async fn track_columns_and_thinning() {
    let f = fixture().await;
    let id = import(&f, "a.gpx", &gpx("Morgenrunde", "cycling", T0, 60)).await;

    let (s, v) = get(&f, &format!("/api/v1/activities/{id}/track")).await;
    assert_eq!(s, StatusCode::OK);
    let sec = &v["data"]["sections"][0];
    assert_eq!(
        (sec["point_count"].as_i64(), sec["returned_point_count"].as_i64(), sec["simplified"].as_bool()),
        (Some(60), Some(60), Some(false))
    );
    assert_eq!(sec["t_ms"][0], T0 * 1000);
    assert_eq!(sec["lat"][0], 47.0);
    assert_eq!(sec["ele_m"][59], 559.0);
    assert_eq!(sec["dist_m"][0], 0.0);
    let d: Vec<f64> = sec["dist_m"].as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect();
    assert!(d.windows(2).all(|w| w[1] > w[0]));
    assert!((d[59] - 1968.0).abs() < 5.0, "{}", d[59]);

    let (_, v) = get(&f, &format!("/api/v1/activities/{id}/track?max_points=5")).await;
    let sec = &v["data"]["sections"][0];
    assert_eq!(sec["index"], json!([0, 15, 30, 44, 59]));
    assert_eq!(sec["simplified"], true);
    // Distanz der ausgedünnten Punkte ist die volle kumulierte Strecke
    assert!((sec["dist_m"][4].as_f64().unwrap() - d[59]).abs() < 1e-6);

    let (_, v) = get(&f, &format!("/api/v1/activities/{id}/track?section=1")).await;
    assert_eq!(v["data"]["sections"], json!([]));
    for q in ["max_points=1", "section=-1", "x=1"] {
        let (s, e) = get(&f, &format!("/api/v1/activities/{id}/track?{q}")).await;
        assert_eq!((s, code(&e)), (StatusCode::BAD_REQUEST, "validation_failed"), "{q}");
    }
    let (s, v) = get(&f, &format!("/api/v1/activities/{id}/efforts")).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["data"], json!([]));
    assert_eq!(v["meta"]["calculation"]["status"], "current");
    f.db.close().await;
}

#[tokio::test]
async fn dashboard_week_and_month() {
    let f = fixture().await;
    import(&f, "mi.gpx", &gpx("Mittwoch", "cycling", T0, 60)).await; // Mi 01.07.
    import(&f, "do.gpx", &gpx("Donnerstag", "running", T0 + 86_400, 31)).await; // Do 02.07.
    import(&f, "mo.gpx", &gpx("Vorwoche", "cycling", T0 - 3 * 86_400, 31)).await; // So 28.06.: Vormonat und Vorwoche

    let today = NaiveDate::from_ymd_opt(2026, 7, 3).unwrap(); // Freitag
    let d = activities::dashboard(&f.cfg, &f.db, today).await.unwrap();
    assert_eq!(
        (d["week"]["period"].as_str(), d["week"]["from"].as_str(), d["week"]["to"].as_str()),
        (Some("2026-W27"), Some("2026-06-29"), Some("2026-07-05"))
    );
    assert_eq!(
        (d["month"]["period"].as_str(), d["month"]["from"].as_str(), d["month"]["to"].as_str()),
        (Some("2026-07"), Some("2026-07-01"), Some("2026-07-31"))
    );
    let all = |b: &Value| b["summaries"].as_array().unwrap().iter().find(|s| s["sport_class"] == "all").unwrap().clone();
    assert_eq!(all(&d["week"])["count"], 2);
    assert_eq!(all(&d["week"])["total_time_ms"], 890_000);
    assert_eq!(all(&d["month"])["count"], 2);
    let by_day = d["week"]["by_day"].as_array().unwrap();
    assert_eq!(by_day.len(), 2);
    assert_eq!((by_day[0]["date"].as_str(), by_day[0]["sport_class"].as_str()), (Some("2026-07-01"), Some("cycling")));
    assert_eq!((by_day[1]["date"].as_str(), by_day[1]["sport_class"].as_str()), (Some("2026-07-02"), Some("running")));

    // Jahr: alle drei Aktivitäten (28.06. bis 02.07.), unabhängig von Saisons
    assert_eq!(
        (d["year"]["period"].as_str(), d["year"]["from"].as_str(), d["year"]["to"].as_str()),
        (Some("2026"), Some("2026-01-01"), Some("2026-12-31"))
    );
    assert_eq!(all(&d["year"])["count"], 3);
    assert_eq!(all(&d["year"])["total_time_ms"], 1_190_000);
    assert!(d.get("season").is_none(), "keine Saison mehr auf dem Dashboard");

    let (s, v) = get(&f, "/api/v1/stats/dashboard").await;
    assert_eq!(s, StatusCode::OK);
    assert!(v["data"]["week"]["summaries"].is_array());
    assert!(v["data"]["year"]["summaries"].is_array());
    f.db.close().await;
}

#[tokio::test]
async fn seasons_are_named_periods_without_overlap() {
    let f = fixture().await;
    let body = |n: &str, a: &str, b: &str| json!({ "name": n, "start_date": a, "end_date": b });
    let (s, v) = with_body(&f, Method::POST, "/api/v1/seasons", body(" Saison 2026 ", "2026-03-01", "2026-10-31")).await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    let id = v["data"]["id"].as_str().unwrap().to_string();
    assert_eq!(v["data"]["name"], "Saison 2026");
    assert_eq!((v["data"]["start_date"].as_str(), v["data"]["end_date"].as_str()), (Some("2026-03-01"), Some("2026-10-31")));

    // Überschneidung auch an einem einzigen Tag, Name ohne Rücksicht auf Groß- und Kleinschreibung
    let (s, v) = with_body(&f, Method::POST, "/api/v1/seasons", body("Winter", "2026-10-31", "2027-02-28")).await;
    assert_eq!((s, code(&v)), (StatusCode::CONFLICT, "season_overlap"));
    assert_eq!(v["error"]["details"]["season"]["id"], id.as_str());
    let (s, v) = with_body(&f, Method::POST, "/api/v1/seasons", body("saison 2026", "2025-03-01", "2025-10-31")).await;
    assert_eq!((s, code(&v)), (StatusCode::CONFLICT, "season_name_taken"));
    let (s, v) = with_body(&f, Method::POST, "/api/v1/seasons", body("Winter", "2027-02-28", "2026-11-01")).await;
    assert_eq!(
        (s, code(&v), v["error"]["details"]["fields"][0]["code"].as_str()),
        (StatusCode::BAD_REQUEST, "validation_failed", Some("before_start"))
    );
    let (s, v) = with_body(&f, Method::POST, "/api/v1/seasons", body("Winter", "2026-11-1", "2027-02-28")).await;
    assert_eq!((s, code(&v)), (StatusCode::BAD_REQUEST, "validation_failed"));
    let (s, v) = with_body(&f, Method::POST, "/api/v1/seasons", body(" ", "2026-11-01", "2027-02-28")).await;
    assert_eq!((s, code(&v)), (StatusCode::BAD_REQUEST, "validation_failed"));
    let (s, v) = with_body(&f, Method::POST, "/api/v1/seasons", body("Winter", "2026-11-01", "2027-02-28")).await;
    assert_eq!(s, StatusCode::CREATED, "Lücke und direkter Anschluss erlaubt: {v}");
    let winter = v["data"]["id"].as_str().unwrap().to_string();

    // Liste neueste zuerst; Vorgabe ist die laufende, sonst die zuletzt beendete Saison
    let (s, v) = get(&f, "/api/v1/seasons").await;
    assert_eq!(s, StatusCode::OK);
    let names: Vec<&str> = v["data"].as_array().unwrap().iter().map(|x| x["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["Winter", "Saison 2026"]);
    assert_eq!(v["meta"]["total"], 2);
    assert!(v["meta"]["default_season_id"].is_string() || v["meta"]["default_season_id"].is_null());

    // Ändern prüft gegen die anderen Saisons, nicht gegen sich selbst
    let (s, v) = with_body(&f, Method::PATCH, &format!("/api/v1/seasons/{id}"), json!({ "end_date": "2026-11-01" })).await;
    assert_eq!((s, code(&v)), (StatusCode::CONFLICT, "season_overlap"));
    let (s, v) =
        with_body(&f, Method::PATCH, &format!("/api/v1/seasons/{id}"), json!({ "start_date": "2026-02-01", "name": "Saison 2026" })).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["data"]["start_date"], "2026-02-01");
    let (s, v) = with_body(&f, Method::PATCH, &format!("/api/v1/seasons/{id}"), json!({})).await;
    assert_eq!((s, code(&v)), (StatusCode::BAD_REQUEST, "validation_failed"));
    let (s, v) = with_body(&f, Method::PATCH, &format!("/api/v1/seasons/{id}"), json!({ "color": "rot" })).await;
    assert_eq!((s, code(&v)), (StatusCode::BAD_REQUEST, "validation_failed"));

    let (s, _) = delete(&f, &format!("/api/v1/seasons/{winter}")).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, v) = get(&f, &format!("/api/v1/seasons/{winter}")).await;
    assert_eq!((s, code(&v)), (StatusCode::NOT_FOUND, "season_not_found"));
    let (s, v) = delete(&f, &format!("/api/v1/seasons/{winter}")).await;
    assert_eq!((s, code(&v)), (StatusCode::NOT_FOUND, "season_not_found"));
    let (s, v) = with_body(&f, Method::PATCH, &format!("/api/v1/seasons/{winter}"), json!({ "name": "x" })).await;
    assert_eq!((s, code(&v)), (StatusCode::NOT_FOUND, "season_not_found"));
    f.db.close().await;
}

#[tokio::test]
async fn calculation_follows_hysteresis_setting() {
    let f = fixture().await;
    let id = import(&f, "a.gpx", &gpx("Morgenrunde", "cycling", T0, 60)).await;
    let first: String =
        sqlx::query_scalar("SELECT id FROM calculation_versions WHERE status = 'published'").fetch_one(&f.db.reader).await.unwrap();
    // Position unbekannt, als käme die Aktivität aus Schema 2
    sqlx::query("UPDATE activities SET has_position = NULL").execute(&f.db.writer).await.unwrap();
    f.db.close().await;
    let cfg = f.cfg.clone();

    // Gleiche Parameter: Stand bleibt, nur die Position wird nachgetragen
    let started = startup::prepare(&cfg).await.unwrap();
    let again: String =
        sqlx::query_scalar("SELECT id FROM calculation_versions WHERE status = 'published'").fetch_one(&started.db.reader).await.unwrap();
    assert_eq!(again, first);
    assert_eq!(count(&started.db, "SELECT has_position FROM activities").await, 1);
    started.db.close().await;
    drop(started);

    // Andere Hysterese: neuer Stand, alter gelöscht (C3), Wert neu berechnet (Schwelle 5 m: 11 × 5 = 55 m)
    let g = fixture_in(f.dir, "[processing.elevation]\nhysteresis_m = 5.0\n").await;
    assert_ne!(calculation::parameters_hash(&g.cfg), calculation::parameters_hash(&cfg));
    let rows: Vec<(String, String)> =
        sqlx::query_as("SELECT id, status FROM calculation_versions ORDER BY created_at_ms").fetch_all(&g.db.reader).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_ne!(rows[0].0, first);
    assert_eq!(rows[0].1, "published");
    let (_, v) = get(&g, &format!("/api/v1/activities/{id}")).await;
    assert_eq!(v["data"]["elevation_gain_m"], 55.0);
    assert_eq!(count(&g.db, "SELECT count(*) FROM activity_calculations").await, 1);
    g.db.close().await;
}

#[tokio::test]
async fn statistics_for_period_class_and_seasons() {
    let f = fixture().await;
    let a = import(&f, "mi.gpx", &gpx("Mittwoch", "cycling", T0, 60)).await; // Mi 01.07., 59 × 10 s
    import(&f, "do.gpx", &gpx("Donnerstag", "running", T0 + 86_400, 31)).await; // Do 02.07.
    import(&f, "so.gpx", &gpx("Vorwoche", "cycling", T0 - 3 * 86_400, 31)).await; // So 28.06.

    let (s, v) = get(&f, "/api/v1/stats?from=2026-07-01&to=2026-07-31").await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let d = &v["data"];
    let sum = |class: &str| d["summaries"].as_array().unwrap().iter().find(|x| x["sport_class"] == class).unwrap().clone();
    assert_eq!(sum("all")["count"], 2);
    assert_eq!(sum("cycling")["count"], 1);
    assert_eq!(sum("ebike")["count"], 0);
    assert_eq!(sum("all")["total_time_ms"], 890_000);
    assert!(d["summaries"].as_array().unwrap().iter().all(|x| x["sport_class"] != "unknown"));
    assert_eq!((d["first_day"].as_str(), d["last_day"].as_str()), (Some("2026-07-01"), Some("2026-07-02")));
    let months: Vec<(&str, &str)> =
        d["by_month"].as_array().unwrap().iter().map(|x| (x["month"].as_str().unwrap(), x["sport_class"].as_str().unwrap())).collect();
    assert_eq!(months, [("2026-07", "cycling"), ("2026-07", "running")]);
    assert_eq!(d["by_week"][0]["week"], "2026-W27");
    assert_eq!(d["by_week"][0]["week_start"], "2026-06-29");
    let rec = d["records"].as_array().unwrap();
    let cyc = rec.iter().find(|x| x["sport_class"] == "cycling" && x["kind"] == "distance").unwrap();
    assert_eq!(cyc["activity"]["id"], a.as_str());

    // Ohne Zeitraum alles, Sportklasse filtert auch die Rekorde
    let (_, v) = get(&f, "/api/v1/stats?sport_class=cycling").await;
    let all = v["data"]["summaries"].as_array().unwrap().iter().find(|x| x["sport_class"] == "all").unwrap().clone();
    assert_eq!(all["count"], 2);
    assert!(v["data"]["records"].as_array().unwrap().iter().all(|x| x["sport_class"] == "cycling"));

    let (s, v) = get(&f, "/api/v1/stats?from=2026-07-02&to=2026-07-01").await;
    assert_eq!((s, code(&v)), (StatusCode::BAD_REQUEST, "validation_failed"));
    let (s, v) = get(&f, "/api/v1/stats?jahr=2026").await;
    assert_eq!((s, code(&v)), (StatusCode::BAD_REQUEST, "validation_failed"));
    let (s, v) = get(&f, "/api/v1/stats?sport_class=boot").await;
    assert_eq!((s, code(&v)), (StatusCode::BAD_REQUEST, "validation_failed"));

    // Kacheln: Summen je Saison und je Jahr
    let (s, v) =
        with_body(&f, Method::POST, "/api/v1/seasons", json!({ "name": "Juli", "start_date": "2026-07-01", "end_date": "2026-07-31" }))
            .await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    let season = v["data"]["id"].as_str().unwrap().to_string();
    let (s, v) = get(&f, "/api/v1/stats/periods?sport_class=cycling").await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["data"]["seasons"][0]["season_id"], season.as_str());
    assert_eq!(v["data"]["seasons"][0]["count"], 1);
    let y2026 = v["data"]["years"].as_array().unwrap().iter().find(|x| x["year"] == 2026).unwrap().clone();
    assert_eq!(y2026["count"], 2);
    let (s, v) = get(&f, "/api/v1/stats/periods?from=2026-01-01").await;
    assert_eq!((s, code(&v)), (StatusCode::BAD_REQUEST, "validation_failed"));

    // Untertyp filtert; unknown zählt nur in „all“ und hat keine Rekorde
    let (s, _) = with_body(&f, Method::PATCH, &format!("/api/v1/activities/{a}"), json!({ "sub_sport": "mtb" })).await;
    assert_eq!(s, StatusCode::OK);
    let (_, v) = get(&f, "/api/v1/stats?sport_class=cycling&sub_sport=mtb").await;
    assert_eq!(v["data"]["summaries"].as_array().unwrap().iter().find(|x| x["sport_class"] == "all").unwrap()["count"], 1);
    assert_eq!(v["data"]["records"][0]["activity"]["sub_sport"], "mtb");
    import(&f, "u.gpx", &gpx("Unbekannt", "foo", T0 + 2 * 86_400, 31)).await; // Fr 03.07.
    let (_, v) = get(&f, "/api/v1/stats?from=2026-07-01&to=2026-07-31").await;
    let d = &v["data"];
    assert_eq!(d["summaries"].as_array().unwrap().iter().find(|x| x["sport_class"] == "all").unwrap()["count"], 3);
    assert!(d["by_month"].as_array().unwrap().iter().any(|x| x["sport_class"] == "unknown"), "Balken stapeln auch unknown");
    assert!(d["records"].as_array().unwrap().iter().all(|x| x["sport_class"] != "unknown"));
    f.db.close().await;
}

// ---------------------------------------------------------------------------------------------------------
// Sportart-Zuordnung in der Datenbank

fn mapping_of<'a>(list: &'a Value, source: &str) -> &'a Value {
    list["data"].as_array().unwrap().iter().find(|r| r["source_sport"] == source).unwrap_or(&Value::Null)
}

fn sub_of<'a>(list: &'a Value, class: &str, pred: impl Fn(&Value) -> bool) -> &'a Value {
    list["data"].as_array().unwrap().iter().find(|r| r["sport_class"] == class && pred(r)).unwrap_or(&Value::Null)
}

/// GPX ohne Namen, damit der Ersatztitel greift.
fn gpx_untitled(kind: &str, start: i64, n: usize) -> Vec<u8> {
    let full = String::from_utf8(gpx("X", kind, start, n)).unwrap();
    full.replace("<name>X</name>", "").into_bytes()
}

#[tokio::test]
async fn skating_is_own_sub_sport_and_old_nordic_defaults_switch_once() {
    let f = fixture().await;
    let (_, v) = get(&f, "/api/v1/sport-mappings").await;
    assert_eq!(mapping_of(&v, "skate_skiing_ws")["sub_sport"], "skating");
    assert_eq!(mapping_of(&v, "cross_country_skiing")["sub_sport"], "nordic");

    // Stand vor Schema 9 nachstellen: beide Skating-Quellen auf nordic, eine davon hat der Benutzer geändert
    for sql in [
        "UPDATE sport_mappings SET sub_sport = 'nordic' WHERE source_sport IN ('skate_skiing_ws', 'cross_country_skiing-skate_skiing')",
        "UPDATE sport_mappings SET sport_class = 'unknown', sub_sport = NULL WHERE source_sport = 'cross_country_skiing-skate_skiing'",
        "DELETE FROM sport_defaults_applied WHERE key LIKE 'mapping-skating:%'",
    ] {
        sqlx::query(sql).execute(&f.db.writer).await.unwrap();
    }
    f.db.close().await;
    let Fixture { dir, .. } = f;
    let f = fixture_in(dir, "").await;
    let (_, v) = get(&f, "/api/v1/sport-mappings").await;
    assert_eq!(mapping_of(&v, "skate_skiing_ws")["sub_sport"], "skating");
    assert_eq!(mapping_of(&v, "cross_country_skiing-skate_skiing")["sport_class"], "unknown");

    // Einmalig: zurück auf nordic gestellt bleibt nach dem nächsten Start nordic
    sqlx::query("UPDATE sport_mappings SET sub_sport = 'nordic' WHERE source_sport = 'skate_skiing_ws'")
        .execute(&f.db.writer)
        .await
        .unwrap();
    f.db.close().await;
    let Fixture { dir, .. } = f;
    let f = fixture_in(dir, "").await;
    let (_, v) = get(&f, "/api/v1/sport-mappings").await;
    assert_eq!(mapping_of(&v, "skate_skiing_ws")["sub_sport"], "nordic");
    f.db.close().await;
}

#[tokio::test]
async fn sport_mapping_defaults_and_toml_takeover_once() {
    let legacy = "[import.sport_class_mapping]\nrowing = \"studio\"\nwalking = \"cycling\"\nbogus = \"golf\"\n\
                  [import.sub_sport_catalog]\nstudio = [\"Rudern\", \"Krafttraining\"]\ngolf = [\"Loch\"]\n\
                  [import.sub_sport_mapping]\nrowing = \"Rudern\"\nwalking = \"Fehlt\"\n";
    let f = fixture_in(common::temp(), legacy).await;
    let (_, subs) = get(&f, "/api/v1/sub-sports").await;
    // Bekannte deutsche Bezeichnung → eingebauter Schlüssel, sonst eigener Eintrag mit Namen
    let rudern = sub_of(&subs, "studio", |r| r["name"] == "Rudern");
    let rudern_key = rudern["key"].as_str().unwrap().to_string();
    assert!(rudern_key.starts_with("custom_") && rudern["builtin"] == false, "{rudern}");
    assert!(sub_of(&subs, "studio", |r| r["key"] == "strength")["name"].is_null());

    let (s, v) = get(&f, "/api/v1/sport-mappings").await;
    assert_eq!(s, StatusCode::OK, "{v}");
    // TOML hat Vorrang vor den Vorgaben, ungültige Einträge fehlen
    assert_eq!(mapping_of(&v, "rowing")["sport_class"], "studio");
    assert_eq!(mapping_of(&v, "rowing")["sub_sport"], rudern_key.as_str());
    assert_eq!(mapping_of(&v, "walking")["sport_class"], "cycling");
    assert_eq!(mapping_of(&v, "walking")["sub_sport"], Value::Null);
    assert!(mapping_of(&v, "bogus").is_null());
    // Vorgaben ergänzen, was die TOML nicht hatte
    assert_eq!(mapping_of(&v, "fitness_equipment")["sub_sport"], "strength");
    assert_eq!(mapping_of(&v, "cycling-road")["effective_sub_sport"], "road");

    // Import nutzt die Zuordnung aus der Datenbank
    let id = import(&f, "r.gpx", &gpx("Rudern", "rowing", T0, 30)).await;
    let (_, a) = get(&f, &format!("/api/v1/activities/{id}")).await;
    assert_eq!((a["data"]["sport_class"].as_str(), a["data"]["sub_sport"].as_str()), (Some("studio"), Some(rudern_key.as_str())));

    // Benutzer löscht eine Vorgabe und ändert eine TOML-Zuordnung; ein Neustart bringt beides nicht zurück.
    assert_eq!(delete(&f, "/api/v1/sport-mappings/fitness_equipment").await.0, StatusCode::NO_CONTENT);
    let (s, v) = with_body(&f, Method::PUT, "/api/v1/sport-mappings/rowing", json!({ "sport_class": "unknown" })).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    f.db.close().await;
    let Fixture { dir, .. } = f;
    let f = fixture_in(dir, legacy).await;
    let (_, v) = get(&f, "/api/v1/sport-mappings").await;
    assert!(mapping_of(&v, "fitness_equipment").is_null());
    assert_eq!(mapping_of(&v, "rowing")["sport_class"], "unknown");
    assert_eq!(mapping_of(&v, "rowing")["activity_count"], 1);
    // Die vorhandene Aktivität behält ihre Zuordnung
    let (_, a) = get(&f, &format!("/api/v1/activities/{id}")).await;
    assert_eq!(a["data"]["sport_class"], "studio");
    f.db.close().await;
}

#[tokio::test]
async fn sport_mapping_validation_and_fallback() {
    let f = fixture().await;
    let url = "/api/v1/sport-mappings/cycling-gravel";
    let put = |body: Value| with_body(&f, Method::PUT, url, body);
    let (s, v) = put(json!({ "sport_class": "cycling", "sub_sport": "trail" })).await;
    assert_eq!((s, code(&v)), (StatusCode::BAD_REQUEST, "validation_failed"), "{v}");
    let (s, v) = put(json!({ "sport_class": "golf" })).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{v}");
    let (s, v) = put(json!({ "sport_class": "cycling", "sub_sport": "mtb" })).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["data"]["effective_sub_sport"], "mtb");
    assert_eq!(delete(&f, url).await.0, StatusCode::NO_CONTENT);
    let (s, v) = delete(&f, url).await;
    assert_eq!((s, code(&v)), (StatusCode::NOT_FOUND, "sport_mapping_not_found"));
    // Ein unbekannter Typ aus Importen erscheint in der Liste, mit Anzahl. GPX-Typen normalisiert der Parser.
    let id = import(&f, "g.gpx", &gpx("Kies", "Gravel Ride", T0, 30)).await;
    let (_, v) = get(&f, "/api/v1/sport-mappings").await;
    let row = mapping_of(&v, "gravel_ride");
    assert_eq!((row["mapped"].as_bool(), row["effective_sport_class"].as_str()), (Some(false), Some("unknown")), "{row}");
    assert_eq!(row["activity_count"], 1);
    assert!(mapping_of(&v, "cycling-gravel").is_null());

    // Untertyp der Aktivität: nur Schlüssel aus dem Katalog der Klasse; ein Klassenwechsel entfernt einen unpassenden.
    let path = format!("/api/v1/activities/{id}");
    let (s, v) = with_body(&f, Method::PATCH, &path, json!({ "sub_sport": "road" })).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "unknown hat keinen Katalog: {v}");
    let (s, v) = with_body(&f, Method::PATCH, &path, json!({ "sport_class": "cycling", "sub_sport": "road" })).await;
    assert_eq!((s, v["data"]["sub_sport"].as_str()), (StatusCode::OK, Some("road")), "{v}");
    let (s, v) = with_body(&f, Method::PATCH, &path, json!({ "sport_class": "running" })).await;
    assert_eq!((s, v["data"]["sub_sport"].clone()), (StatusCode::OK, Value::Null), "{v}");
    let (s, _) = with_body(&f, Method::PATCH, &path, json!({ "sport_class": "cycling", "sub_sport": "indoor" })).await;
    assert_eq!(s, StatusCode::OK);
    let (s, v) = with_body(&f, Method::PATCH, &path, json!({ "sport_class": "swimming" })).await;
    assert_eq!((s, v["data"]["sub_sport"].as_str()), (StatusCode::OK, Some("indoor")), "gleicher Schlüssel in beiden Klassen: {v}");
    f.db.close().await;
}

#[tokio::test]
async fn sub_sport_rename_and_delete_is_blocked_while_used() {
    let f = fixture().await;
    let (s, v) = with_body(&f, Method::POST, "/api/v1/sub-sports", json!({ "sport_class": "cycling", "name": " Gravel " })).await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    assert_eq!((v["data"]["name"].as_str(), v["data"]["builtin"].as_bool()), (Some("Gravel"), Some(false)));
    let gravel = v["data"]["key"].as_str().unwrap().to_string();
    let (s, v) = with_body(&f, Method::POST, "/api/v1/sub-sports", json!({ "sport_class": "cycling", "name": "gravel" })).await;
    assert_eq!((s, code(&v)), (StatusCode::CONFLICT, "sub_sport_name_taken"));
    // Kollision mit der Bezeichnung eines eingebauten Eintrags
    let (s, v) = with_body(&f, Method::POST, "/api/v1/sub-sports", json!({ "sport_class": "cycling", "name": "rennrad" })).await;
    assert_eq!((s, code(&v)), (StatusCode::CONFLICT, "sub_sport_name_taken"));
    let (s, _) = with_body(&f, Method::POST, "/api/v1/sub-sports", json!({ "sport_class": "unknown", "name": "X" })).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    // Eine Aktivität mit Untertyp mtb und die Vorgaben → mtb verwenden ihn.
    let id = import(&f, "m.gpx", &gpx("Wald", "mountain_biking", T0, 30)).await;
    let (_, a) = get(&f, &format!("/api/v1/activities/{id}")).await;
    assert_eq!(a["data"]["sub_sport"], "mtb");
    let (s, v) = delete(&f, "/api/v1/sub-sports/cycling/mtb").await;
    assert_eq!((s, code(&v)), (StatusCode::CONFLICT, "sub_sport_in_use"), "{v}");
    assert_eq!(v["error"]["details"]["activity_count"], 1);
    assert_eq!(v["error"]["details"]["mapping_count"], 3); // cycling-mountain, mountain_biking, mountain

    // Umbenennen ändert nur den Namen; Aktivität und Zuordnungen behalten den Schlüssel.
    let (s, v) = with_body(&f, Method::PATCH, "/api/v1/sub-sports/cycling/mtb", json!({ "name": "gravel" })).await;
    assert_eq!((s, code(&v)), (StatusCode::CONFLICT, "sub_sport_name_taken"));
    let (s, v) = with_body(&f, Method::PATCH, "/api/v1/sub-sports/cycling/mtb", json!({ "name": "Mountainbike" })).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!((v["data"]["name"].as_str(), v["data"]["activity_count"].as_i64()), (Some("Mountainbike"), Some(1)));
    let (_, c) = get(&f, "/api/v1/config").await;
    assert_eq!(
        c["data"]["sub_sport_catalog"]["cycling"][1],
        json!({ "key": "mtb", "name": "Mountainbike", "icon": null, "builtin": true })
    );
    assert_eq!(c["data"]["sub_sport_catalog"]["cycling"][3]["key"], gravel.as_str());
    // Ein Ersatztitel trägt den eigenen Namen
    let id2 = import(&f, "m2.gpx", &gpx_untitled("mountain_biking", T0 + 86_400, 30)).await;
    let (_, a) = get(&f, &format!("/api/v1/activities/{id2}")).await;
    assert!(a["data"]["title"].as_str().unwrap().starts_with("Mountainbike "), "{a}");
    // null setzt einen eingebauten zurück, ein eigener braucht einen Namen
    let (s, v) = with_body(&f, Method::PATCH, "/api/v1/sub-sports/cycling/mtb", json!({ "name": null })).await;
    assert_eq!((s, v["data"]["name"].clone()), (StatusCode::OK, Value::Null), "{v}");
    let (s, _) = with_body(&f, Method::PATCH, &format!("/api/v1/sub-sports/cycling/{gravel}"), json!({ "name": null })).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = with_body(&f, Method::PATCH, "/api/v1/sub-sports/cycling/mtb", json!({})).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    // Symbol: wählbar aus dem festen Satz, null = Vorgabe, Name bleibt unberührt
    let (s, v) = with_body(&f, Method::PATCH, &format!("/api/v1/sub-sports/cycling/{gravel}"), json!({ "icon": "mtb" })).await;
    assert_eq!((s, v["data"]["icon"].as_str(), v["data"]["name"].as_str()), (StatusCode::OK, Some("mtb"), Some("Gravel")), "{v}");
    let (_, cfg) = get(&f, "/api/v1/config").await;
    let entry =
        cfg["data"]["sub_sport_catalog"]["cycling"].as_array().unwrap().iter().find(|e| e["key"] == gravel.as_str()).unwrap().clone();
    assert_eq!(entry["icon"], "mtb");
    let (s, v) = with_body(&f, Method::PATCH, &format!("/api/v1/sub-sports/cycling/{gravel}"), json!({ "icon": "rakete" })).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{v}");
    let (s, v) = with_body(&f, Method::PATCH, &format!("/api/v1/sub-sports/cycling/{gravel}"), json!({ "icon": null })).await;
    assert_eq!((s, v["data"]["icon"].clone()), (StatusCode::OK, Value::Null), "{v}");
    let (s, v) =
        with_body(&f, Method::POST, "/api/v1/sub-sports", json!({ "sport_class": "running", "name": "Bahn", "icon": "star" })).await;
    assert_eq!((s, v["data"]["icon"].as_str()), (StatusCode::CREATED, Some("star")), "{v}");

    // Unbenutzt lässt er sich löschen
    assert_eq!(delete(&f, &format!("/api/v1/sub-sports/cycling/{gravel}")).await.0, StatusCode::NO_CONTENT);
    let (s, v) = delete(&f, &format!("/api/v1/sub-sports/cycling/{gravel}")).await;
    assert_eq!((s, code(&v)), (StatusCode::NOT_FOUND, "sub_sport_not_found"));
    f.db.close().await;
}

/// Migration 7 wandelt vorhandene Untertyp-Texte um: eingebaute deutsche Bezeichnung → Schlüssel, sonst eigener Eintrag.
#[tokio::test]
async fn migration_7_converts_sub_sport_texts() {
    let dir = common::temp();
    let cfg = common::validated(dir.path());
    std::fs::create_dir_all(&cfg.raw.paths.database_dir).unwrap();
    let db = Db::open(&cfg.database_file()).await.unwrap();
    let pending = db.pending_migrations().await.unwrap();
    let upto6: Vec<_> = pending.iter().copied().filter(|(v, _)| *v <= 6).collect();
    db.migrate(&upto6).await.unwrap();
    sqlx::raw_sql(
        "INSERT INTO users (id, display_name, created_at_ms, updated_at_ms) VALUES ('u', 'T', 0, 0);
         INSERT INTO original_files (id, format, relative_path, sha256, size_bytes, created_at_ms)
           VALUES ('f', 'gpx', 'a.gpx', '0000000000000000000000000000000000000000000000000000000000000000', 1, 0);",
    )
    .execute(&db.writer)
    .await
    .unwrap();
    let rows = [
        ("cycling", Some("Rennrad")),
        ("running", Some("Gehen")),
        ("cycling", Some("Gravel")),
        ("unknown", Some("Yoga")),
        ("running", None),
    ];
    for (i, (class, sub)) in rows.iter().enumerate() {
        sqlx::query(
            "INSERT INTO activities (id, user_id, original_file_id, session_index, title, sport_class, sub_sport, start_time_ms, \
             start_time_origin, total_time_ms, total_time_origin, created_at_ms, updated_at_ms) \
             VALUES (?, 'u', 'f', ?, 't', ?, ?, 0, 'file', 0, 'file', 0, 0)",
        )
        .bind(format!("a{i}"))
        .bind(i as i64)
        .bind(class)
        .bind(sub)
        .execute(&db.writer)
        .await
        .unwrap();
    }
    let rest = db.pending_migrations().await.unwrap();
    db.migrate(&rest).await.unwrap();
    let subs: Vec<(String, String)> =
        sqlx::query_as("SELECT id, coalesce(sub_sport, '-') FROM activities ORDER BY id").fetch_all(&db.reader).await.unwrap();
    assert_eq!(subs[0], ("a0".into(), "road".into()));
    assert_eq!(subs[1], ("a1".into(), "walking".into()));
    assert!(subs[2].1.starts_with("custom_") && subs[3].1.starts_with("custom_"), "{subs:?}");
    assert_eq!(subs[4].1, "-");
    let gravel: Option<String> =
        sqlx::query_scalar("SELECT name FROM sub_sports WHERE key = ?").bind(&subs[2].1).fetch_one(&db.reader).await.unwrap();
    assert_eq!(gravel.as_deref(), Some("Gravel"));
    let fk: i64 = sqlx::query_scalar("SELECT count(*) FROM pragma_foreign_key_check").fetch_one(&db.reader).await.unwrap();
    assert_eq!(fk, 0);
    db.close().await;
}
