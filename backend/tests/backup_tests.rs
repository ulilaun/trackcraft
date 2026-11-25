//! Sicherung (#57): Stand im Ziel, Prüfung gegen das Manifest, keine Sicherung ohne Änderung oder ohne Ziel,
//! Aufbewahrung der letzten fünf, Status über die API.

mod common;

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;
use trackcraft::api::{self, AppState};
use trackcraft::backup::{Outcome, Runner, Trigger, format};
use trackcraft::config::Validated;
use trackcraft::db::Db;
use trackcraft::startup;

const HOST: &str = "trackcraft.test";
const ORIGIN: &str = "https://trackcraft.test";
const T0: i64 = 1_782_892_800;

struct Fixture {
    _dir: tempfile::TempDir,
    cfg: Arc<Validated>,
    app: Router,
    db: Db,
    runner: Arc<Runner>,
}

async fn fixture() -> Fixture {
    let dir = common::temp();
    std::fs::write(dir.path().join("trackcraft.toml"), common::config_text(dir.path(), "", "")).unwrap();
    let cfg = Arc::new(common::validated(dir.path()));
    let started = startup::prepare(&cfg).await.unwrap();
    let db = started.db.clone();
    drop(started);
    let runner = Arc::new(Runner::default());
    let app = api::router(AppState { cfg: cfg.clone(), db: db.clone(), garmin: Default::default(), backup: runner.clone() });
    Fixture { _dir: dir, cfg, app, db, runner }
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

async fn attempt(f: &Fixture) -> Outcome {
    f.runner.attempt(&f.cfg, &f.db, Trigger::Manual).await.expect("kein paralleler Lauf").1
}

#[tokio::test]
async fn backup_needs_target_and_changes() {
    let f = fixture().await;
    let target = f.cfg.raw.backup.target_dir.clone();

    // Ziel fehlt: übersprungen, nicht angelegt, kein Erfolg
    assert!(matches!(attempt(&f).await, Outcome::TargetMissing(_)));
    assert!(!target.exists());
    let (_, v) = send(&f, Method::GET, "/api/v1/backups/status", None).await;
    let d = &v["data"];
    assert_eq!(d["target"]["state"], "unavailable");
    assert_eq!(d["last_attempt"]["outcome"], "skipped_target_unavailable");
    assert!(d["last_attempt"]["reason"].as_str().unwrap().contains("nicht vorhanden"));
    assert_eq!(d["last_success"], Value::Null);
    assert_eq!(d["pending_changes"], true);
    assert_eq!(d["running_operation_id"], Value::Null);
    let (_, v) = send(&f, Method::GET, "/api/v1/backups", None).await;
    assert_eq!(v["meta"]["target"]["state"], "unavailable");
    assert_eq!(v["data"], json!([]));

    std::fs::create_dir_all(&target).unwrap();
    let id = import(&f, &gpx(T0, 40)).await;

    // Erster Stand: Datenbank, TOML, Original, Manifest; im Ziel geprüft
    let Outcome::Created { name } = attempt(&f).await else { panic!("Sicherung erwartet") };
    let stand = target.join(&name);
    let m = format::read_manifest(&stand).unwrap();
    format::verify(&stand, &m).unwrap();
    assert_eq!(m.format, "trackcraft-backup");
    assert_eq!(m.trigger, "manual");
    assert_eq!(m.schema_version, trackcraft::db::SCHEMA_VERSION);
    assert_eq!((m.originals(), m.activity_count, m.segment_count), (1, 1, 0));
    assert!(m.files.iter().any(|f| f.path == "trackcraft.db"));
    assert!(m.files.iter().any(|f| f.path == "trackcraft.toml"));
    let copy = Db::open(&stand.join("trackcraft.db")).await.unwrap();
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM activities WHERE id = ?").bind(&id).fetch_one(&copy.reader).await.unwrap();
    assert_eq!(n, 1);
    copy.close().await;
    // Zwischenstand im Staging ist weg
    assert!(std::fs::read_dir(f.cfg.staging_dir()).unwrap().next().is_none());

    // Keine Änderung: kein neuer Stand
    assert_eq!(attempt(&f).await, Outcome::Unchanged);
    let (_, v) = send(&f, Method::GET, "/api/v1/backups/status", None).await;
    let d = &v["data"];
    assert_eq!(d["pending_changes"], false);
    assert_eq!(d["target"]["state"], "available");
    assert!(d["target"]["free_bytes"].as_u64().unwrap() > 0);
    assert_eq!(d["last_attempt"]["outcome"], "skipped_no_changes");
    assert_eq!(d["last_success"]["backup_id"], name.as_str());
    assert_eq!(d["last_success"]["revision_id"], m.data_state.as_ref().unwrap().revision_id.as_str());
    let (_, v) = send(&f, Method::GET, "/api/v1/backups", None).await;
    let e = &v["data"][0];
    assert_eq!(v["data"].as_array().unwrap().len(), 1);
    assert_eq!(e["id"], name.as_str());
    assert_eq!(e["kind"], "regular");
    assert_eq!(e["check_status"], "ok");
    assert_eq!(e["activity_count"], 1);
    assert_eq!(e["original_count"], 1);
    assert_eq!(e["format_version"], 1);

    // Ein beschädigter Stand fällt in der Liste auf
    let original = m.files.iter().find(|f| f.path.starts_with("originals/")).unwrap();
    std::fs::write(format::safe_join(&stand, &original.path).unwrap(), b"kaputt").unwrap();
    let (_, v) = send(&f, Method::GET, "/api/v1/backups", None).await;
    assert_eq!(v["data"][0]["check_status"], "failed");
    assert!(format::verify(&stand, &m).is_err());

    // Audit-Eintrag der erfolgreichen Sicherung
    let audits: i64 = sqlx::query_scalar("SELECT count(*) FROM audit_log WHERE action = 'backup' AND outcome = 'ok'")
        .fetch_one(&f.db.reader)
        .await
        .unwrap();
    assert_eq!(audits, 1);
}

#[tokio::test]
async fn retention_keeps_last_five_valid_stands() {
    let f = fixture().await;
    let target = f.cfg.raw.backup.target_dir.clone();
    std::fs::create_dir_all(&target).unwrap();
    // Fremde Ordner und abgebrochene Reste
    std::fs::create_dir_all(target.join("Fotos")).unwrap();
    std::fs::create_dir_all(target.join("trackcraft-20000101-000000-s1-c1.partial")).unwrap();

    let mut names = vec![];
    for i in 0..7 {
        import(&f, &gpx(T0 + i * 86_400, 30)).await;
        let Outcome::Created { name } = attempt(&f).await else { panic!("Sicherung erwartet") };
        names.push(name);
    }
    let left: Vec<String> = format::list(&target).unwrap().into_iter().map(|s| s.name).collect();
    let mut expected: Vec<String> = names[2..].to_vec();
    expected.reverse();
    assert_eq!(left, expected);
    assert!(target.join("Fotos").is_dir());
    assert!(!target.join("trackcraft-20000101-000000-s1-c1.partial").exists());
}

#[tokio::test]
async fn start_endpoint_runs_operation_in_background() {
    let f = fixture().await;
    std::fs::create_dir_all(&f.cfg.raw.backup.target_dir).unwrap();
    // Den heutigen Tagesversuch als erledigt vermerken, damit nur „Jetzt sichern“ läuft.
    let today = chrono::Utc::now().with_timezone(&f.cfg.timezone).format("%Y-%m-%d").to_string();
    sqlx::query("UPDATE backup_state SET daily_last_date = ?").bind(today).execute(&f.db.writer).await.unwrap();

    // Ohne Planer bleibt der Vorgang angefordert; ein zweiter wird abgewiesen.
    let (s, v) = send(&f, Method::POST, "/api/v1/backups", None).await;
    assert_eq!(s, StatusCode::ACCEPTED, "{v}");
    let id = v["data"]["id"].as_str().unwrap().to_string();
    assert_eq!(v["data"]["kind"], "backup");
    assert_eq!(v["data"]["status"], "queued");
    let (s, v) = send(&f, Method::POST, "/api/v1/backups", None).await;
    assert_eq!(s, StatusCode::CONFLICT);
    assert_eq!(v["error"]["code"], "operation_running");
    assert_eq!(v["error"]["details"]["operation_id"], id.as_str());
    let (_, v) = send(&f, Method::GET, "/api/v1/backups/status", None).await;
    assert_eq!(v["data"]["running_operation_id"], id.as_str());

    let task = tokio::spawn(trackcraft::backup::scheduler(f.cfg.clone(), f.db.clone(), f.runner.clone()));
    let mut done = Value::Null;
    for _ in 0..200 {
        let (_, v) = send(&f, Method::GET, &format!("/api/v1/operations/{id}"), None).await;
        if v["data"]["finished_at"] != Value::Null {
            done = v;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    task.abort();
    let op = &done["data"];
    assert_eq!(op["status"], "done", "{done}");
    assert_eq!(op["outcome"], "created");
    assert_eq!(op["phase"], Value::Null);
    assert_eq!(op["error"], Value::Null);
    assert!(op["backup_id"].as_str().unwrap().starts_with("trackcraft-"));
    let (_, v) = send(&f, Method::GET, "/api/v1/backups/status", None).await;
    assert_eq!(v["data"]["running_operation_id"], Value::Null);
    assert_eq!(v["data"]["last_attempt"]["operation_id"], id.as_str());

    let (s, _) = send(&f, Method::GET, "/api/v1/operations/unbekannt", None).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn interrupted_operation_ends_failed_after_restart() {
    let f = fixture().await;
    sqlx::query(
        "INSERT INTO operations (id, kind, trigger, status, phase, started_at_ms) VALUES ('x', 'backup', 'daily', 'running', 'copying', 1)",
    )
    .execute(&f.db.writer)
    .await
    .unwrap();
    assert_eq!(trackcraft::backup::recover(&f.db).await.unwrap(), 1);
    let op = trackcraft::backup::operation(&f.db, "x").await.unwrap().unwrap();
    assert_eq!((op.status.as_str(), op.outcome.as_deref()), ("failed", Some("failed")));
    assert_eq!(op.error_code.as_deref(), Some(trackcraft::backup::ERROR_INTERRUPTED));
    assert!(op.phase.is_none() && op.finished_at_ms.is_some());
}

#[tokio::test]
async fn missing_original_is_noted_not_fatal() {
    let f = fixture().await;
    let target = f.cfg.raw.backup.target_dir.clone();
    std::fs::create_dir_all(&target).unwrap();
    import(&f, &gpx(T0, 40)).await;
    import(&f, &gpx(T0 + 86_400, 40)).await;
    let rel: String = sqlx::query_scalar("SELECT relative_path FROM original_files ORDER BY relative_path LIMIT 1")
        .fetch_one(&f.db.reader)
        .await
        .unwrap();
    std::fs::remove_file(f.cfg.raw.paths.activities_dir.join(&rel)).unwrap();

    let Outcome::Created { name } = attempt(&f).await else { panic!("Sicherung erwartet") };
    let stand = target.join(&name);
    let m = format::read_manifest(&stand).unwrap();
    format::verify(&stand, &m).unwrap();
    assert_eq!(m.missing_originals, vec![rel]);
    assert_eq!(m.originals(), 1);
    let (_, v) = send(&f, Method::GET, "/api/v1/backups", None).await;
    assert_eq!(v["data"][0]["missing_original_count"], 1);
    assert_eq!(v["data"][0]["check_status"], "ok");
}
