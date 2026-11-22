//! Privater Testkorpus durch den ganzen Import (M1b): jede Datei hochladen, alle wählbaren Sessions speichern,
//! danach jeden gespeicherten Abschnitt dekodieren. Läuft nur mit `TRACKCRAFT_TEST_CORPUS` (siehe corpus_tests).

mod common;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::json;
use sqlx::Row;
use trackcraft::db::new_id;
use trackcraft::domain::track;
use trackcraft::import_job::{self, CreateError, ImportDecision};
use trackcraft::startup;

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().path()).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            walk(&p, out);
        } else if matches!(p.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref(), Some("fit" | "gpx")) {
            out.push(p);
        }
    }
}

#[tokio::test]
async fn whole_corpus_imports() {
    let Ok(root) = std::env::var("TRACKCRAFT_TEST_CORPUS") else {
        eprintln!("TRACKCRAFT_TEST_CORPUS nicht gesetzt, Korpus-Import übersprungen");
        return;
    };
    let mut files = vec![];
    walk(Path::new(&root), &mut files);
    assert!(!files.is_empty());

    let dir = common::temp();
    let cfg = Arc::new(common::validated(dir.path()));
    let started = startup::prepare(&cfg).await.unwrap();
    let db = started.db.clone();

    let (mut rejected, mut locked, mut imported, mut identical) = (0, 0, 0, 0);
    for path in &files {
        let bytes = std::fs::read(path).unwrap();
        let id = new_id();
        std::fs::create_dir_all(import_job::staging_dir(&cfg, &id)).unwrap();
        std::fs::write(import_job::upload_path(&cfg, &id), &bytes).unwrap();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let job = match import_job::create(&cfg, &db, &id, &name, bytes.len() as i64, &import_job::sha256_hex(&bytes)).await {
            Ok(job) => job,
            Err(CreateError::Rejected(_)) => {
                rejected += 1;
                continue;
            }
            Err(CreateError::Internal(e)) => panic!("{}: {e:#}", path.display()),
        };
        let mut decisions = vec![];
        for s in job["sessions"].as_array().unwrap() {
            let mut d = json!({ "session_index": s["session_index"] });
            match s["state"].as_str().unwrap() {
                "selectable" => {
                    d["action"] = json!("import");
                    if s["time_input_required"] == true {
                        d["start_time"] = json!({ "local": "2026-01-01T10:00:00" });
                        d["total_time_ms"] = json!(3_600_000);
                    }
                }
                "locked_existing" => {
                    identical += 1;
                    d["action"] = json!("skip");
                }
                _ => {
                    locked += 1;
                    d["action"] = json!("skip");
                }
            }
            decisions.push(d);
        }
        let decision: ImportDecision = serde_json::from_value(json!({ "state_token": job["state_token"], "sessions": decisions })).unwrap();
        let done = match import_job::confirm(&cfg, &db, &id, &decision).await {
            Ok(c) => c.job,
            Err(e) => panic!("{}: {e:?}", path.display()),
        };
        assert_eq!(done["status"], "done", "{}: {done}", path.display());
        imported += done["imported_activity_ids"].as_array().map(Vec::len).unwrap_or(0);
    }

    let activities: i64 = sqlx::query_scalar("SELECT count(*) FROM activities").fetch_one(&db.reader).await.unwrap();
    assert_eq!(activities as usize, imported);
    let mut points = 0usize;
    for r in sqlx::query("SELECT point_count, data FROM track_sections").fetch_all(&db.reader).await.unwrap() {
        let n: i64 = r.get("point_count");
        let data: Vec<u8> = r.get("data");
        points += track::decode(&data, Some(n as usize)).unwrap().len();
    }
    let staging = std::fs::read_dir(cfg.staging_dir()).unwrap().count();
    assert_eq!(staging, 0);
    eprintln!(
        "{} Dateien: {imported} Aktivitäten gespeichert ({points} Punkte), {rejected} abgewiesen, {locked} Sessions gesperrt, {identical} identisch",
        files.len()
    );
    db.close().await;
    drop(started);
}
