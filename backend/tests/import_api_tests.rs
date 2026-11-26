//! Import-API (RFC-0003 Abschnitt 8, RFC-0008 Abschnitte 1, 7, 8): Upload, Sperren, Duplikate, Ersetzen,
//! Zeiteingabe, Abbruch, Wiederanlauf und Aufbewahrung (C12), Audit (C11).

mod common;

use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use rustyfit::profile::{mesgdef, typedef};
use rustyfit::proto::{FIT, Message};
use serde_json::{Value, json};
use trackcraft::api::{self, AppState};
use trackcraft::config::{self, Validated};
use trackcraft::db::Db;
use trackcraft::{import_job, startup};

const HOST: &str = "trackcraft.test";
const ORIGIN: &str = "https://trackcraft.test";
const T0: i64 = 1_782_892_800; // 2026-07-01T08:00:00Z

struct Fixture {
    dir: tempfile::TempDir,
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
    let app = api::router(AppState { cfg: cfg.clone(), db: db.clone(), garmin: Default::default(), backup: Default::default() });
    Fixture { dir, cfg, app, db }
}

async fn fixture() -> Fixture {
    fixture_with("").await
}

async fn send(app: &Router, req: Request<Body>) -> (StatusCode, Value) {
    common::contract::call_json(app, req).await
}

fn code(v: &Value) -> &str {
    v["error"]["code"].as_str().unwrap_or("")
}

const BOUNDARY: &str = "----trackcraft-test";

fn multipart(name: &str, bytes: &[u8]) -> Vec<u8> {
    let mut body = format!(
        "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{name}\"\r\nContent-Type: application/octet-stream\r\n\r\n"
    )
    .into_bytes();
    body.extend_from_slice(bytes);
    body.extend_from_slice(format!("\r\n--{BOUNDARY}--\r\n").as_bytes());
    body
}

async fn upload(f: &Fixture, name: &str, bytes: &[u8]) -> (StatusCode, Value) {
    let body = multipart(name, bytes);
    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/activities/import")
        .header("host", HOST)
        .header("origin", ORIGIN)
        .header("content-type", format!("multipart/form-data; boundary={BOUNDARY}"))
        .header("content-length", body.len())
        .body(Body::from(body))
        .unwrap();
    send(&f.app, req).await
}

async fn confirm(f: &Fixture, id: &str, decision: Value) -> (StatusCode, Value) {
    let req = Request::builder()
        .method(Method::POST)
        .uri(format!("/api/v1/import/jobs/{id}/confirm"))
        .header("host", HOST)
        .header("origin", ORIGIN)
        .header("content-type", "application/json")
        .body(Body::from(decision.to_string()))
        .unwrap();
    send(&f.app, req).await
}

async fn get(f: &Fixture, path: &str) -> (StatusCode, Value) {
    send(&f.app, Request::builder().uri(path).header("host", HOST).body(Body::empty()).unwrap()).await
}

async fn delete(f: &Fixture, path: &str) -> (StatusCode, Value) {
    let req =
        Request::builder().method(Method::DELETE).uri(path).header("host", HOST).header("origin", ORIGIN).body(Body::empty()).unwrap();
    send(&f.app, req).await
}

async fn count(db: &Db, sql: &str) -> i64 {
    sqlx::query_scalar(sqlx::AssertSqlSafe(sql.to_string())).fetch_one(&db.reader).await.unwrap()
}

fn staging_entries(f: &Fixture) -> usize {
    std::fs::read_dir(f.cfg.staging_dir()).unwrap().count()
}

fn archive(f: &Fixture) -> PathBuf {
    f.dir.path().join("data/activities")
}

/// GPX mit `n` Punkten im Abstand von 10 s und gut 33 m (12 km/h), Typ `cycling`.
fn gpx(name: &str, n: usize, with_time: bool) -> Vec<u8> {
    let mut s =
        String::from("<?xml version=\"1.0\"?>\n<gpx version=\"1.1\" creator=\"test\" xmlns=\"http://www.topografix.com/GPX/1/1\">\n<trk>");
    s.push_str(&format!("<name>{name}</name><type>cycling</type><trkseg>\n"));
    for i in 0..n {
        let lat = 47.0 + i as f64 * 0.0003;
        let time = if with_time {
            let t = chrono::DateTime::from_timestamp(T0 + i as i64 * 10, 0).unwrap();
            format!("<time>{}</time>", t.format("%Y-%m-%dT%H:%M:%SZ"))
        } else {
            String::new()
        };
        s.push_str(&format!("<trkpt lat=\"{lat:.6}\" lon=\"11.000000\"><ele>{}</ele>{time}</trkpt>\n", 500 + i));
    }
    s.push_str("</trkseg></trk></gpx>\n");
    s.into_bytes()
}

/// FIT ohne GPS und ohne Zeitangaben: nur Herzfrequenz; Start und Gesamtzeit müssen eingegeben werden.
fn fit_without_time() -> Vec<u8> {
    let mut file_id = mesgdef::FileId::new();
    file_id.r#type = typedef::File::ACTIVITY;
    let mut messages: Vec<Message> = vec![file_id.into()];
    for _ in 0..5 {
        let mut r = mesgdef::Record::new();
        r.heart_rate = 120;
        messages.push(r.into());
    }
    let mut s = mesgdef::Session::new();
    s.sport = typedef::Sport::CYCLING;
    messages.push(s.into());
    let mut enc = rustyfit::EncoderBuilder::new().build();
    let mut fit = FIT { messages, ..Default::default() };
    let mut out = embedded_io_adapters::std::FromStd::new(std::io::Cursor::new(Vec::<u8>::new()));
    enc.encode(&mut out, &mut fit).unwrap();
    out.into_inner().into_inner()
}

fn job_id(v: &Value) -> String {
    v["data"]["id"].as_str().unwrap().to_string()
}

fn token(v: &Value) -> String {
    v["data"]["state_token"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn upload_and_import_gpx() {
    let f = fixture().await;
    let (s, v) = upload(&f, "C:\\Touren\\Morgenrunde.gpx", &gpx("Morgenrunde", 60, true)).await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    let job = &v["data"];
    assert_eq!(job["status"], "awaiting_decision");
    assert_eq!(job["format"], "gpx");
    assert_eq!(job["original_filename"], "Morgenrunde.gpx");
    let session = &job["sessions"][0];
    assert_eq!(session["state"], "selectable");
    assert_eq!(session["preselected"], true);
    assert_eq!(session["title"], "Morgenrunde");
    assert_eq!(session["sport_class"], "cycling");
    assert_eq!(session["start_time"], "2026-07-01T08:00:00.000Z");
    assert_eq!(session["total_time_ms"], 590_000);
    assert_eq!(session["point_count"], 60);
    assert_eq!(session["avg_heart_rate_bpm"], Value::Null);
    assert_eq!(session["duplicate_candidates"], json!([]));
    assert!(job["state_token"].as_str().unwrap().starts_with("st_"));
    // Vor der Bestätigung ist nichts gespeichert.
    assert_eq!(count(&f.db, "SELECT count(*) FROM activities").await, 0);
    assert_eq!(staging_entries(&f), 1);

    let id = job_id(&v);
    let (s, v) = confirm(&f, &id, json!({ "state_token": token(&v), "sessions": [{ "session_index": 0, "action": "import" }] })).await;
    assert_eq!(s, StatusCode::ACCEPTED, "{v}");
    assert_eq!(v["data"]["status"], "done");
    assert_eq!(v["data"]["state_token"], Value::Null);
    let activity = v["data"]["imported_activity_ids"][0].as_str().unwrap().to_string();

    let row = sqlx::query_as::<_, (String, String, i64, String, i64)>(
        "SELECT title, sport_class, start_time_ms, start_time_origin, total_time_ms FROM activities WHERE id = ?",
    )
    .bind(&activity)
    .fetch_one(&f.db.reader)
    .await
    .unwrap();
    assert_eq!(row, ("Morgenrunde".into(), "cycling".into(), T0 * 1000, "first_point".into(), 590_000));
    assert_eq!(count(&f.db, "SELECT count(*) FROM track_sections").await, 1);
    assert_eq!(count(&f.db, "SELECT sum(point_count) FROM track_sections").await, 60);
    assert_eq!(count(&f.db, "SELECT count(*) FROM jobs WHERE kind = 'activity_evaluation' AND status = 'queued'").await, 1);
    assert_eq!(count(&f.db, "SELECT count(*) FROM audit_log WHERE action = 'import' AND outcome = 'ok'").await, 1);
    assert_eq!(count(&f.db, "SELECT change_counter FROM data_state").await, 1);

    // Original unter sha256.gpx abgelegt, Staging leer.
    let rel: String = sqlx::query_scalar("SELECT relative_path FROM original_files").fetch_one(&f.db.reader).await.unwrap();
    assert!(rel.ends_with(".gpx") && rel.len() == 68, "{rel}");
    assert!(archive(&f).join(&rel).is_file());
    assert_eq!(staging_entries(&f), 0);

    // Erneute Bestätigung geht nicht mehr.
    let (s, v) = confirm(&f, &id, json!({ "state_token": "st_x", "sessions": [{ "session_index": 0, "action": "skip" }] })).await;
    assert_eq!((s, code(&v)), (StatusCode::CONFLICT, "import_not_confirmable"));
    f.db.close().await;
}

#[tokio::test]
async fn identical_file_is_locked_existing() {
    let f = fixture().await;
    let bytes = gpx("Runde", 30, true);
    let (_, v) = upload(&f, "a.gpx", &bytes).await;
    let (s, _) =
        confirm(&f, &job_id(&v), json!({ "state_token": token(&v), "sessions": [{ "session_index": 0, "action": "import" }] })).await;
    assert_eq!(s, StatusCode::ACCEPTED);
    let existing: String = sqlx::query_scalar("SELECT id FROM activities").fetch_one(&f.db.reader).await.unwrap();

    let (s, v) = upload(&f, "a-kopie.gpx", &bytes).await;
    assert_eq!(s, StatusCode::CREATED);
    let session = &v["data"]["sessions"][0];
    assert_eq!(session["state"], "locked_existing");
    assert_eq!(session["preselected"], false);
    assert_eq!(session["existing_activity_id"], existing.as_str());

    let id = job_id(&v);
    let (s, e) = confirm(&f, &id, json!({ "state_token": token(&v), "sessions": [{ "session_index": 0, "action": "import" }] })).await;
    assert_eq!((s, code(&e)), (StatusCode::BAD_REQUEST, "invalid_import_decision"));

    // Abbrechen bereinigt den Upload.
    assert_eq!(staging_entries(&f), 1);
    let (s, _) = delete(&f, &format!("/api/v1/import/jobs/{id}")).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    assert_eq!(staging_entries(&f), 0);
    let (s, e) = get(&f, &format!("/api/v1/import/jobs/{id}")).await;
    assert_eq!((s, code(&e)), (StatusCode::NOT_FOUND, "import_job_not_found"));
    assert_eq!(count(&f.db, "SELECT count(*) FROM original_files").await, 1);
    f.db.close().await;
}

#[tokio::test]
async fn duplicate_candidate_can_be_replaced() {
    let f = fixture().await;
    let (_, v) = upload(&f, "edge.gpx", &gpx("Edge", 60, true)).await;
    let (_, v) =
        confirm(&f, &job_id(&v), json!({ "state_token": token(&v), "sessions": [{ "session_index": 0, "action": "import" }] })).await;
    let old = v["data"]["imported_activity_ids"][0].as_str().unwrap().to_string();
    let old_rel: String = sqlx::query_scalar("SELECT relative_path FROM original_files").fetch_one(&f.db.reader).await.unwrap();
    sqlx::query("UPDATE activities SET description = 'Mit Anna', sub_sport = 'road' WHERE id = ?")
        .bind(&old)
        .execute(&f.db.writer)
        .await
        .unwrap();

    // Andere Datei derselben Fahrt: ein Punkt mehr, anderer Name.
    let (s, v) = upload(&f, "strava.gpx", &gpx("Strava", 61, true)).await;
    assert_eq!(s, StatusCode::CREATED);
    let candidates = &v["data"]["sessions"][0]["duplicate_candidates"];
    assert_eq!(candidates.as_array().unwrap().len(), 1, "{v}");
    assert_eq!(candidates[0]["activity_id"], old.as_str());
    assert_eq!(candidates[0]["matched_criteria"], json!(["start_time", "total_time", "distance"]));
    assert_eq!(v["data"]["sessions"][0]["state"], "selectable");

    let id = job_id(&v);
    let tok = token(&v);
    // Ziel kein Kandidat
    let (s, e) = confirm(
        &f,
        &id,
        json!({ "state_token": tok, "sessions": [{ "session_index": 0, "action": "replace", "replace_activity_id": "0198c1f2-6b3a-7c1e-9d44-2f6a1b8e5a10" }] }),
    )
    .await;
    assert_eq!((s, code(&e)), (StatusCode::BAD_REQUEST, "invalid_import_decision"));
    // replace ohne Ziel
    let (s, e) = confirm(&f, &id, json!({ "state_token": tok, "sessions": [{ "session_index": 0, "action": "replace" }] })).await;
    assert_eq!((s, code(&e)), (StatusCode::BAD_REQUEST, "validation_failed"));

    let (s, v) = confirm(
        &f,
        &id,
        json!({ "state_token": tok, "sessions": [{
            "session_index": 0, "action": "replace", "replace_activity_id": old,
            "carry_over": { "title": false, "sub_sport": false }
        }] }),
    )
    .await;
    assert_eq!(s, StatusCode::ACCEPTED, "{v}");
    assert_eq!(v["data"]["status"], "done");
    assert_eq!(v["data"]["imported_activity_ids"], json!([old]));

    let row = sqlx::query_as::<_, (String, Option<String>, Option<String>, i64, i64)>(
        "SELECT title, description, sub_sport, revision, total_time_ms FROM activities WHERE id = ?",
    )
    .bind(&old)
    .fetch_one(&f.db.reader)
    .await
    .unwrap();
    assert_eq!(row, ("Strava".into(), Some("Mit Anna".into()), None, 2, 600_000));
    assert_eq!(count(&f.db, "SELECT count(*) FROM activities").await, 1);
    assert_eq!(count(&f.db, "SELECT sum(point_count) FROM track_sections").await, 61);
    // Das alte Original hat keinen Verweis mehr und ist entfernt.
    assert_eq!(count(&f.db, "SELECT count(*) FROM original_files").await, 1);
    assert!(!archive(&f).join(&old_rel).exists());
    assert_eq!(count(&f.db, "SELECT count(*) FROM audit_log WHERE action = 'replace'").await, 1);
    f.db.close().await;
}

#[tokio::test]
async fn changed_state_needs_new_confirmation() {
    let f = fixture().await;
    let (_, a) = upload(&f, "a.gpx", &gpx("A", 60, true)).await;
    let (_, b) = upload(&f, "b.gpx", &gpx("B", 61, true)).await;
    // Beide ohne Kandidaten; A wird gespeichert, damit ist A Kandidat für B.
    assert_eq!(b["data"]["sessions"][0]["duplicate_candidates"], json!([]));
    let (s, _) =
        confirm(&f, &job_id(&a), json!({ "state_token": token(&a), "sessions": [{ "session_index": 0, "action": "import" }] })).await;
    assert_eq!(s, StatusCode::ACCEPTED);

    let (s, e) =
        confirm(&f, &job_id(&b), json!({ "state_token": token(&b), "sessions": [{ "session_index": 0, "action": "import" }] })).await;
    assert_eq!((s, code(&e)), (StatusCode::CONFLICT, "import_state_changed"));
    let (_, fresh) = get(&f, &format!("/api/v1/import/jobs/{}", job_id(&b))).await;
    assert_ne!(token(&fresh), token(&b));
    assert_eq!(fresh["data"]["sessions"][0]["duplicate_candidates"].as_array().unwrap().len(), 1);
    // Zusätzlich importieren ist erlaubt.
    let (s, v) = confirm(&f, &job_id(&b), json!({ "state_token": token(&fresh), "sessions": [{ "session_index": 0, "action": "import", "title": "  Zweite Aufzeichnung " }] })).await;
    assert_eq!(s, StatusCode::ACCEPTED, "{v}");
    let titles: Vec<String> = sqlx::query_scalar("SELECT title FROM activities ORDER BY title").fetch_all(&f.db.reader).await.unwrap();
    assert_eq!(titles, ["A", "Zweite Aufzeichnung"]);
    f.db.close().await;
}

#[tokio::test]
async fn skip_only_imports_nothing() {
    let f = fixture().await;
    let (_, v) = upload(&f, "a.gpx", &gpx("A", 10, true)).await;
    let (s, v) =
        confirm(&f, &job_id(&v), json!({ "state_token": token(&v), "sessions": [{ "session_index": 0, "action": "skip" }] })).await;
    assert_eq!(s, StatusCode::ACCEPTED);
    assert_eq!(v["data"]["status"], "done");
    assert_eq!(v["data"]["imported_activity_ids"], json!([]));
    assert_eq!(count(&f.db, "SELECT count(*) FROM activities").await, 0);
    assert_eq!(count(&f.db, "SELECT count(*) FROM original_files").await, 0);
    assert_eq!(staging_entries(&f), 0);
    assert_eq!(std::fs::read_dir(archive(&f)).unwrap().count(), 0);
    f.db.close().await;
}

#[tokio::test]
async fn rejected_uploads_leave_nothing() {
    let f = fixture_with("[import]\nmax_file_size_mb = 1\n").await;
    let (s, e) = upload(&f, "route.gpx", &gpx("Route", 10, false)).await;
    assert_eq!((s, code(&e)), (StatusCode::BAD_REQUEST, "gpx_without_time"));
    let (s, e) = upload(&f, "bild.png", b"\x89PNG\r\n\x1a\nnot an activity").await;
    assert_eq!((s, code(&e)), (StatusCode::UNSUPPORTED_MEDIA_TYPE, "unsupported_format"));
    let (s, e) = upload(&f, "kaputt.gpx", b"<?xml version=\"1.0\"?><gpx><trk><trkseg><trkpt lat=\"1\"").await;
    assert_eq!((s, code(&e)), (StatusCode::BAD_REQUEST, "invalid_file"));
    let (s, e) = upload(&f, "gross.gpx", &vec![b' '; 1024 * 1024 + 1]).await;
    assert_eq!((s, code(&e)), (StatusCode::PAYLOAD_TOO_LARGE, "file_too_large"));

    // Ohne Datei-Part
    let body = format!("--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"other\"\r\n\r\nx\r\n--{BOUNDARY}--\r\n");
    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/activities/import")
        .header("host", HOST)
        .header("origin", ORIGIN)
        .header("content-type", format!("multipart/form-data; boundary={BOUNDARY}"))
        .body(Body::from(body))
        .unwrap();
    let (s, e) = send(&f.app, req).await;
    assert_eq!((s, code(&e)), (StatusCode::BAD_REQUEST, "validation_failed"));

    assert_eq!(count(&f.db, "SELECT count(*) FROM import_jobs").await, 0);
    assert_eq!(staging_entries(&f), 0);
    f.db.close().await;
}

#[tokio::test]
async fn session_without_time_needs_input() {
    let f = fixture().await;
    let (s, v) = upload(&f, "indoor.fit", &fit_without_time()).await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    let session = &v["data"]["sessions"][0];
    assert_eq!(session["time_input_required"], true);
    assert_eq!(session["start_time"], Value::Null);
    assert_eq!(session["has_position"], false);
    assert_eq!(session["title"], "Radfahren");
    let (id, tok) = (job_id(&v), token(&v));
    let decide = |extra: Value| {
        let mut d = json!({ "session_index": 0, "action": "import" });
        d.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        json!({ "state_token": tok, "sessions": [d] })
    };

    let (s, e) = confirm(&f, &id, decide(json!({}))).await;
    assert_eq!((s, code(&e)), (StatusCode::BAD_REQUEST, "time_input_required"));
    let (s, e) = confirm(&f, &id, decide(json!({ "start_time": { "local": "2026-10-25T02:30:00" }, "total_time_ms": 3_600_000 }))).await;
    assert_eq!((s, code(&e)), (StatusCode::BAD_REQUEST, "local_time_ambiguous"));
    let (s, e) = confirm(&f, &id, decide(json!({ "start_time": { "local": "gestern" }, "total_time_ms": 3_600_000 }))).await;
    assert_eq!((s, code(&e)), (StatusCode::BAD_REQUEST, "validation_failed"));

    // Frühjahrslücke: 02:30 wird 03:30 MESZ = 01:30 UTC, mit Hinweis.
    let (s, v) = confirm(&f, &id, decide(json!({ "start_time": { "local": "2026-03-29T02:30:00" }, "total_time_ms": 3_600_000 }))).await;
    assert_eq!(s, StatusCode::ACCEPTED, "{v}");
    assert_eq!(v["meta"]["notices"][0]["code"], "local_time_shifted");
    let row = sqlx::query_as::<_, (String, i64, String, i64, String)>(
        "SELECT title, start_time_ms, start_time_origin, total_time_ms, total_time_origin FROM activities",
    )
    .fetch_one(&f.db.reader)
    .await
    .unwrap();
    let expected = chrono::DateTime::parse_from_rfc3339("2026-03-29T01:30:00Z").unwrap().timestamp_millis();
    assert_eq!(row, ("Radfahren 29.03.2026 03:30".into(), expected, "manual".into(), 3_600_000, "manual".into()));
    f.db.close().await;
}

#[tokio::test]
async fn wrong_file_checksum_is_imported_with_notice_and_audit() {
    let f = fixture().await;
    let mut bytes = fit_without_time();
    let n = bytes.len();
    bytes[n - 1] ^= 0xFF;
    let (s, v) = upload(&f, "alt.fit", &bytes).await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    let decision = json!({ "state_token": token(&v), "sessions": [{
        "session_index": 0, "action": "import",
        "start_time": { "local": "2026-07-01T10:00:00" }, "total_time_ms": 60_000,
    }] });
    let (s, v) = confirm(&f, &job_id(&v), decision).await;
    assert_eq!(s, StatusCode::ACCEPTED, "{v}");
    assert_eq!(v["meta"]["notices"][0]["code"], "file_checksum_ignored");
    assert_eq!(
        count(&f.db, "SELECT count(*) FROM audit_log WHERE action = 'import' AND json_extract(details, '$.checksum_ignored') = 1").await,
        1
    );
    f.db.close().await;
}

#[tokio::test]
async fn request_validation() {
    let f = fixture().await;
    let (_, v) = upload(&f, "a.gpx", &gpx("A", 10, true)).await;
    let id = job_id(&v);
    let tok = token(&v);
    let cases = [
        (json!({ "state_token": tok, "sessions": [], "x": 1 }), "validation_failed"),
        (json!({ "state_token": tok, "sessions": [] }), "validation_failed"),
        (json!({ "state_token": tok, "sessions": [{ "session_index": 3, "action": "import" }] }), "invalid_import_decision"),
        (
            json!({ "state_token": tok, "sessions": [{ "session_index": 0, "action": "import" }, { "session_index": 0, "action": "skip" }] }),
            "invalid_import_decision",
        ),
        (json!({ "state_token": tok, "sessions": [{ "session_index": 0, "action": "import", "title": " " }] }), "validation_failed"),
        (
            json!({ "state_token": tok, "sessions": [{ "session_index": 0, "action": "import", "start_time": { "local": "2026-07-01T10:00:00" }, "total_time_ms": 1 }] }),
            "invalid_import_decision",
        ),
        (json!({ "state_token": "st_falsch", "sessions": [{ "session_index": 0, "action": "import" }] }), "import_state_changed"),
    ];
    for (body, expected) in cases {
        let (_, e) = confirm(&f, &id, body.clone()).await;
        assert_eq!(code(&e), expected, "{body}");
    }
    let (s, e) = confirm(&f, "0198c200-0000-7000-8000-0000000000b1", json!({ "state_token": "x", "sessions": [] })).await;
    assert_eq!((s, code(&e)), (StatusCode::NOT_FOUND, "import_job_not_found"));
    let (s, e) = get(&f, "/api/v1/import/jobs/..%2F..%2Fx").await;
    assert_eq!((s, code(&e)), (StatusCode::NOT_FOUND, "import_job_not_found"));
    let (s, e) = delete(&f, "/api/v1/import/jobs/0198c200-0000-7000-8000-0000000000b1").await;
    assert_eq!((s, code(&e)), (StatusCode::NOT_FOUND, "import_job_not_found"));

    // Liste mit Filter und Paging
    let (s, l) = get(&f, "/api/v1/import/jobs?status=awaiting_decision&limit=10").await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(l["meta"], json!({ "total": 1, "limit": 10, "offset": 0 }));
    assert_eq!(l["data"][0]["id"], id.as_str());
    let (s, e) = get(&f, "/api/v1/import/jobs?sort=x").await;
    assert_eq!((s, code(&e)), (StatusCode::BAD_REQUEST, "validation_failed"));
    let (s, e) = get(&f, "/api/v1/import/jobs?status=fertig").await;
    assert_eq!((s, code(&e)), (StatusCode::BAD_REQUEST, "validation_failed"));
    let (s, e) = get(&f, "/api/v1/import/jobs?limit=500").await;
    assert_eq!((s, code(&e)), (StatusCode::BAD_REQUEST, "validation_failed"));
    f.db.close().await;
}

#[tokio::test]
async fn restart_rolls_back_interrupted_import_and_keeps_open_ones() {
    let f = fixture().await;
    let (_, open) = upload(&f, "offen.gpx", &gpx("Offen", 10, true)).await;
    let open_id = job_id(&open);

    // Ein unterbrochener Import: Status running, Original schon abgelegt, aber nicht in der Datenbank.
    let sha = "a".repeat(64);
    let stray = archive(&f).join(format!("{sha}.gpx"));
    std::fs::write(&stray, b"x").unwrap();
    sqlx::query("INSERT INTO import_jobs (id, original_filename, format, size_bytes, sha256, status, created_at_ms, sessions_json) VALUES ('0198c200-0000-7000-8000-000000000001', 'x.gpx', 'gpx', 1, ?, 'running', 0, '[]')")
        .bind(&sha)
        .execute(&f.db.writer)
        .await
        .unwrap();
    let unknown = f.cfg.staging_dir().join("unbekannt");
    std::fs::create_dir_all(&unknown).unwrap();
    f.db.close().await;

    let started = startup::prepare(&f.cfg).await.unwrap();
    let db = started.db.clone();
    let status: (String, Option<String>) =
        sqlx::query_as("SELECT status, error_code FROM import_jobs WHERE id = '0198c200-0000-7000-8000-000000000001'")
            .fetch_one(&db.reader)
            .await
            .unwrap();
    assert_eq!(status, ("rolled_back".into(), Some("interrupted".into())));
    assert!(!stray.exists());
    assert!(!unknown.exists());
    assert!(import_job::staging_dir(&f.cfg, &open_id).is_dir());
    assert_eq!(count(&db, "SELECT count(*) FROM audit_log WHERE outcome = 'failed'").await, 1);
    db.close().await;
    drop(started);
}

#[tokio::test]
async fn retention_removes_orphans_and_old_entries() {
    let f = fixture().await;
    let (_, v) = upload(&f, "a.gpx", &gpx("A", 10, true)).await;
    let id = job_id(&v);
    let now = chrono::Utc::now().timestamp_millis();
    let hour = 3_600_000;
    let day = 24 * hour;

    // Noch nicht verwaist
    let c = import_job::cleanup(&f.cfg, &f.db, now + 23 * hour).await.unwrap();
    assert_eq!(c.orphaned_imports, 0);
    for (i, finished) in [(1, now - 31 * day), (2, now - 28 * day)] {
        sqlx::query("INSERT INTO import_jobs (id, original_filename, status, created_at_ms, finished_at_ms) VALUES (?, 'x', 'done', 0, ?)")
            .bind(format!("0198c200-0000-7000-8000-00000000000{i}"))
            .bind(finished)
            .execute(&f.db.writer)
            .await
            .unwrap();
        sqlx::query("INSERT INTO jobs (id, kind, status, created_at_ms, finished_at_ms) VALUES (?, 'recalculation', 'done', 0, ?)")
            .bind(format!("job-{i}"))
            .bind(finished)
            .execute(&f.db.writer)
            .await
            .unwrap();
    }
    sqlx::query("INSERT INTO audit_log (id, at_ms, action, object_type, outcome) VALUES ('alt', ?, 'import', 'activity', 'ok')")
        .bind(now - 366 * day)
        .execute(&f.db.writer)
        .await
        .unwrap();

    let c = import_job::cleanup(&f.cfg, &f.db, now + 25 * hour).await.unwrap();
    assert_eq!(c, import_job::Cleaned { orphaned_imports: 1, finished_imports: 1, finished_jobs: 1, audit_entries: 1 });
    assert!(!import_job::staging_dir(&f.cfg, &id).exists());
    let (s, _) = get(&f, &format!("/api/v1/import/jobs/{id}")).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    assert_eq!(count(&f.db, "SELECT count(*) FROM import_jobs").await, 1);
    assert_eq!(count(&f.db, "SELECT count(*) FROM jobs").await, 1);
    f.db.close().await;
}
