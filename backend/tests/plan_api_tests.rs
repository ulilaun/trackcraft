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
    let goal =
        json!({ "date": day(40), "sport_class": "cycling", "name": " Tour ", "url": "https://example.org/tour", "distance_m": 120000 });
    let (s, v) = send(&f, Method::POST, "/api/v1/plan/goals", Some(goal)).await;
    assert_eq!((s, v["data"]["name"].as_str()), (StatusCode::CREATED, Some("Tour")), "{v}");
    let goal_id = v["data"]["id"].as_str().unwrap().to_string();
    assert_eq!((v["data"]["distance_m"].as_f64(), v["data"]["target_time_s"].clone()), (Some(120000.0), Value::Null));
    let skipped = new_unit(&f, unit_body(&day(3), "Fällt aus")).await;
    let (s, _) = send(&f, Method::PATCH, &format!("/api/v1/plan/units/{skipped}"), Some(json!({ "status": "skipped" }))).await;
    assert_eq!(s, StatusCode::OK);
    let v = get(&f, "/api/v1/form?forecast=plan&forecast_days=28").await;
    let fc = v["forecast"]["days"].as_array().unwrap();
    assert_eq!(fc.len(), 43, "Ziel in 40 Tagen + 3");
    assert_eq!(v["forecast"]["mode"], "plan");
    assert_eq!(v["plan"]["goals"][0]["date"].as_str(), Some(day(40).as_str()));
    assert_eq!(v["plan"]["last_date"].as_str(), Some(day(2).as_str()));
    let load_on = |d: &str| fc.iter().find(|x| x["date"] == d).unwrap()["load"].as_f64().unwrap();
    assert_eq!((load_on(&day(1)), load_on(&day(2)), load_on(&day(3))), (0.0, 80.0, 0.0));
    assert!(v["current"]["compare"]["week_ahead"]["fitness"].as_f64().unwrap() > 0.0);
    // Ohne Vorschau keine Verlängerung
    assert_eq!(get(&f, "/api/v1/form?forecast=plan&forecast_days=0").await["forecast"]["days"], json!([]));

    // Liste im Zeitraum, Ziel entfernen, Einheit löschen
    let list = get(&f, &format!("/api/v1/plan?from={}&to={}", day(0), day(10))).await;
    assert_eq!(list["units"].as_array().unwrap().len(), 2);
    assert_eq!(list["goals"][0]["name"], "Tour");
    let (s, _) = send(&f, Method::DELETE, &format!("/api/v1/plan/goals/{goal_id}"), None).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    assert_eq!(get(&f, "/api/v1/plan").await["goals"], json!([]));
    let (s, v) = send(&f, Method::DELETE, &format!("/api/v1/plan/goals/{goal_id}"), None).await;
    assert_eq!((s, v["error"]["code"].as_str()), (StatusCode::NOT_FOUND, Some("plan_goal_not_found")));
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

#[tokio::test]
async fn suggestion_preview_and_apply() {
    let f = fixture().await;
    let today: chrono::NaiveDate = get(&f, "/api/v1/plan").await["today"].as_str().unwrap().parse().unwrap();
    let day = |n: i64| today + chrono::Duration::days(n);
    let input = json!({
        "sport_class": "cycling",
        "minutes": [0, 60, 0, 75, 0, 180, 120],
        "max_gain": 5,
        "recovery_weeks": true,
        "intensity": "polarized",
    });

    // Ohne Ziel kein Vorschlag; Prüfungen der Eingaben
    let (s, v) = send(&f, Method::POST, "/api/v1/plan/suggestion", Some(input.clone())).await;
    assert_eq!((s, v["error"]["details"]["fields"][0]["field"].as_str()), (StatusCode::BAD_REQUEST, Some("goal_id")), "{v}");
    let mut input = input;
    input["goal_id"] = json!("gibtsnicht");
    let (s, v) = send(&f, Method::POST, "/api/v1/plan/suggestion", Some(input.clone())).await;
    assert_eq!((s, v["error"]["details"]["fields"][0]["code"].as_str()), (StatusCode::BAD_REQUEST, Some("not_found")), "{v}");
    // Zwei Ziele; geplant wird auf das gewählte
    let goal = |d: i64, n: &str| json!({ "date": day(d).to_string(), "sport_class": "cycling", "name": n });
    let (s, v) = send(&f, Method::POST, "/api/v1/plan/goals", Some(goal(45, "Tour"))).await;
    assert_eq!(s, StatusCode::CREATED);
    input["goal_id"] = v["data"]["id"].clone();
    let (s, _) = send(&f, Method::POST, "/api/v1/plan/goals", Some(goal(20, "Vorbereitung"))).await;
    assert_eq!(s, StatusCode::CREATED);
    let mut bad = input.clone();
    bad["max_gain"] = json!(9);
    let (s, v) = send(&f, Method::POST, "/api/v1/plan/suggestion", Some(bad)).await;
    assert_eq!((s, v["error"]["details"]["fields"][0]["field"].as_str()), (StatusCode::BAD_REQUEST, Some("max_gain")), "{v}");
    let zones = json!({ "valid_from": "2020-01-01", "max_hr": 180, "resting_hr": 50, "model": "reserve" });
    let (s, _) = send(&f, Method::POST, "/api/v1/hr-zones", Some(zones)).await;
    assert_eq!(s, StatusCode::CREATED);

    // Feste Einheit am ersten Samstag bleibt und wird eingerechnet
    let sat = (1..=7).map(day).find(|d| chrono::Datelike::weekday(d) == chrono::Weekday::Sat).unwrap();
    new_unit(&f, json!({ "date": sat.to_string(), "sport_class": "cycling", "title": "Ausfahrt", "mode": "zones", "zone_minutes": [10, 100, 0, 0, 0] })).await;

    // Vorschau speichert nichts
    let (s, v) = send(&f, Method::POST, "/api/v1/plan/suggestion", Some(input.clone())).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let p = &v["data"];
    let units = p["units"].as_array().unwrap();
    assert!(!units.is_empty());
    assert!(units.iter().all(|u| u["date"] != sat.to_string() && u["mode"] == "zones" && u["phase"].is_string()));
    assert_eq!(p["fixed_units"], 1);
    assert_eq!(p["goal_date"].as_str(), Some(day(45).to_string().as_str()));
    let taper = p["summary"]["taper_days"].as_u64().unwrap();
    assert!((7..=14).contains(&taper), "{p}");
    assert_eq!(p["forecast"].as_array().unwrap().len(), 48, "morgen bis Ziel + 3");
    assert!(!p["weeks"].as_array().unwrap().is_empty());
    let listed = |v: &Value| v["units"].as_array().unwrap().len();
    let range = format!("/api/v1/plan?from={}&to={}", day(1), day(60));
    assert_eq!(listed(&get(&f, &range).await), 1);

    // Übernehmen legt normale Einheiten an und merkt sich die Eingaben (ohne Ziel)
    let (s, v) = send(&f, Method::POST, "/api/v1/plan/suggestion/apply", Some(input.clone())).await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    let created = v["data"]["created"].as_u64().unwrap() as usize;
    assert_eq!(created, units.len());
    let list = get(&f, &range).await;
    assert_eq!(listed(&list), created + 1);
    let mut remembered = input.clone();
    remembered.as_object_mut().unwrap().remove("goal_id");
    assert_eq!(list["assistant"], remembered);
    assert!(list["units"].as_array().unwrap().iter().all(|u| u.get("suggested").is_none()));
    // Die Vorschau „Plan“ der Formseite reicht bis zum letzten Ziel
    let form = get(&f, "/api/v1/form?forecast=plan&forecast_days=28").await;
    assert_eq!(form["plan"]["goals"].as_array().unwrap().len(), 2);
    assert_eq!(form["forecast"]["days"].as_array().unwrap().len(), 48);

    // Ein neuer Vorschlag ersetzt nichts: Alle Einheiten gelten als fest, frei sind nur noch Tage ohne Einheit
    let (s, v) = send(&f, Method::POST, "/api/v1/plan/suggestion", Some(input.clone())).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["data"]["fixed_units"].as_u64(), Some(created as u64 + 1));
    let taken: std::collections::HashSet<String> =
        list["units"].as_array().unwrap().iter().map(|u| u["date"].as_str().unwrap().to_string()).collect();
    assert!(v["data"]["units"].as_array().unwrap().iter().all(|u| !taken.contains(u["date"].as_str().unwrap())));
}

/// Einheiten im Zeitraum, nach Datum.
async fn units_between(f: &Fixture, from: chrono::NaiveDate, to: chrono::NaiveDate) -> Vec<Value> {
    get(f, &format!("/api/v1/plan?from={from}&to={to}")).await["units"].as_array().unwrap().clone()
}

fn field_of(v: &Value) -> Option<&str> {
    v["error"]["details"]["fields"][0]["field"].as_str()
}

#[tokio::test]
async fn repeat_series_and_change_following() {
    let f = fixture().await;
    let today: chrono::NaiveDate = get(&f, "/api/v1/plan").await["today"].as_str().unwrap().parse().unwrap();
    let day = |n: i64| today + chrono::Duration::days(n);

    // Wiederholen: jede Woche bis +28 Tage = 5 Einheiten
    let mut body = unit_body(&day(1).to_string(), "Grundlage");
    body["repeat"] = json!({ "every_weeks": 1, "until": day(29).to_string() });
    let id = new_unit(&f, body).await;
    let all = units_between(&f, day(0), day(60)).await;
    assert_eq!(all.len(), 5);
    let series = all[0]["series_id"].as_str().unwrap().to_string();
    assert!(all.iter().all(|u| u["series_id"] == series.as_str()));
    assert_eq!(all.iter().map(|u| u["series_following"].as_u64().unwrap()).collect::<Vec<_>>(), [4, 3, 2, 1, 0]);

    // Grenzen: keine Wiederholung, mehr als 26, Abstand
    for (rep, field) in [
        (json!({ "every_weeks": 1, "until": day(3).to_string() }), "repeat.until"),
        (json!({ "every_weeks": 1, "until": day(7 * 26 + 1).to_string() }), "repeat.until"),
        (json!({ "every_weeks": 5, "until": day(60).to_string() }), "repeat.every_weeks"),
    ] {
        let mut b = unit_body(&day(1).to_string(), "X");
        b["repeat"] = rep;
        let (s, v) = send(&f, Method::POST, "/api/v1/plan/units", Some(b)).await;
        assert_eq!((s, field_of(&v)), (StatusCode::BAD_REQUEST, Some(field)), "{v}");
    }
    // Alle 2 Wochen, 26 Einheiten sind erlaubt; danach die ganze Serie löschen
    let mut b = unit_body(&day(2).to_string(), "Zweiwöchentlich");
    b["repeat"] = json!({ "every_weeks": 2, "until": day(2 + 14 * 25).to_string() });
    let long = new_unit(&f, b).await;
    assert_eq!(units_between(&f, day(0), day(400)).await.len(), 5 + 26);
    let (s, _) = send(&f, Method::DELETE, &format!("/api/v1/plan/units/{long}?scope=following"), None).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    assert_eq!(units_between(&f, day(0), day(400)).await.len(), 5, "ganze Serie gelöscht");

    // Die dritte als ausgelassen markieren: bleibt bei „diese und folgende“ unverändert
    let third = all[2]["id"].as_str().unwrap();
    let (s, _) = send(&f, Method::PATCH, &format!("/api/v1/plan/units/{third}"), Some(json!({ "status": "skipped" }))).await;
    assert_eq!(s, StatusCode::OK);

    // Zweite ändern, diese und folgende: Titel und +1 Tag, Status nur hier
    let second = all[1]["id"].as_str().unwrap();
    let (s, v) = send(
        &f,
        Method::PATCH,
        &format!("/api/v1/plan/units/{second}?scope=following"),
        Some(json!({ "title": "Grundlage lang", "date": day(9).to_string(), "status": "skipped" })),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let all = units_between(&f, day(0), day(60)).await;
    let row = |n: i64| {
        let u = all.iter().find(|u| u["date"] == day(n).to_string().as_str()).unwrap_or_else(|| panic!("kein Tag {n}"));
        (u["title"].as_str().unwrap().to_string(), u["status"].as_str().unwrap().to_string())
    };
    assert_eq!(row(1).0, "Grundlage");
    assert_eq!(row(9), ("Grundlage lang".into(), "skipped".into()));
    assert_eq!(row(15), ("Grundlage".into(), "skipped".into()), "ausgelassene bleibt");
    assert_eq!(row(23), ("Grundlage lang".into(), "planned".into()));
    assert_eq!(row(30).0, "Grundlage lang");

    // Löschen ab day 23: löscht day 23 und 30, nicht die ausgelassene davor
    let fourth = all.iter().find(|u| u["date"] == day(23).to_string().as_str()).unwrap()["id"].as_str().unwrap().to_string();
    let (s, _) = send(&f, Method::DELETE, &format!("/api/v1/plan/units/{fourth}?scope=following"), None).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    assert_eq!(units_between(&f, day(0), day(60)).await.len(), 3);

    // Ungültiger Geltungsbereich, Wiederholen mit „folgende“
    let (s, _) = send(&f, Method::DELETE, &format!("/api/v1/plan/units/{id}?scope=all"), None).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = send(
        &f,
        Method::PATCH,
        &format!("/api/v1/plan/units/{id}?scope=following"),
        Some(json!({ "repeat": { "every_weeks": 1, "until": day(50).to_string() } })),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    // Wiederholen ab einer vorhandenen Einheit: neue Serie mit 3 Kopien
    let (s, v) = send(
        &f,
        Method::PATCH,
        &format!("/api/v1/plan/units/{id}"),
        Some(json!({ "repeat": { "every_weeks": 2, "until": day(43).to_string() } })),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_ne!(v["data"]["series_id"].as_str(), Some(series.as_str()));
    assert_eq!(v["data"]["series_following"], 3);
}

/// Migration 18 übernimmt das bisherige einzelne Ziel in die Ziel-Tabelle (Sportklasse aus der Planungshilfe) und
/// macht vorgeschlagene Einheiten zu normalen Einheiten.
#[tokio::test]
async fn migration_18_moves_goal_to_table() {
    let dir = common::temp();
    let cfg = common::validated(dir.path());
    std::fs::create_dir_all(&cfg.raw.paths.database_dir).unwrap();
    let db = Db::open(&cfg.database_file()).await.unwrap();
    let pending = db.pending_migrations().await.unwrap();
    let upto17: Vec<_> = pending.iter().copied().filter(|(v, _)| *v <= 17).collect();
    db.migrate(&upto17).await.unwrap();
    sqlx::raw_sql(
        r#"UPDATE plan_settings SET goal_date = '2027-02-07', goal_name = 'Radmarathon', updated_at_ms = 5,
             assistant = '{"sport_class":"running","minutes":[0,60,0,60,0,120,90],"max_gain":5,"recovery_weeks":true,"intensity":"base"}';
           INSERT INTO planned_units (id, date, sport_class, title, mode, zone_minutes, created_at_ms, updated_at_ms, suggestion_id)
             VALUES ('u1', '2027-01-10', 'running', 'Grundlage', 'zones', '[10,50,0,0,0]', 0, 0, 's1');"#,
    )
    .execute(&db.writer)
    .await
    .unwrap();
    let rest = db.pending_migrations().await.unwrap();
    db.migrate(&rest).await.unwrap();
    let goals: Vec<(String, String, String, i64)> =
        sqlx::query_as("SELECT date, sport_class, name, updated_at_ms FROM plan_goals").fetch_all(&db.reader).await.unwrap();
    assert_eq!(goals, vec![("2027-02-07".into(), "running".into(), "Radmarathon".into(), 5)]);
    let id: String = sqlx::query_scalar("SELECT id FROM plan_goals").fetch_one(&db.reader).await.unwrap();
    assert_eq!(id.len(), 36, "{id}");
    let assistant: Option<String> =
        sqlx::query_scalar("SELECT assistant FROM plan_settings WHERE id = 1").fetch_one(&db.reader).await.unwrap();
    assert!(assistant.unwrap().contains("running"));
    let units: i64 = sqlx::query_scalar("SELECT count(*) FROM planned_units").fetch_one(&db.reader).await.unwrap();
    assert_eq!(units, 1);
    db.close().await;
}

#[tokio::test]
async fn unit_templates_crud() {
    let f = fixture().await;
    let body = json!({
        "name": " 4×8 Z4 ", "sport_class": "cycling", "title": "Intervalle 4×8", "mode": "zones",
        "zone_minutes": [15, 20, 0, 32, 0], "note": "Kadenz hoch"
    });
    let (s, v) = send(&f, Method::POST, "/api/v1/plan/templates", Some(body)).await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    let t = &v["data"];
    let id = t["id"].as_str().unwrap().to_string();
    assert_eq!(
        (t["name"].as_str(), t["title"].as_str(), t["load"]["minutes"].as_u64()),
        (Some("4×8 Z4"), Some("Intervalle 4×8"), Some(67))
    );
    assert_eq!(t["load"]["method"], "estimated");
    assert!(t.get("date").is_none() && t.get("status").is_none());

    // Name eindeutig ohne Groß/Klein; Pflichtfelder; Prüfungen wie bei Einheiten
    let lang =
        json!({ "name": "4×8 z4", "sport_class": "cycling", "title": "Lang", "mode": "target", "duration_min": 180, "target_load": 150 });
    let (s, v) = send(&f, Method::POST, "/api/v1/plan/templates", Some(lang.clone())).await;
    assert_eq!((s, v["error"]["code"].as_str()), (StatusCode::CONFLICT, Some("plan_template_exists")));
    let (s, v) =
        send(&f, Method::POST, "/api/v1/plan/templates", Some(json!({ "name": "X", "sport_class": "cycling", "mode": "zones" }))).await;
    assert_eq!((s, field_of(&v)), (StatusCode::BAD_REQUEST, Some("title")));
    let mut bad = lang.clone();
    bad["name"] = json!("Lange Einheit");
    bad["target_load"] = json!(5000);
    let (s, v) = send(&f, Method::POST, "/api/v1/plan/templates", Some(bad)).await;
    assert_eq!((s, field_of(&v)), (StatusCode::BAD_REQUEST, Some("target_load")));
    let mut ok = lang.clone();
    ok["name"] = json!("Lange Einheit");
    let (s, _) = send(&f, Method::POST, "/api/v1/plan/templates", Some(ok)).await;
    assert_eq!(s, StatusCode::CREATED);
    let names: Vec<String> =
        get(&f, "/api/v1/plan/templates").await.as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap().to_string()).collect();
    assert_eq!(names, vec!["4×8 Z4", "Lange Einheit"]);

    // Umbenennen und ändern; Umbenennen auf einen vorhandenen Namen geht nicht
    let path = format!("/api/v1/plan/templates/{id}");
    let (s, v) =
        send(&f, Method::PATCH, &path, Some(json!({ "name": "Intervalle kurz", "zone_minutes": [15, 20, 0, 24, 0], "note": null }))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(
        (v["data"]["name"].as_str(), v["data"]["load"]["minutes"].as_u64(), v["data"]["note"].clone()),
        (Some("Intervalle kurz"), Some(59), Value::Null)
    );
    let (s, v) = send(&f, Method::PATCH, &path, Some(json!({ "name": "lange einheit" }))).await;
    assert_eq!((s, v["error"]["code"].as_str()), (StatusCode::CONFLICT, Some("plan_template_exists")));

    // Löschen
    let (s, _) = send(&f, Method::DELETE, &path, None).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, v) = send(&f, Method::PATCH, &path, Some(json!({ "name": "Y" }))).await;
    assert_eq!((s, v["error"]["code"].as_str()), (StatusCode::NOT_FOUND, Some("plan_template_not_found")));
    assert_eq!(get(&f, "/api/v1/plan/templates").await.as_array().unwrap().len(), 1);
}
