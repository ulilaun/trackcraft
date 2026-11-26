//! Export-Endpunkte (RFC-0019, OpenAPI „Export“): Original und GPX je Aktivität, Gesamtexport als ZIP im Strom.
//! Das Fachdaten-JSON setzt sich aus den Darstellungen der übrigen Endpunkte zusammen, ohne Geheimnisse.

use std::collections::HashSet;
use std::io::{self, Write};

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use chrono::{Datelike, Timelike, Utc};
use serde_json::{Value, json};
use sqlx::Row;
use zip::write::SimpleFileOptions;

use super::AppState;
use super::error::ApiError;
use crate::activities::{self, ListQuery, Sort};
use crate::export::{self, UniqueNames, gpx};
use crate::import_job::timestamp;
use crate::segments::store;

pub const ORIGINAL_MISSING: ApiError =
    ApiError::new(StatusCode::NOT_FOUND, "original_missing", "The original file is missing from the activity archive");
pub const ACTIVITY_WITHOUT_POSITION: ApiError =
    ApiError::new(StatusCode::CONFLICT, "activity_without_position", "The activity has no points with a position");

/// Formatkennung und -version des Fachdaten-JSON.
pub const FORMAT: &str = "trackcraft-export";
pub const FORMAT_VERSION: i64 = 1;
const DOCUMENT_NAME: &str = "trackcraft-export.json";

fn content_type(format: &str) -> &'static str {
    if format == "fit" { "application/vnd.ant.fit" } else { "application/gpx+xml" }
}

fn download(content_type: &str, filename: &str, body: Body) -> Response {
    (
        [
            (header::CONTENT_TYPE, content_type.to_string()),
            (header::CONTENT_DISPOSITION, export::attachment(filename)),
            (header::CACHE_CONTROL, "no-store".to_string()),
        ],
        body,
    )
        .into_response()
}

/// `GET /activities/{id}/original`
pub async fn original(State(state): State<AppState>, Path(id): Path<String>) -> Result<Response, ApiError> {
    let row = sqlx::query(
        "SELECT a.title, a.start_time_ms, o.relative_path, o.format FROM activities a \
         JOIN original_files o ON o.id = a.original_file_id WHERE a.id = ?",
    )
    .bind(&id)
    .fetch_optional(&state.db.reader)
    .await
    .map_err(anyhow::Error::from)?
    .ok_or(ApiError::ACTIVITY_NOT_FOUND)?;
    let format: String = row.get("format");
    let path = crate::import_job::archive_path(&state.cfg, &row.get::<String, _>("relative_path"))?;
    let bytes = match tokio::fs::read(&path).await {
        Ok(b) => b,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Err(ORIGINAL_MISSING),
        Err(e) => return Err(anyhow::Error::from(e).into()),
    };
    let name = format!("{}.{format}", export::file_stem(row.get("start_time_ms"), state.cfg.timezone, &row.get::<String, _>("title")));
    Ok(download(content_type(&format), &name, Body::from(bytes)))
}

/// `GET /activities/{id}/gpx`
pub async fn gpx(State(state): State<AppState>, Path(id): Path<String>) -> Result<Response, ApiError> {
    let row = sqlx::query("SELECT title, description, sport_class, start_time_ms FROM activities WHERE id = ?")
        .bind(&id)
        .fetch_optional(&state.db.reader)
        .await
        .map_err(anyhow::Error::from)?
        .ok_or(ApiError::ACTIVITY_NOT_FOUND)?;
    let mut conn = state.db.reader.acquire().await.map_err(anyhow::Error::from)?;
    let sections = crate::calculation::sections(&mut conn, &id).await?;
    drop(conn);
    let (title, description, sport_class): (String, Option<String>, String) =
        (row.get("title"), row.get("description"), row.get("sport_class"));
    let start: i64 = row.get("start_time_ms");
    let meta = gpx::Meta { title: &title, description: description.as_deref(), sport_class: &sport_class, start_time_ms: start };
    let text = gpx::write(&meta, &sections).ok_or(ACTIVITY_WITHOUT_POSITION)?;
    let name = format!("{}.gpx", export::file_stem(start, state.cfg.timezone, &title));
    Ok(download("application/gpx+xml", &name, Body::from(text)))
}

// ---------------------------------------------------------------------------------------------------------
// Gesamtexport

/// Eine Aktivität mit ihren Dateien im ZIP.
struct Entry {
    id: String,
    title: String,
    description: Option<String>,
    sport_class: String,
    start_time_ms: i64,
    gpx: Option<String>,
}

/// Fachdaten-JSON ohne `missing_originals` und die Liste der Dateien.
async fn document(state: &AppState) -> anyhow::Result<(Value, Vec<Entry>, Vec<String>)> {
    let cfg = &state.cfg;
    let db = &state.db;
    let today = activities::today(cfg);

    let query = ListQuery { sort: Sort::StartAsc, limit: -1, ..Default::default() };
    let (mut list, _, _) = activities::list(cfg, db, &query).await?;

    let originals: Vec<(String, String, String, String, i64)> = sqlx::query(
        "SELECT a.id, o.relative_path, o.format, o.sha256, o.size_bytes FROM activities a JOIN original_files o ON o.id = a.original_file_id",
    )
    .fetch_all(&db.reader)
    .await?
    .iter()
    .map(|r| (r.get("id"), r.get("relative_path"), r.get("format"), r.get("sha256"), r.get("size_bytes")))
    .collect();
    let by_activity: std::collections::HashMap<&str, &(String, String, String, String, i64)> =
        originals.iter().map(|o| (o.0.as_str(), o)).collect();

    let mut names = UniqueNames::default();
    let mut entries = Vec::with_capacity(list.len());
    let mut files: Vec<String> = vec![];
    let mut seen: HashSet<String> = HashSet::new();
    for a in &mut list {
        let id = a["id"].as_str().unwrap_or_default().to_string();
        let title = a["title"].as_str().unwrap_or_default().to_string();
        let start_time_ms = chrono::DateTime::parse_from_rfc3339(a["start_time"].as_str().unwrap_or_default())
            .map(|d| d.timestamp_millis())
            .unwrap_or_default();
        a["original"] = match by_activity.get(id.as_str()) {
            Some(o) => {
                if seen.insert(o.1.clone()) {
                    files.push(o.1.clone());
                }
                json!({ "path": format!("originals/{}", o.1.replace('\\', "/")), "format": o.2, "sha256": o.3, "size_bytes": o.4 })
            }
            None => Value::Null,
        };
        let gpx = a["has_position"]
            .as_bool()
            .unwrap_or(false)
            .then(|| format!("gpx/{}", names.take(&export::file_stem(start_time_ms, cfg.timezone, &title), "gpx")));
        a["gpx"] = json!(gpx);
        entries.push(Entry {
            id,
            title,
            description: a["description"].as_str().map(str::to_string),
            sport_class: a["sport_class"].as_str().unwrap_or("unknown").to_string(),
            start_time_ms,
            gpx,
        });
    }

    let (mut segments, _, _) =
        store::list(cfg, db, &store::ListQuery { q: None, sport_class: None, kind: None, category: None, order: "", limit: -1, offset: 0 })
            .await?;
    for s in &mut segments {
        let id = s["id"].as_str().unwrap_or_default().to_string();
        if let Some(t) = store::track(db, &id, None).await? {
            let d = &t["data"];
            s["reference"] =
                json!({ "point_count": d["point_count"], "lat": d["lat"], "lon": d["lon"], "ele_m": d["ele_m"], "dist_m": d["dist_m"] });
        }
    }

    let efforts: Vec<Value> = sqlx::query(
        "SELECT e.segment_id, e.activity_id, e.attempt_position, e.status, e.not_ranked_reason, e.start_time_ms, e.end_time_ms, \
         e.duration_ms, e.avg_heart_rate_bpm, e.max_heart_rate_bpm FROM segment_efforts e \
         WHERE e.calculation_version_id = (SELECT id FROM calculation_versions WHERE status = 'published') \
         ORDER BY e.segment_id, e.start_time_ms, e.activity_id, e.attempt_position",
    )
    .fetch_all(&db.reader)
    .await?
    .iter()
    .map(|r| {
        json!({
            "segment_id": r.get::<String, _>("segment_id"),
            "activity_id": r.get::<String, _>("activity_id"),
            "attempt_position": r.get::<i64, _>("attempt_position"),
            "status": r.get::<String, _>("status"),
            "not_ranked_reason": r.get::<Option<String>, _>("not_ranked_reason"),
            "start_time": r.get::<Option<i64>, _>("start_time_ms").map(timestamp),
            "end_time": r.get::<Option<i64>, _>("end_time_ms").map(timestamp),
            "duration_ms": r.get::<Option<i64>, _>("duration_ms"),
            "avg_heart_rate_bpm": r.get::<Option<i64>, _>("avg_heart_rate_bpm"),
            "max_heart_rate_bpm": r.get::<Option<i64>, _>("max_heart_rate_bpm"),
        })
    })
    .collect();

    let range: (Option<String>, Option<String>) =
        sqlx::query_as("SELECT min(date), max(date) FROM planned_units").fetch_one(&db.reader).await?;
    let units = match range {
        (Some(from), Some(to)) => crate::plan::list(db, from.parse()?, to.parse()?).await?["units"].take(),
        _ => json!([]),
    };

    // Ohne Geheimnisspeicher: der Export nennt nur, ob ein Schlüssel hinterlegt ist.
    let garmin = crate::garmin::store::load(db, &crate::secrets::Secrets::default()).await?;
    let backup = crate::backup::load_state(db).await?;
    let doc = json!({
        "format": FORMAT,
        "format_version": FORMAT_VERSION,
        "created_at": timestamp(Utc::now().timestamp_millis()),
        "app_version": crate::version::APP_VERSION,
        "schema_version": crate::db::SCHEMA_VERSION,
        "timezone": cfg.timezone.name(),
        "profile": crate::profile::get(db).await?.to_json(),
        "activities": list,
        "segments": segments,
        "segment_efforts": efforts,
        "seasons": crate::seasons::list(db).await?.iter().map(|s| s.to_json(today)).collect::<Vec<_>>(),
        "hr_zone_sets": crate::load::zone_sets(db).await?.iter().map(|s| s.to_json()).collect::<Vec<_>>(),
        "form_settings": crate::load::settings(db).await?.to_json(),
        "plan": {
            "units": units,
            "goals": crate::plan::goals_json(db).await?,
            "templates": crate::plan::templates(db, today).await?,
        },
        "sport_mappings": crate::sports::list_mappings(db).await?.iter().map(|r| r.to_json()).collect::<Vec<_>>(),
        "sub_sports": crate::sports::list_sub_sports(db).await?.iter().map(|r| r.to_json()).collect::<Vec<_>>(),
        "place_settings": crate::places::settings(db).await?,
        // ohne Schlüssel (RFC-0019, #10)
        "garmin": {
            "configured": garmin.api_key.is_some() || garmin.key_unreadable,
            "enabled": garmin.enabled,
            "start_date": garmin.start_date,
            "interval_minutes": garmin.interval_minutes,
        },
        "backup_settings": { "daily_time": backup.daily_time(cfg), "keep": backup.keep() },
    });
    Ok((doc, entries, files))
}

/// `GET /export`: Gesamtexport, im Strom erzeugt.
pub async fn full(State(state): State<AppState>) -> Result<Response, ApiError> {
    let (doc, entries, files) = document(&state).await?;
    let (writer, body, errors) = export::channel();
    let handle = tokio::runtime::Handle::current();
    let st = state.clone();
    let activity_count = entries.len();
    tokio::task::spawn_blocking(move || {
        let started = std::time::Instant::now();
        match produce(&handle, &st, writer, doc, &entries, &files) {
            Ok((bytes, missing)) => tracing::info!(
                activities = activity_count,
                originals = files.len() - missing,
                missing,
                bytes,
                ms = started.elapsed().as_millis() as u64,
                "Gesamtexport geliefert"
            ),
            Err(e) if aborted(&e) => {
                tracing::info!("Gesamtexport vom Browser abgebrochen")
            }
            Err(e) => {
                tracing::warn!(error = %format!("{e:#}"), "Gesamtexport fehlgeschlagen");
                // Der Browser sieht einen abgebrochenen Download statt eines scheinbar vollständigen ZIP.
                let _ = errors.blocking_send(Err(io::Error::other("export failed")));
            }
        }
    });
    let local = Utc::now().with_timezone(&state.cfg.timezone);
    let name = format!("trackcraft-export-{}.zip", local.format("%Y-%m-%d"));
    Ok(download("application/zip", &name, Body::new(body)))
}

/// Abbruch durch den Browser (Kanal geschlossen), auch eingepackt in einen ZIP-Fehler.
fn aborted(e: &anyhow::Error) -> bool {
    let broken = |io: &io::Error| io.kind() == io::ErrorKind::BrokenPipe;
    e.chain().any(|c| {
        c.downcast_ref::<io::Error>().is_some_and(broken)
            || matches!(c.downcast_ref::<zip::result::ZipError>(), Some(zip::result::ZipError::Io(io)) if broken(io))
    })
}

/// Schreibt das ZIP; liefert geschriebene Bytes und Zahl der fehlenden Originale.
fn produce(
    handle: &tokio::runtime::Handle,
    state: &AppState,
    writer: export::ChunkWriter,
    mut doc: Value,
    entries: &[Entry],
    files: &[String],
) -> anyhow::Result<(u64, usize)> {
    let now = Utc::now().with_timezone(&state.cfg.timezone).naive_local();
    let mtime = zip::DateTime::from_date_and_time(
        now.year().clamp(1980, 2107) as u16,
        now.month() as u8,
        now.day() as u8,
        now.hour() as u8,
        now.minute() as u8,
        now.second() as u8,
    )
    .unwrap_or_default();
    let deflate = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated).last_modified_time(mtime);
    let mut zip = zip::ZipWriter::new_stream(writer);

    let mut missing = vec![];
    for rel in files {
        let path = crate::import_job::archive_path(&state.cfg, rel)?;
        match std::fs::File::open(&path) {
            Ok(mut f) => {
                zip.start_file(
                    format!("originals/{}", rel.replace('\\', "/")),
                    deflate.large_file(f.metadata()?.len() >= u32::MAX as u64),
                )?;
                io::copy(&mut f, &mut zip)?;
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => missing.push(rel.clone()),
            Err(e) => return Err(e.into()),
        }
    }
    for e in entries {
        let Some(name) = &e.gpx else { continue };
        let sections = handle.block_on(async {
            let mut conn = state.db.reader.acquire().await?;
            crate::calculation::sections(&mut conn, &e.id).await
        })?;
        let meta = gpx::Meta {
            title: &e.title,
            description: e.description.as_deref(),
            sport_class: &e.sport_class,
            start_time_ms: e.start_time_ms,
        };
        if let Some(text) = gpx::write(&meta, &sections) {
            zip.start_file(name.as_str(), deflate)?;
            zip.write_all(text.as_bytes())?;
        }
    }
    doc["missing_originals"] = json!(missing);
    zip.start_file(DOCUMENT_NAME, deflate)?;
    serde_json::to_writer_pretty(&mut zip, &doc)?;
    let mut w = zip.finish()?.into_inner();
    w.flush()?;
    Ok((w.sent_bytes(), missing.len()))
}
