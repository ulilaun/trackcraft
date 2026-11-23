//! Aktivitäten ansehen, ändern und löschen (RFC-0017, RFC-0003, OpenAPI `Activity`), Trackdaten (RFC-0003 Abschnitt 11)
//! und Dashboard-Zahlen (RFC-0018). HTTP-unabhängig; die Endpunkte stehen in `api::activities`.

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::Context;
use chrono::{Datelike, NaiveDate, TimeZone};
use serde::{Deserialize, Deserializer};
use serde_json::{Value, json};
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqliteConnection};

use crate::audit::{self, Action};
use crate::calculation;
use crate::config::Validated;
use crate::db::{self, Db, new_id};
use crate::domain::track::{Point, e7_to_deg};
use crate::geo::distance_m;
use crate::import_job::timestamp;
use crate::localtime::{self, LocalTimeError, LocalTimeInput};
use crate::sports;

pub const SPORT_CLASSES: &[&str] = &["cycling", "ebike", "running", "skiing", "swimming", "studio", "unknown"];

/// Spalten einer Aktivität samt abgeleiteter Werte; `elevation` ist Dateiwert, sonst Hysterese des veröffentlichten Stands.
pub(crate) const BASE: &str = "SELECT a.id, a.title, a.description, a.location, a.sport_class, a.source_sport, a.sub_sport, \
    a.start_time_ms, a.start_time_original_ms, a.start_time_origin, a.total_time_ms, a.total_time_original_ms, a.total_time_origin, \
    a.distance_m, a.distance_origin, a.elevation_gain_file_m, c.hysteresis_gain_m, \
    coalesce(a.elevation_gain_file_m, c.hysteresis_gain_m) AS elevation, \
    a.avg_heart_rate_bpm, a.heart_rate_origin, a.filtered_outliers_count, a.max_speed_threshold_kmh, \
    (SELECT count(*) FROM track_sections t WHERE t.activity_id = a.id) AS section_count, \
    (SELECT coalesce(sum(t.point_count), 0) FROM track_sections t WHERE t.activity_id = a.id) AS point_count, \
    coalesce(a.has_position, 0) AS has_position, a.session_index, a.processing_status, a.created_at_ms, a.updated_at_ms, a.revision, \
    (SELECT count(*) FROM segment_efforts e WHERE e.activity_id = a.id AND e.status = 'valid' \
     AND e.calculation_version_id = (SELECT id FROM calculation_versions WHERE status = 'published') \
     AND NOT EXISTS (SELECT 1 FROM segment_efforts p WHERE p.segment_id = e.segment_id AND p.status = 'valid' \
       AND p.calculation_version_id = e.calculation_version_id AND p.duration_ms <= e.duration_ms \
       AND (p.start_time_ms, p.activity_id, p.attempt_position) < (e.start_time_ms, e.activity_id, e.attempt_position))) \
    AS segment_success_count, l.trimp AS load_trimp, l.method AS load_method \
    FROM activities a LEFT JOIN activity_calculations c ON c.activity_id = a.id \
    AND c.calculation_version_id = (SELECT id FROM calculation_versions WHERE status = 'published') \
    LEFT JOIN activity_loads l ON l.activity_id = a.id";

fn avg_speed(distance: Option<f64>, total_ms: i64) -> Option<f64> {
    match distance {
        Some(d) if total_ms > 0 => Some(d / (total_ms as f64 / 1000.0) * 3.6),
        _ => None,
    }
}

fn activity_json(r: &SqliteRow) -> Value {
    let distance: Option<f64> = r.get("distance_m");
    let total: i64 = r.get("total_time_ms");
    let file_gain: Option<f64> = r.get("elevation_gain_file_m");
    let hyst: Option<f64> = r.get("hysteresis_gain_m");
    let (gain, gain_origin) = match (file_gain, hyst) {
        (Some(g), _) => (Some(g), Some("file")),
        (None, Some(h)) => (Some(h), Some("hysteresis")),
        _ => (None, None),
    };
    json!({
        "id": r.get::<String, _>("id"),
        "title": r.get::<String, _>("title"),
        "description": r.get::<Option<String>, _>("description"),
        "location": r.get::<Option<String>, _>("location"),
        "sport_class": r.get::<String, _>("sport_class"),
        "source_sport": r.get::<Option<String>, _>("source_sport"),
        "sub_sport": r.get::<Option<String>, _>("sub_sport"),
        "start_time": timestamp(r.get("start_time_ms")),
        "start_time_origin": r.get::<String, _>("start_time_origin"),
        "start_time_original": r.get::<Option<i64>, _>("start_time_original_ms").map(timestamp),
        "total_time_ms": total,
        "total_time_origin": r.get::<String, _>("total_time_origin"),
        "total_time_original_ms": r.get::<Option<i64>, _>("total_time_original_ms"),
        "distance_m": distance,
        "distance_origin": r.get::<Option<String>, _>("distance_origin"),
        "elevation_gain_m": gain,
        "elevation_origin": gain_origin,
        "avg_speed_kmh": avg_speed(distance, total),
        "avg_heart_rate_bpm": r.get::<Option<i64>, _>("avg_heart_rate_bpm"),
        "heart_rate_origin": r.get::<Option<String>, _>("heart_rate_origin"),
        "filtered_outliers_count": r.get::<i64, _>("filtered_outliers_count"),
        "max_speed_threshold_kmh": r.get::<Option<f64>, _>("max_speed_threshold_kmh"),
        "section_count": r.get::<i64, _>("section_count"),
        "point_count": r.get::<i64, _>("point_count"),
        "has_position": r.get::<i64, _>("has_position") == 1,
        "session_index": r.get::<i64, _>("session_index"),
        "processing_status": r.get::<String, _>("processing_status"),
        // Erste Bestzeit oder Verbesserung, abgeleitet (RFC-0009 „Historische PRs“)
        "segment_success_count": r.get::<i64, _>("segment_success_count"),
        // Belastung (E7), abgeleitet; `null`, solange nicht berechnet
        "load": r.get::<Option<f64>, _>("load_trimp").map(|v| (v * 10.0).round() / 10.0),
        "load_method": r.get::<Option<String>, _>("load_method"),
        "created_at": timestamp(r.get("created_at_ms")),
        "updated_at": timestamp(r.get("updated_at_ms")),
        "revision": r.get::<i64, _>("revision"),
    })
}

pub async fn get(db: &Db, id: &str) -> anyhow::Result<Option<Value>> {
    let sql = format!("SELECT * FROM ({BASE}) x WHERE x.id = ?");
    let row = sqlx::query(sqlx::AssertSqlSafe(sql)).bind(id).fetch_optional(&db.reader).await?;
    Ok(row.as_ref().map(activity_json))
}

// ---------------------------------------------------------------------------------------------------------
// Liste

#[derive(Debug, Clone, Default)]
pub struct ListQuery {
    pub sport_class: Option<String>,
    pub sub_sport: Option<String>,
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
    pub q: Option<String>,
    pub route_id: Option<String>,
    pub sort: Sort,
    pub limit: i64,
    pub offset: i64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Sort {
    #[default]
    StartDesc,
    StartAsc,
    DistanceDesc,
    DistanceAsc,
    TimeDesc,
    TimeAsc,
    ElevationDesc,
    ElevationAsc,
    LoadDesc,
    LoadAsc,
}

impl Sort {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "-start_time" => Self::StartDesc,
            "start_time" => Self::StartAsc,
            "-distance_m" => Self::DistanceDesc,
            "distance_m" => Self::DistanceAsc,
            "-total_time_ms" => Self::TimeDesc,
            "total_time_ms" => Self::TimeAsc,
            "-elevation_gain_m" => Self::ElevationDesc,
            "elevation_gain_m" => Self::ElevationAsc,
            "-load" => Self::LoadDesc,
            "load" => Self::LoadAsc,
            _ => return None,
        })
    }

    /// Unbekannte Werte stehen immer am Ende; Gleichstände stabil nach ID (RFC-0003 Abschnitt 4).
    fn order_by(self) -> &'static str {
        match self {
            Self::StartDesc => "x.start_time_ms DESC, x.id DESC",
            Self::StartAsc => "x.start_time_ms ASC, x.id ASC",
            Self::DistanceDesc => "x.distance_m IS NULL, x.distance_m DESC, x.id DESC",
            Self::DistanceAsc => "x.distance_m IS NULL, x.distance_m ASC, x.id ASC",
            Self::TimeDesc => "x.total_time_ms DESC, x.id DESC",
            Self::TimeAsc => "x.total_time_ms ASC, x.id ASC",
            Self::ElevationDesc => "x.elevation IS NULL, x.elevation DESC, x.id DESC",
            Self::ElevationAsc => "x.elevation IS NULL, x.elevation ASC, x.id ASC",
            Self::LoadDesc => "x.load_trimp IS NULL, x.load_trimp DESC, x.id DESC",
            Self::LoadAsc => "x.load_trimp IS NULL, x.load_trimp ASC, x.id ASC",
        }
    }
}

enum Bind {
    S(String),
    I(i64),
    Null,
}

/// Beginn eines lokalen Kalendertags in UTC-Millisekunden.
pub fn local_day_start(day: NaiveDate, tz: chrono_tz::Tz) -> i64 {
    let midnight = day.and_hms_opt(0, 0, 0).expect("Mitternacht");
    match tz.from_local_datetime(&midnight) {
        chrono::LocalResult::Single(d) | chrono::LocalResult::Ambiguous(d, _) => d.timestamp_millis(),
        // Zeitumstellung um Mitternacht: erste gültige Zeit danach
        chrono::LocalResult::None => tz
            .from_local_datetime(&(midnight + chrono::Duration::hours(1)))
            .earliest()
            .map(|d| d.timestamp_millis())
            .unwrap_or_else(|| midnight.and_utc().timestamp_millis()),
    }
}

fn filters(q: &ListQuery, tz: chrono_tz::Tz) -> (String, Vec<Bind>) {
    let mut w = vec!["1 = 1".to_string()];
    let mut b = vec![];
    if let Some(c) = &q.sport_class {
        w.push("x.sport_class = ?".into());
        b.push(Bind::S(c.clone()));
    }
    if let Some(s) = &q.sub_sport {
        w.push("x.sub_sport = ?".into());
        b.push(Bind::S(s.clone()));
    }
    if let Some(d) = q.from {
        w.push("x.start_time_ms >= ?".into());
        b.push(Bind::I(local_day_start(d, tz)));
    }
    if let Some(d) = q.to {
        w.push("x.start_time_ms < ?".into());
        b.push(Bind::I(local_day_start(d + chrono::Duration::days(1), tz)));
    }
    if let Some(text) = &q.q {
        // LIKE ist in SQLite nur für ASCII unabhängig von Groß-/Kleinschreibung.
        let pattern = format!("%{}%", text.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_"));
        w.push(
            "(x.title LIKE ? ESCAPE '\\' OR coalesce(x.description, '') LIKE ? ESCAPE '\\' OR coalesce(x.location, '') LIKE ? ESCAPE '\\')"
                .into(),
        );
        for _ in 0..3 {
            b.push(Bind::S(pattern.clone()));
        }
    }
    if let Some(r) = &q.route_id {
        w.push(
            "EXISTS (SELECT 1 FROM segment_efforts e JOIN segments s ON s.id = e.segment_id \
             WHERE e.activity_id = x.id AND e.segment_id = ? AND s.kind = 'route' AND e.status = 'valid' \
             AND e.calculation_version_id = (SELECT id FROM calculation_versions WHERE status = 'published'))"
                .into(),
        );
        b.push(Bind::S(r.clone()));
    }
    (w.join(" AND "), b)
}

macro_rules! bind_all {
    ($q:expr, $binds:expr) => {{
        let mut q = $q;
        for b in $binds {
            q = match b {
                Bind::S(s) => q.bind(s.clone()),
                Bind::I(i) => q.bind(*i),
                Bind::Null => q.bind(Option::<String>::None),
            };
        }
        q
    }};
}

/// Liste, Gesamtzahl und Summen über alle gefilterten Aktivitäten (RFC-0006 „Summenzeile“).
pub async fn list(cfg: &Validated, db: &Db, q: &ListQuery) -> anyhow::Result<(Vec<Value>, i64, Value)> {
    let (where_sql, binds) = filters(q, cfg.timezone);
    let sql = format!("SELECT * FROM ({BASE}) x WHERE {where_sql} ORDER BY {} LIMIT ? OFFSET ?", q.sort.order_by());
    let rows = bind_all!(sqlx::query(sqlx::AssertSqlSafe(sql)), &binds).bind(q.limit).bind(q.offset).fetch_all(&db.reader).await?;

    let sum_sql = format!(
        "SELECT count(*) AS n, sum(x.distance_m) AS dist, sum(x.total_time_ms) AS time, sum(x.elevation) AS gain, \
         coalesce(sum(x.distance_m IS NULL), 0) AS no_dist, coalesce(sum(x.elevation IS NULL), 0) AS no_gain, \
         sum(CASE WHEN x.distance_m IS NOT NULL THEN x.total_time_ms END) AS time_with_dist, \
         count(DISTINCT x.sport_class) AS classes, \
         sum(CASE WHEN x.avg_heart_rate_bpm IS NOT NULL THEN x.avg_heart_rate_bpm * x.total_time_ms END) AS hr_weighted, \
         sum(CASE WHEN x.avg_heart_rate_bpm IS NOT NULL THEN x.total_time_ms END) AS hr_time, \
         coalesce(sum(x.segment_success_count), 0) AS successes, sum(x.load_trimp) AS load \
         FROM ({BASE}) x WHERE {where_sql}"
    );
    let s = bind_all!(sqlx::query(sqlx::AssertSqlSafe(sum_sql)), &binds).fetch_one(&db.reader).await?;
    let n: i64 = s.get("n");
    let dist: Option<f64> = s.get("dist");
    let time_with_dist: Option<i64> = s.get("time_with_dist");
    let hr_weighted: Option<i64> = s.get("hr_weighted");
    let hr_time: Option<i64> = s.get("hr_time");
    let classes: i64 = s.get("classes");
    let summary = json!({
        "count": n,
        "distance_m": dist.unwrap_or(0.0),
        "total_time_ms": s.get::<Option<i64>, _>("time").unwrap_or(0),
        "elevation_gain_m": s.get::<Option<f64>, _>("gain").unwrap_or(0.0),
        "activities_without_distance": s.get::<i64, _>("no_dist"),
        "activities_without_elevation": s.get::<i64, _>("no_gain"),
        "mixed_sport_classes": classes > 1,
        // Schnitt nur über Aktivitäten mit Distanz, bei gemischten Sportklassen ohne Wert
        "avg_speed_kmh": if classes > 1 { None } else { avg_speed(dist, time_with_dist.unwrap_or(0)) },
        "avg_heart_rate_bpm": match (hr_weighted, hr_time) {
            (Some(w), Some(t)) if t > 0 => Some((w as f64 / t as f64).round() as i64),
            _ => None,
        },
        "segment_success_count": s.get::<i64, _>("successes"),
        // Summe der berechneten Belastungen (E7)
        "load": s.get::<Option<f64>, _>("load").map(|v| v.round()),
    });
    Ok((rows.iter().map(activity_json).collect(), n, summary))
}

// ---------------------------------------------------------------------------------------------------------
// Ändern

/// Unterscheidet „Feld fehlt“ (`None`) von „Feld ist null“ (`Some(None)`).
pub fn nullable<'de, D: Deserializer<'de>, T: Deserialize<'de>>(d: D) -> Result<Option<Option<T>>, D::Error> {
    Option::<T>::deserialize(d).map(Some)
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivityUpdate {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default, deserialize_with = "nullable")]
    pub description: Option<Option<String>>,
    #[serde(default, deserialize_with = "nullable")]
    pub location: Option<Option<String>>,
    #[serde(default)]
    pub sport_class: Option<String>,
    #[serde(default, deserialize_with = "nullable")]
    pub sub_sport: Option<Option<String>>,
    #[serde(default)]
    pub start_time: Option<LocalTimeInput>,
    #[serde(default)]
    pub total_time_ms: Option<i64>,
}

#[derive(Debug)]
pub enum UpdateError {
    NotFound,
    Validation { field: &'static str, code: &'static str, message: &'static str },
    LocalTimeAmbiguous,
    Internal(anyhow::Error),
}

impl<E: Into<anyhow::Error>> From<E> for UpdateError {
    fn from(e: E) -> Self {
        Self::Internal(e.into())
    }
}

/// Leere Zeichenkette wird zu `null`, sonst getrimmt.
fn text(v: &Option<Option<String>>) -> Option<Option<String>> {
    v.as_ref().map(|o| o.as_ref().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()))
}

pub async fn update(cfg: &Validated, db: &Db, id: &str, u: &ActivityUpdate) -> Result<(Value, Vec<Value>), UpdateError> {
    let v = |field, code, message| UpdateError::Validation { field, code, message };
    if u.title.is_none()
        && u.description.is_none()
        && u.location.is_none()
        && u.sport_class.is_none()
        && u.sub_sport.is_none()
        && u.start_time.is_none()
        && u.total_time_ms.is_none()
    {
        return Err(v("body", "empty", "At least one field is required"));
    }
    let title = match &u.title {
        Some(t) if t.trim().is_empty() => return Err(v("title", "must_not_be_empty", "Title must not be empty")),
        Some(t) => Some(t.trim().to_string()),
        None => None,
    };
    if let Some(c) = &u.sport_class
        && !SPORT_CLASSES.contains(&c.as_str())
    {
        return Err(v("sport_class", "invalid_value", "Unknown sport class"));
    }
    if u.total_time_ms.is_some_and(|t| t < 0) {
        return Err(v("total_time_ms", "must_not_be_negative", "total_time_ms must be >= 0"));
    }
    let mut notices = vec![];
    let start = match &u.start_time {
        None => None,
        Some(input) => match localtime::to_utc(input, cfg.timezone) {
            Ok(c) => {
                if c.shifted {
                    notices
                        .push(json!({ "code": "local_time_shifted", "message": "Local time did not exist and was moved one hour later" }));
                }
                Some(c.utc_ms)
            }
            Err(LocalTimeError::Ambiguous) => return Err(UpdateError::LocalTimeAmbiguous),
            Err(LocalTimeError::Invalid) => return Err(v("start_time.local", "invalid_format", "Expected YYYY-MM-DDTHH:MM:SS")),
        },
    };

    let now = chrono::Utc::now().timestamp_millis();
    let mut tx = db.writer.begin().await?;
    let cur = sqlx::query(
        "SELECT sport_class, sub_sport, start_time_ms, start_time_original_ms, total_time_ms, total_time_original_ms FROM activities WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(UpdateError::NotFound)?;

    let mut sets: Vec<&str> = vec![];
    let mut binds: Vec<Bind> = vec![];
    for (col, val) in [("title = ?", title.map(Some)), ("description = ?", text(&u.description)), ("location = ?", text(&u.location))] {
        if let Some(val) = val {
            sets.push(col);
            binds.push(match val {
                Some(s) => Bind::S(s),
                None => Bind::Null,
            });
        }
    }

    let old_class: String = cur.get("sport_class");
    let class_changed = u.sport_class.as_ref().is_some_and(|c| *c != old_class);
    if let Some(c) = &u.sport_class {
        sets.push("sport_class = ?");
        binds.push(Bind::S(c.clone()));
    }
    // Untertyp ist ein Schlüssel im Katalog der Sportklasse. Wechselt nur die Klasse, entfällt ein
    // Untertyp, den die neue Klasse nicht kennt.
    let new_class = u.sport_class.clone().unwrap_or_else(|| old_class.clone());
    let cur_sub: Option<String> = cur.get("sub_sport");
    let sub = match text(&u.sub_sport) {
        Some(Some(k)) => {
            if !sports::sub_exists(&mut tx, &new_class, &k).await? {
                return Err(v("sub_sport", "not_in_catalog", "Sub sport is not in the catalog of this sport class"));
            }
            Some(Bind::S(k))
        }
        Some(None) => Some(Bind::Null),
        None => match cur_sub {
            Some(k) if class_changed && !sports::sub_exists(&mut tx, &new_class, &k).await? => Some(Bind::Null),
            _ => None,
        },
    };
    if let Some(b) = sub {
        sets.push("sub_sport = ?");
        binds.push(b);
    }
    // Korrektur von Start und Gesamtzeit: Originalwert bleibt beim ersten Korrigieren erhalten (RFC-0017).
    if let Some(ms) = start {
        let original: Option<i64> = cur.get("start_time_original_ms");
        sets.push("start_time_ms = ?");
        binds.push(Bind::I(ms));
        sets.push("start_time_original_ms = ?");
        binds.push(Bind::I(original.unwrap_or(cur.get("start_time_ms"))));
        sets.push("start_time_origin = 'manual_corrected'");
    }
    if let Some(ms) = u.total_time_ms {
        let original: Option<i64> = cur.get("total_time_original_ms");
        sets.push("total_time_ms = ?");
        binds.push(Bind::I(ms));
        sets.push("total_time_original_ms = ?");
        binds.push(Bind::I(original.unwrap_or(cur.get("total_time_ms"))));
        sets.push("total_time_origin = 'manual_corrected'");
    }
    if class_changed {
        sets.push("processing_status = 'pending'");
    }
    sets.push("revision = revision + 1");
    sets.push("updated_at_ms = ?");
    binds.push(Bind::I(now));

    let sql = format!("UPDATE activities SET {} WHERE id = ?", sets.join(", "));
    let mut q = sqlx::query(sqlx::AssertSqlSafe(sql));
    for b in &binds {
        q = match b {
            Bind::I(i) => q.bind(*i),
            Bind::S(s) => q.bind(s.clone()),
            Bind::Null => q.bind(Option::<String>::None),
        };
    }
    q.bind(id).execute(&mut *tx).await?;

    if class_changed {
        // Wertungen der alten Sportklasse entfallen, die Aktivität wird neu ausgewertet (RFC-0008 Abschnitt 4).
        sqlx::query("DELETE FROM segment_efforts WHERE activity_id = ?").bind(id).execute(&mut *tx).await?;
        queue_evaluation(&mut tx, id, now).await?;
    }
    db::bump_revision(&mut tx, now).await?;
    tx.commit().await?;
    let view = get(db, id).await?.context("Aktivität fehlt nach dem Ändern")?;
    Ok((view, notices))
}

async fn queue_evaluation(conn: &mut SqliteConnection, activity_id: &str, now: i64) -> sqlx::Result<()> {
    let queued: i64 =
        sqlx::query_scalar("SELECT count(*) FROM jobs WHERE activity_id = ? AND kind = 'activity_evaluation' AND status = 'queued'")
            .bind(activity_id)
            .fetch_one(&mut *conn)
            .await?;
    if queued == 0 {
        sqlx::query("INSERT INTO jobs (id, kind, activity_id, status, total_items, created_at_ms) VALUES (?, 'activity_evaluation', ?, 'queued', 1, ?)")
            .bind(new_id())
            .bind(activity_id)
            .bind(now)
            .execute(conn)
            .await?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------------------
// Löschen

pub async fn deletion_preview(db: &Db, id: &str) -> anyhow::Result<Option<Value>> {
    let Some(file): Option<String> =
        sqlx::query_scalar("SELECT original_file_id FROM activities WHERE id = ?").bind(id).fetch_optional(&db.reader).await?
    else {
        return Ok(None);
    };
    let efforts: i64 =
        sqlx::query_scalar("SELECT count(*) FROM segment_efforts WHERE activity_id = ?").bind(id).fetch_one(&db.reader).await?;
    let segments: i64 = sqlx::query_scalar("SELECT count(DISTINCT segment_id) FROM segment_efforts WHERE activity_id = ?")
        .bind(id)
        .fetch_one(&db.reader)
        .await?;
    let others: i64 = sqlx::query_scalar("SELECT count(*) FROM activities WHERE original_file_id = ? AND id <> ?")
        .bind(&file)
        .bind(id)
        .fetch_one(&db.reader)
        .await?;
    Ok(Some(json!({
        "object_type": "activity",
        "effort_count": efforts,
        "affected_segment_count": segments,
        "original_file_removed": others == 0,
    })))
}

/// Löscht die Aktivität; `false`, wenn es sie nicht gibt. Das Original entfällt nur ohne weitere Verweise (ADR-0011).
pub async fn delete(cfg: &Validated, db: &Db, id: &str) -> anyhow::Result<bool> {
    let now = chrono::Utc::now().timestamp_millis();
    let mut tx = db.writer.begin().await?;
    let Some(row) =
        sqlx::query("SELECT original_file_id, title, start_time_ms FROM activities WHERE id = ?").bind(id).fetch_optional(&mut *tx).await?
    else {
        return Ok(false);
    };
    let file: String = row.get("original_file_id");
    let segments: Vec<String> =
        sqlx::query_scalar("SELECT DISTINCT segment_id FROM segment_efforts WHERE activity_id = ?").bind(id).fetch_all(&mut *tx).await?;
    sqlx::query("DELETE FROM activities WHERE id = ?").bind(id).execute(&mut *tx).await?;

    let mut removed: Option<PathBuf> = None;
    let refs: i64 =
        sqlx::query_scalar("SELECT count(*) FROM activities WHERE original_file_id = ?").bind(&file).fetch_one(&mut *tx).await?;
    if refs == 0 {
        let rel: String =
            sqlx::query_scalar("SELECT relative_path FROM original_files WHERE id = ?").bind(&file).fetch_one(&mut *tx).await?;
        sqlx::query("DELETE FROM original_files WHERE id = ?").bind(&file).execute(&mut *tx).await?;
        removed = Some(crate::import_job::archive_path(cfg, &rel)?);
    }
    // Ranglisten der betroffenen Segmente neu berechnen (M3)
    for s in &segments {
        sqlx::query(
            "INSERT INTO jobs (id, kind, segment_id, status, total_items, created_at_ms) VALUES (?, 'segment_history', ?, 'queued', 1, ?)",
        )
        .bind(new_id())
        .bind(s)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    }
    audit::record(
        &mut tx,
        Action::Delete,
        "activity",
        Some(id),
        true,
        json!({ "title": row.get::<String, _>("title"), "start_time": timestamp(row.get("start_time_ms")), "original_file_removed": refs == 0 }),
    )
    .await?;
    db::bump_revision(&mut tx, now).await?;
    tx.commit().await?;

    if let Some(path) = removed
        && let Err(e) = std::fs::remove_file(&path)
    {
        tracing::warn!(path = %path.display(), error = %e, "Nicht mehr referenziertes Original lässt sich nicht entfernen");
    }
    tracing::info!(id, "Aktivität gelöscht");
    Ok(true)
}

// ---------------------------------------------------------------------------------------------------------
// Trackdaten und Versuche

/// Gleichmäßig nach Index ausgewählte Positionen; erster und letzter Punkt bleiben (RFC-0003 Abschnitt 11).
pub fn thin_indices(n: usize, max: Option<usize>) -> Vec<usize> {
    match max {
        Some(m) if m >= 2 && n > m => {
            let mut out: Vec<usize> = (0..m).map(|i| ((i as f64) * (n - 1) as f64 / (m - 1) as f64).round() as usize).collect();
            out.dedup();
            out
        }
        _ => (0..n).collect(),
    }
}

fn section_json(index: usize, points: &[Point], max: Option<usize>) -> Value {
    // Kumulierte Strecke über alle Punkte, ausgedünnt wird erst danach.
    let mut dist: Vec<Option<f64>> = Vec::with_capacity(points.len());
    let mut acc = 0.0;
    let mut prev: Option<(i32, i32)> = None;
    for p in points {
        match p.pos {
            Some(pos) => {
                if let Some(q) = prev {
                    acc += distance_m(q, pos);
                }
                prev = Some(pos);
                dist.push(Some(acc));
            }
            None => dist.push(None),
        }
    }
    let idx = thin_indices(points.len(), max);
    let pick = |f: &dyn Fn(&Point) -> Value| idx.iter().map(|&i| f(&points[i])).collect::<Vec<_>>();
    json!({
        "section_index": index,
        "point_count": points.len(),
        "returned_point_count": idx.len(),
        "simplified": idx.len() < points.len(),
        "index": idx,
        "t_ms": pick(&|p| json!(p.t_ms)),
        "lat": pick(&|p| json!(p.pos.map(|(la, _)| e7_to_deg(la)))),
        "lon": pick(&|p| json!(p.pos.map(|(_, lo)| e7_to_deg(lo)))),
        "ele_m": pick(&|p| json!(p.ele_dm.map(|d| d as f64 / 10.0))),
        "hr": pick(&|p| json!(p.hr)),
        "dist_m": idx.iter().map(|&i| json!(dist[i])).collect::<Vec<_>>(),
    })
}

pub async fn track(db: &Db, id: &str, section: Option<usize>, max_points: Option<usize>) -> anyhow::Result<Option<Value>> {
    let mut conn = db.reader.acquire().await?;
    let exists: Option<i64> = sqlx::query_scalar("SELECT 1 FROM activities WHERE id = ?").bind(id).fetch_optional(&mut *conn).await?;
    if exists.is_none() {
        return Ok(None);
    }
    let sections = calculation::sections(&mut conn, id).await?;
    let out: Vec<Value> = sections
        .iter()
        .enumerate()
        .filter(|(i, _)| section.is_none_or(|s| s == *i))
        .map(|(i, pts)| section_json(i, pts, max_points))
        .collect();
    Ok(Some(json!({ "data": { "sections": out } })))
}

/// Versuche der Aktivität im veröffentlichten Stand mit abgeleitetem Rang und Bestzeitenart (RFC-0009).
pub async fn efforts(cfg: &Validated, db: &Db, id: &str) -> anyhow::Result<Option<Vec<Value>>> {
    let exists: Option<String> = sqlx::query_scalar("SELECT id FROM activities WHERE id = ?").bind(id).fetch_optional(&db.reader).await?;
    if exists.is_none() {
        return Ok(None);
    }
    Ok(Some(crate::segments::store::activity_efforts(cfg, db, id).await?))
}

// ---------------------------------------------------------------------------------------------------------
// Dashboard (RFC-0018)

#[derive(Default, Clone, Copy)]
struct Acc {
    count: i64,
    distance: f64,
    time: i64,
    gain: f64,
    no_distance: i64,
    no_gain: i64,
}

impl Acc {
    fn add(&mut self, distance: Option<f64>, time: i64, gain: Option<f64>) {
        self.count += 1;
        self.time += time;
        match distance {
            Some(d) => self.distance += d,
            None => self.no_distance += 1,
        }
        match gain {
            Some(g) => self.gain += g,
            None => self.no_gain += 1,
        }
    }
}

struct Item {
    day: NaiveDate,
    class: String,
    distance: Option<f64>,
    time: i64,
    gain: Option<f64>,
}

fn block(period: String, from: NaiveDate, to: NaiveDate, items: &[Item]) -> Value {
    let inside: Vec<&Item> = items.iter().filter(|i| i.day >= from && i.day <= to).collect();
    let mut summaries = vec![];
    for class in SPORT_CLASSES.iter().copied().chain(["all"]) {
        let mut a = Acc::default();
        for i in inside.iter().filter(|i| class == "all" || i.class == class) {
            a.add(i.distance, i.time, i.gain);
        }
        summaries.push(json!({
            "period": period, "sport_class": class, "count": a.count, "distance_m": a.distance, "total_time_ms": a.time,
            "elevation_gain_m": a.gain, "activities_without_distance": a.no_distance, "activities_without_elevation": a.no_gain,
        }));
    }
    // Ein Eintrag je Tag und Sportklasse mit Aktivität; Tage ohne Aktivität fehlen (RFC-0018).
    let mut days: BTreeMap<(NaiveDate, usize), Acc> = BTreeMap::new();
    for i in &inside {
        let order = SPORT_CLASSES.iter().position(|c| *c == i.class).unwrap_or(SPORT_CLASSES.len());
        days.entry((i.day, order)).or_default().add(i.distance, i.time, i.gain);
    }
    let by_day: Vec<Value> = days
        .iter()
        .map(|((day, order), a)| {
            json!({
                "date": day.to_string(), "sport_class": SPORT_CLASSES.get(*order).copied().unwrap_or("unknown"),
                "count": a.count, "distance_m": a.distance, "total_time_ms": a.time,
            })
        })
        .collect();
    json!({ "period": period, "from": from.to_string(), "to": to.to_string(), "summaries": summaries, "by_day": by_day })
}

/// Laufende ISO-Woche, laufender Monat und laufende (sonst zuletzt beendete) Saison, bezogen auf den lokalen Tag `today`.
pub async fn dashboard(cfg: &Validated, db: &Db, today: NaiveDate) -> anyhow::Result<Value> {
    let tz = cfg.timezone;
    let week_from = today - chrono::Duration::days(today.weekday().num_days_from_monday() as i64);
    let week_to = week_from + chrono::Duration::days(6);
    let month_from = today.with_day(1).expect("erster Tag");
    let next_month = if month_from.month() == 12 {
        NaiveDate::from_ymd_opt(month_from.year() + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(month_from.year(), month_from.month() + 1, 1)
    }
    .expect("Folgemonat");
    let month_to = next_month - chrono::Duration::days(1);
    // Laufendes Kalenderjahr statt Saison (Uli 23.11.2026); umfasst Woche und Monat außer in der ersten Januarwoche
    let year_from = NaiveDate::from_ymd_opt(today.year(), 1, 1).expect("1. Januar");
    let year_to = NaiveDate::from_ymd_opt(today.year(), 12, 31).expect("31. Dezember");
    let from = year_from.min(week_from);
    let to = year_to.max(week_to);

    let sql = format!(
        "SELECT x.sport_class, x.start_time_ms, x.distance_m, x.total_time_ms, x.elevation FROM ({BASE}) x WHERE x.start_time_ms >= ? AND x.start_time_ms < ?"
    );
    let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(local_day_start(from, tz))
        .bind(local_day_start(to + chrono::Duration::days(1), tz))
        .fetch_all(&db.reader)
        .await?;
    let items: Vec<Item> = rows
        .iter()
        .filter_map(|r| {
            let start: i64 = r.get("start_time_ms");
            let day = tz.timestamp_millis_opt(start).single()?.date_naive();
            Some(Item {
                day,
                class: r.get("sport_class"),
                distance: r.get("distance_m"),
                time: r.get("total_time_ms"),
                gain: r.get("elevation"),
            })
        })
        .collect();
    let iso = today.iso_week();
    Ok(json!({
        "week": block(format!("{}-W{:02}", iso.year(), iso.week()), week_from, week_to, &items),
        "month": block(format!("{}-{:02}", month_from.year(), month_from.month()), month_from, month_to, &items),
        // Summen des laufenden Kalenderjahres; die Seite zeigt nur die Summen
        "year": block(today.year().to_string(), year_from, year_to, &items),
    }))
}

/// Heutiger Kalendertag in der Zeitzone der Konfiguration.
pub fn today(cfg: &Validated) -> NaiveDate {
    chrono::Utc::now().with_timezone(&cfg.timezone).date_naive()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thinning_keeps_first_and_last() {
        assert_eq!(thin_indices(5, None), vec![0, 1, 2, 3, 4]);
        assert_eq!(thin_indices(5, Some(3)), vec![0, 2, 4]);
        assert_eq!(thin_indices(3, Some(10)), vec![0, 1, 2]);
        let t = thin_indices(24_122, Some(2000));
        assert_eq!(t.len(), 2000);
        assert_eq!((t[0], *t.last().unwrap()), (0, 24_121));
    }

    #[test]
    fn local_midnight_in_berlin() {
        let d = NaiveDate::from_ymd_opt(2026, 7, 1).unwrap();
        // 00:00 MESZ = 22:00 UTC am Vortag
        assert_eq!(local_day_start(d, chrono_tz::Europe::Berlin), 1_782_856_800_000);
    }

    #[test]
    fn sort_values() {
        assert_eq!(Sort::parse("-elevation_gain_m"), Some(Sort::ElevationDesc));
        assert_eq!(Sort::parse("title"), None);
    }
}
