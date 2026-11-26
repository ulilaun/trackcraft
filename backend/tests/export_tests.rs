//! Export (RFC-0019, #58): Original und GPX je Aktivität, Gesamtexport als ZIP im Strom mit Fachdaten-JSON
//! ohne Geheimnisse und fehlenden Originalen.

mod common;

use std::io::{Cursor, Read};
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use serde_json::{Value, json};
use trackcraft::api::{self, AppState};
use trackcraft::config::{self, Validated};
use trackcraft::db::Db;
use trackcraft::{jobs, startup};

const HOST: &str = "trackcraft.test";
const ORIGIN: &str = "https://trackcraft.test";
const T0: i64 = 1_782_892_800; // 2026-07-01T08:00:00Z
const SECRET: &str = "geheimer-garmin-schluessel-4711";

struct Fixture {
    _dir: tempfile::TempDir,
    cfg: Arc<Validated>,
    app: Router,
    db: Db,
}

async fn fixture() -> Fixture {
    let dir = common::temp();
    let text = common::config_text(dir.path(), "", "");
    let cfg = config::parse(&dir.path().join("t.toml"), &text).unwrap();
    let started = startup::prepare(&cfg).await.unwrap();
    let db = started.db.clone();
    drop(started);
    let cfg = Arc::new(cfg);
    let app = api::router(AppState { cfg: cfg.clone(), db: db.clone(), garmin: Default::default(), backup: Default::default() });
    Fixture { _dir: dir, cfg, app, db }
}

struct Raw {
    status: StatusCode,
    content_type: String,
    disposition: String,
    bytes: Vec<u8>,
}

async fn raw(f: &Fixture, path: &str) -> Raw {
    let req = Request::builder().uri(path).header("host", HOST).body(Body::empty()).unwrap();
    let (status, headers, bytes) = common::contract::call(&f.app, req).await;
    let h = |n: header::HeaderName| headers.get(n).map(|v| v.to_str().unwrap().to_string()).unwrap_or_default();
    let (content_type, disposition) = (h(header::CONTENT_TYPE), h(header::CONTENT_DISPOSITION));
    let bytes = bytes.to_vec();
    Raw { status, content_type, disposition, bytes }
}

async fn send(app: &Router, req: Request<Body>) -> (StatusCode, Value) {
    common::contract::call_json(app, req).await
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

/// GPX mit `n` Punkten im Abstand von 10 s, Herzfrequenz 120 + i.
fn gpx(name: &str, kind: &str, start: i64, n: usize) -> Vec<u8> {
    let mut s = format!(
        "<?xml version=\"1.0\"?>\n<gpx version=\"1.1\" creator=\"test\" xmlns=\"http://www.topografix.com/GPX/1/1\" \
         xmlns:gpxtpx=\"http://www.garmin.com/xmlschemas/TrackPointExtension/v1\">\n<trk><name>{name}</name><type>{kind}</type><trkseg>\n"
    );
    for i in 0..n {
        let t = chrono::DateTime::from_timestamp(start + i as i64 * 10, 0).unwrap();
        s.push_str(&format!(
            "<trkpt lat=\"{:.6}\" lon=\"11.000000\"><ele>{}</ele><time>{}</time>\
             <extensions><gpxtpx:TrackPointExtension><gpxtpx:hr>{}</gpxtpx:hr></gpxtpx:TrackPointExtension></extensions></trkpt>\n",
            47.0 + i as f64 * 0.0003,
            500 + i,
            t.format("%Y-%m-%dT%H:%M:%SZ"),
            120 + i,
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

fn unzip(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut z = zip::ZipArchive::new(Cursor::new(bytes)).expect("gültiges ZIP");
    (0..z.len())
        .map(|i| {
            let mut e = z.by_index(i).unwrap();
            let mut b = vec![];
            e.read_to_end(&mut b).unwrap();
            (e.name().unwrap().to_string(), b)
        })
        .collect()
}

#[tokio::test]
async fn single_downloads_and_full_export() {
    let f = fixture().await;
    let a_bytes = gpx("Morgenrunde am Ölberg", "cycling", T0, 60);
    let a = import(&f, "a.gpx", &a_bytes).await;
    let b = import(&f, "b.gpx", &gpx("Morgenrunde am Ölberg", "cycling", T0 + 86_400, 40)).await;
    jobs::run_all(&f.cfg, &f.db).await.unwrap();
    let (s, v) = with_body(
        &f,
        Method::POST,
        "/api/v1/segments/from-activity",
        json!({ "activity_id": a, "section_index": 0, "start_index": 5, "end_index": 25, "name": "Anstieg", "sport_class": "cycling" }),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    let segment = v["data"]["id"].as_str().unwrap().to_string();
    jobs::run_all(&f.cfg, &f.db).await.unwrap();
    let n = sqlx::query("UPDATE garmin_settings SET api_key = ?, enabled = 1").bind(SECRET).execute(&f.db.writer).await.unwrap();
    assert_eq!(n.rows_affected(), 1);

    // Original Byte für Byte, Name aus Ortszeit und Titel
    let r = raw(&f, &format!("/api/v1/activities/{a}/original")).await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.bytes, a_bytes);
    assert_eq!(r.content_type, "application/gpx+xml");
    assert!(r.disposition.starts_with("attachment; filename=\"2026-07-01-"), "{}", r.disposition);
    assert!(r.disposition.ends_with("-Morgenrunde-am-Oelberg.gpx\""), "{}", r.disposition);

    // GPX aus den aufbereiteten Punkten
    let r = raw(&f, &format!("/api/v1/activities/{a}/gpx")).await;
    assert_eq!(r.status, StatusCode::OK);
    let text = String::from_utf8(r.bytes).unwrap();
    assert_eq!(text.matches("<trkpt ").count(), 60);
    assert!(text.contains("<name>Morgenrunde am Ölberg</name>"));
    assert!(text.contains("<gpxtpx:hr>120</gpxtpx:hr>"));
    assert!(text.contains("<type>cycling</type>"));

    // Unbekannte Aktivität
    let missing_id = "01900000-0000-7000-8000-000000000000";
    for p in ["original", "gpx"] {
        let r = raw(&f, &format!("/api/v1/activities/{missing_id}/{p}")).await;
        assert_eq!(r.status, StatusCode::NOT_FOUND);
    }

    // Gesamtexport
    let r = raw(&f, "/api/v1/export").await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.content_type, "application/zip");
    assert!(r.disposition.starts_with("attachment; filename=\"trackcraft-export-20"), "{}", r.disposition);
    let entries = unzip(&r.bytes);
    let names: Vec<&str> = entries.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names.last(), Some(&"trackcraft-export.json"), "{names:?}");
    assert_eq!(names.iter().filter(|n| n.starts_with("originals/")).count(), 2, "{names:?}");
    let gpx_names: Vec<&&str> = names.iter().filter(|n| n.starts_with("gpx/")).collect();
    assert_eq!(gpx_names.len(), 2, "{names:?}");
    assert_ne!(gpx_names[0], gpx_names[1]);
    for (name, bytes) in &entries {
        assert!(!String::from_utf8_lossy(bytes).contains(SECRET), "Geheimnis in {name}");
    }

    let doc: Value = serde_json::from_slice(&entries.last().unwrap().1).unwrap();
    assert_eq!(doc["format"], "trackcraft-export");
    assert_eq!(doc["format_version"], 1);
    assert_eq!(doc["schema_version"], trackcraft::db::SCHEMA_VERSION);
    let acts = doc["activities"].as_array().unwrap();
    assert_eq!(acts.len(), 2);
    assert_eq!(acts[0]["id"], a.as_str(), "nach Startzeit aufsteigend");
    assert_eq!(acts[1]["id"], b.as_str());
    let orig = acts[0]["original"]["path"].as_str().unwrap();
    let (_, orig_bytes) = entries.iter().find(|(n, _)| n == orig).expect("Original im ZIP");
    assert_eq!(orig_bytes, &a_bytes);
    assert_eq!(acts[0]["original"]["format"], "gpx");
    assert!(names.contains(&acts[1]["gpx"].as_str().unwrap()));
    let seg = doc["segments"].as_array().unwrap();
    assert_eq!(seg.len(), 1);
    assert_eq!(seg[0]["id"], segment.as_str());
    assert_eq!(seg[0]["reference"]["point_count"], 21);
    assert_eq!(seg[0]["reference"]["lat"].as_array().unwrap().len(), 21);
    let efforts = doc["segment_efforts"].as_array().unwrap();
    assert_eq!(efforts.len(), 2, "{efforts:?}");
    assert!(efforts.iter().all(|e| e["segment_id"] == segment.as_str() && e["status"] == "valid"));
    assert_eq!(doc["garmin"], json!({ "configured": true, "enabled": true, "start_date": null, "interval_minutes": 30 }));
    assert!(doc["profile"].is_object());
    assert!(doc["plan"]["units"].as_array().unwrap().is_empty());
    assert!(doc["sport_mappings"].as_array().is_some_and(|m| !m.is_empty()));
    assert!(doc["backup_settings"]["keep"].is_number());
    assert_eq!(doc["missing_originals"], json!([]));

    // Fehlt ein Original im Archiv: Einzeldownload 404, Gesamtexport läuft weiter und vermerkt es
    let rel = orig.strip_prefix("originals/").unwrap();
    std::fs::remove_file(f.cfg.raw.paths.activities_dir.join(rel)).unwrap();
    let r = raw(&f, &format!("/api/v1/activities/{a}/original")).await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
    let v: Value = serde_json::from_slice(&r.bytes).unwrap();
    assert_eq!(v["error"]["code"], "original_missing");
    let r = raw(&f, "/api/v1/export").await;
    assert_eq!(r.status, StatusCode::OK);
    let entries = unzip(&r.bytes);
    assert_eq!(entries.iter().filter(|(n, _)| n.starts_with("originals/")).count(), 1);
    let doc: Value = serde_json::from_slice(&entries.last().unwrap().1).unwrap();
    assert_eq!(doc["missing_originals"], json!([rel]));
    f.db.close().await;
}
