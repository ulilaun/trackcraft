//! Wiederherstellen (#57 Teil 2): Vorschau, Vergleich, Zustimmung, Umschalten per Neustart, Rücknahme, älteres Schema.

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

// ---------------------------------------------------------------------------------------------------------
// Wiederherstellen (#57 Teil 2)

async fn attempt(f: &Fixture) -> Outcome {
    f.runner.attempt(&f.cfg, &f.db, Trigger::Manual).await.expect("kein paralleler Lauf").1
}

async fn backup(f: &Fixture) -> String {
    let Outcome::Created { name } = attempt(f).await else { panic!("Sicherung erwartet") };
    name
}

async fn count(db: &Db, sql: &'static str) -> i64 {
    sqlx::query_scalar(sql).fetch_one(&db.reader).await.unwrap()
}

/// Wartet auf den angeforderten internen Neustart und durchläuft den Start erneut, wie `server::run`.
async fn restart(f: Fixture) -> Fixture {
    tokio::time::timeout(std::time::Duration::from_secs(20), f.runner.restart_requested()).await.expect("Neustart angefordert");
    f.db.close().await;
    let started = startup::prepare(&f.cfg).await.unwrap();
    let db = started.db.clone();
    drop(started);
    let runner = Arc::new(Runner::default());
    let app = api::router(AppState { cfg: f.cfg.clone(), db: db.clone(), garmin: Default::default(), backup: runner.clone() });
    Fixture { _dir: f._dir, cfg: f.cfg, app, db, runner }
}

async fn preview(f: &Fixture, id: &str) -> (StatusCode, Value) {
    send(f, Method::POST, "/api/v1/restores/preview", Some(json!({ "backup_id": id }))).await
}

#[tokio::test]
async fn restore_older_backup_needs_acknowledgement_and_replaces_data() {
    let f = fixture().await;
    std::fs::create_dir_all(&f.cfg.raw.backup.target_dir).unwrap();
    import(&f, &gpx(T0, 40)).await;
    let first = backup(&f).await;
    let first_revision = format::read_manifest(&f.cfg.raw.backup.target_dir.join(&first)).unwrap().data_state.unwrap().revision_id;
    import(&f, &gpx(T0 + 86_400, 40)).await;
    let second = backup(&f).await;
    assert_eq!(count(&f.db, "SELECT count(*) FROM activities").await, 2);

    // Gleicher Stand, älterer Stand
    let (s, v) = preview(&f, &second).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["data"]["comparison"], "same");
    assert_eq!(v["data"]["requires_acknowledgement"], false);
    let (_, v) = preview(&f, &first).await;
    let d = &v["data"];
    assert_eq!(d["comparison"], "older");
    assert_eq!(d["requires_acknowledgement"], true);
    assert_eq!(d["current"]["activity_count"], 2);
    assert_eq!(d["backup"]["activity_count"], 1);
    assert!(d["warnings"].as_array().unwrap().iter().any(|w| w["code"] == "restore_older"));
    let token = d["preview_token"].as_str().unwrap().to_string();

    // Unbekannter Stand, Pfad statt Name
    assert_eq!(preview(&f, "trackcraft-19990101-000000-s1-c1").await.0, StatusCode::NOT_FOUND);
    assert_eq!(preview(&f, "../ziel").await.0, StatusCode::NOT_FOUND);

    // Ohne Zustimmung nicht, mit Zustimmung ja
    let (s, v) = send(&f, Method::POST, "/api/v1/restores", Some(json!({ "preview_token": token }))).await;
    assert_eq!((s, v["error"]["code"].as_str()), (StatusCode::BAD_REQUEST, Some("acknowledgement_required")));
    let (s, v) = send(&f, Method::POST, "/api/v1/restores", Some(json!({ "preview_token": token, "acknowledge_warnings": true }))).await;
    assert_eq!(s, StatusCode::ACCEPTED, "{v}");
    let op = v["data"]["id"].as_str().unwrap().to_string();
    assert_eq!(v["data"]["kind"], "restore");
    // Während des Vorgangs keine Sicherung
    assert_eq!(send(&f, Method::POST, "/api/v1/backups", None).await.0, StatusCode::CONFLICT);

    let f = restart(f).await;
    assert_eq!(count(&f.db, "SELECT count(*) FROM activities").await, 1);
    let originals = std::fs::read_dir(&f.cfg.raw.paths.activities_dir).unwrap().count();
    assert_eq!(originals, 1);
    let (_, v) = send(&f, Method::GET, &format!("/api/v1/operations/{op}"), None).await;
    assert_eq!((v["data"]["status"].as_str(), v["data"]["outcome"].as_str()), (Some("done"), Some("restored")), "{v}");
    // Neue Revision vom gesicherten Stand aus, Sicherungsstatus der laufenden Datenbank erhalten
    let state: (String, String) =
        sqlx::query_as("SELECT revision_id, predecessor_revision_id FROM data_state").fetch_one(&f.db.reader).await.unwrap();
    assert_eq!(state.1, first_revision);
    assert_ne!(state.0, first_revision);
    let (_, v) = send(&f, Method::GET, "/api/v1/backups/status", None).await;
    assert_eq!(v["data"]["last_success"]["backup_id"], second.as_str());
    assert_eq!(v["data"]["pending_changes"], true);
    // Alter Bestand liegt bereit, Merker und Staging sind weg
    let db_dir = &f.cfg.raw.paths.database_dir;
    assert!(db_dir.join("vor-wiederherstellung/database/trackcraft.db").is_file());
    assert_eq!(std::fs::read_dir(db_dir.join("vor-wiederherstellung/activities")).unwrap().count(), 2);
    assert!(!db_dir.join("wiederherstellung.json").exists());
    assert!(std::fs::read_dir(f.cfg.staging_dir()).unwrap().next().is_none());
    // Der übersprungene neuere Stand gehört zu einem anderen Zweig
    let (_, v) = preview(&f, &second).await;
    assert_eq!(v["data"]["comparison"], "not_comparable");
    let audits = count(&f.db, "SELECT count(*) FROM audit_log WHERE action = 'restore' AND outcome = 'ok'").await;
    assert_eq!(audits, 1);
}

#[tokio::test]
async fn newer_backup_and_branches_are_recognised() {
    let f = fixture().await;
    std::fs::create_dir_all(&f.cfg.raw.backup.target_dir).unwrap();
    import(&f, &gpx(T0, 40)).await;
    let first = backup(&f).await;
    let r1: String = sqlx::query_scalar("SELECT revision_id FROM data_state").fetch_one(&f.db.reader).await.unwrap();
    import(&f, &gpx(T0 + 86_400, 40)).await;
    let second = backup(&f).await;
    let r2: String = sqlx::query_scalar("SELECT revision_id FROM data_state").fetch_one(&f.db.reader).await.unwrap();

    // Die laufenden Daten stehen wieder auf r1 (z. B. Datenbankdatei von Hand zurückkopiert): der zweite Stand ist neuer.
    sqlx::query("UPDATE data_state SET revision_id = ?").bind(&r1).execute(&f.db.writer).await.unwrap();
    sqlx::query("DELETE FROM revision_history WHERE revision_id = ?").bind(&r2).execute(&f.db.writer).await.unwrap();
    let (_, v) = preview(&f, &second).await;
    assert_eq!(v["data"]["comparison"], "newer");
    assert_eq!(v["data"]["requires_acknowledgement"], false);
    let (_, v) = preview(&f, &first).await;
    assert_eq!(v["data"]["comparison"], "same");

    // Nach einem Wiederherstellen ist der Bestand ein neuer Zweig: der übersprungene Stand ist nicht vergleichbar.
    sqlx::query("UPDATE data_state SET revision_id = ?").bind(&r2).execute(&f.db.writer).await.unwrap();
    let (_, v) = preview(&f, &first).await;
    let token = v["data"]["preview_token"].as_str().unwrap().to_string();
    let (s, _) = send(&f, Method::POST, "/api/v1/restores", Some(json!({ "preview_token": token, "acknowledge_warnings": true }))).await;
    assert_eq!(s, StatusCode::ACCEPTED);
    let f = restart(f).await;
    let (_, v) = preview(&f, &second).await;
    assert_eq!(v["data"]["comparison"], "not_comparable");
    assert_eq!(v["data"]["requires_acknowledgement"], true);
    let (_, v) = preview(&f, &first).await;
    assert_eq!(v["data"]["comparison"], "older");
}

#[tokio::test]
async fn stale_token_and_damaged_backup_are_refused() {
    let f = fixture().await;
    std::fs::create_dir_all(&f.cfg.raw.backup.target_dir).unwrap();
    import(&f, &gpx(T0, 40)).await;
    let name = backup(&f).await;
    let (_, v) = preview(&f, &name).await;
    let token = v["data"]["preview_token"].as_str().unwrap().to_string();

    // Änderung nach der Vorschau
    import(&f, &gpx(T0 + 86_400, 40)).await;
    let (s, v) = send(&f, Method::POST, "/api/v1/restores", Some(json!({ "preview_token": token, "acknowledge_warnings": true }))).await;
    assert_eq!((s, v["error"]["code"].as_str()), (StatusCode::CONFLICT, Some("restore_preview_stale")));
    let (s, _) = send(&f, Method::POST, "/api/v1/restores", Some(json!({ "preview_token": "kaputt", "acknowledge_warnings": true }))).await;
    assert_eq!(s, StatusCode::CONFLICT);

    // Beschädigtes Original im Stand
    let stand = f.cfg.raw.backup.target_dir.join(&name);
    let m = format::read_manifest(&stand).unwrap();
    let original = m.files.iter().find(|f| f.path.starts_with("originals/")).unwrap();
    std::fs::write(format::safe_join(&stand, &original.path).unwrap(), b"kaputt").unwrap();
    let (s, v) = preview(&f, &name).await;
    assert_eq!((s, v["error"]["code"].as_str()), (StatusCode::BAD_REQUEST, Some("backup_invalid")), "{v}");
}

#[tokio::test]
async fn failed_check_after_switch_rolls_back() {
    let f = fixture().await;
    std::fs::create_dir_all(&f.cfg.raw.backup.target_dir).unwrap();
    import(&f, &gpx(T0, 40)).await;
    let name = backup(&f).await;
    import(&f, &gpx(T0 + 86_400, 40)).await;
    let revision: String = sqlx::query_scalar("SELECT revision_id FROM data_state").fetch_one(&f.db.reader).await.unwrap();
    let (_, v) = preview(&f, &name).await;
    let token = v["data"]["preview_token"].as_str().unwrap().to_string();
    let (s, v) = send(&f, Method::POST, "/api/v1/restores", Some(json!({ "preview_token": token, "acknowledge_warnings": true }))).await;
    assert_eq!(s, StatusCode::ACCEPTED);
    let op = v["data"]["id"].as_str().unwrap().to_string();
    tokio::time::timeout(std::time::Duration::from_secs(20), f.runner.restart_requested()).await.unwrap();

    // Bereitgestelltes Original verschwindet vor dem Umschalten: die Funktionsprüfung muss scheitern.
    let staged = std::fs::read_dir(f.cfg.staging_dir()).unwrap().next().unwrap().unwrap().path();
    for e in std::fs::read_dir(staged.join("originals")).unwrap() {
        std::fs::remove_file(e.unwrap().path()).unwrap();
    }
    let f = restart(f).await;
    assert_eq!(count(&f.db, "SELECT count(*) FROM activities").await, 2);
    assert_eq!(std::fs::read_dir(&f.cfg.raw.paths.activities_dir).unwrap().count(), 2);
    let now: String = sqlx::query_scalar("SELECT revision_id FROM data_state").fetch_one(&f.db.reader).await.unwrap();
    assert_eq!(now, revision);
    let (_, v) = send(&f, Method::GET, &format!("/api/v1/operations/{op}"), None).await;
    assert_eq!(v["data"]["status"], "rolled_back", "{v}");
    assert_eq!(v["data"]["error"]["code"], "restore_failed");
    assert!(!f.cfg.raw.paths.database_dir.join("wiederherstellung.json").exists());
}

#[tokio::test]
async fn backup_with_older_schema_is_migrated_on_restore() {
    use trackcraft::db::MIGRATIONS;
    let f = fixture().await;
    let target = f.cfg.raw.backup.target_dir.clone();
    std::fs::create_dir_all(&target).unwrap();
    import(&f, &gpx(T0, 40)).await;

    // Stand aus einer Datenbank mit Schemaversion 20 (vor Sicherungsstatus und Revisionsverlauf)
    let old_dir = f._dir.path().join("alt");
    std::fs::create_dir_all(&old_dir).unwrap();
    let old = Db::open(&old_dir.join("trackcraft.db")).await.unwrap();
    old.migrate(&MIGRATIONS[..20]).await.unwrap();
    sqlx::query("INSERT INTO data_state (id, dataset_id, revision_id, change_counter, updated_at_ms) VALUES (1, 'alt', 'r0', 0, 0)")
        .execute(&old.writer)
        .await
        .unwrap();
    sqlx::query("INSERT INTO users (id, display_name, created_at_ms, updated_at_ms) VALUES ('u', 'Alt', 0, 0)")
        .execute(&old.writer)
        .await
        .unwrap();
    let snap_dir = f._dir.path().join("stand");
    let mut old_cfg = (*f.cfg).clone();
    old_cfg.raw.paths.activities_dir = old_dir.clone();
    format::snapshot(&old_cfg, &old, &snap_dir, "manual").await.unwrap();
    old.close().await;
    let name = "trackcraft-20200101-000000-s20-c0";
    format::copy_stand(&snap_dir, &target, name).unwrap();

    let (s, v) = preview(&f, name).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["data"]["comparison"], "not_comparable");
    assert!(v["data"]["warnings"].as_array().unwrap().iter().any(|w| w["code"] == "restore_schema_migration"));
    let token = v["data"]["preview_token"].as_str().unwrap().to_string();
    let (s, _) = send(&f, Method::POST, "/api/v1/restores", Some(json!({ "preview_token": token, "acknowledge_warnings": true }))).await;
    assert_eq!(s, StatusCode::ACCEPTED);
    let f = restart(f).await;
    assert_eq!(f.db.schema_version().await.unwrap(), trackcraft::db::SCHEMA_VERSION);
    assert_eq!(count(&f.db, "SELECT count(*) FROM activities").await, 0);
    assert_eq!(count(&f.db, "SELECT count(*) FROM revision_history").await, 2);
}
