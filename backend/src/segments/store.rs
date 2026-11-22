//! Segmente und Strecken in der Datenbank (RFC-0016, OpenAPI „Segments“): Anlegen aus einer Aktivität, Lesen
//! mit Zusammenfassung, Ändern, Löschen, Referenz, Rangliste und Versuche.

use std::collections::HashMap;

use chrono::Datelike;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::{Row, SqliteConnection};

use super::evaluation::{RANKED, reference_points};
use super::matching::{Reference, ReferenceError};
use super::ranking::{self, Derived, Valid};
use super::similarity;
use crate::audit::{self, Action};
use crate::config::Validated;
use crate::db::{self, Db, new_id};
use crate::domain::track::{self, Point, e7_to_deg};
use crate::geo;
use crate::import::process;
use crate::import_job::timestamp;

pub const KINDS: [&str; 2] = ["segment", "route"];

/// Kategorien je Art (Uli 12.11.2026): Segment Anstieg/Abfahrt/Wellig, Strecke Rundkurs/Hin & zurück/Strecke.
pub const SEGMENT_CATEGORIES: [&str; 3] = ["climb", "descent", "rolling"];
pub const ROUTE_CATEGORIES: [&str; 3] = ["loop", "out_and_back", "point_to_point"];
/// Ø-Steigung ab der ein Segment Anstieg bzw. Abfahrt ist (Prozent)
const CLIMB_GRADE_PCT: f64 = 2.0;
/// Hin & zurück: so viel der zweiten Hälfte liegt höchstens `OUT_AND_BACK_TOL_M` neben der ersten
const OUT_AND_BACK_SHARE: f64 = 0.7;
const OUT_AND_BACK_TOL_M: f64 = 35.0;
/// Probenabstand für den Vergleich der Hälften
const OUT_AND_BACK_STEP_M: f64 = 25.0;

pub fn categories_for(kind: &str) -> &'static [&'static str] {
    if kind == "route" { &ROUTE_CATEGORIES } else { &SEGMENT_CATEGORIES }
}

/// Automatische Kategorie aus der Referenz; `None` ohne Höhen (Segment) bzw. Positionen.
/// Strecken: Start und Ziel höchstens `loop_radius_m` auseinander gilt als Rundkurs bzw. Hin & zurück
/// (`[segments] route_loop_radius_m`, Uli 14.11.2026: 750 m). Damit kurze gerade Strecken nicht als Rundkurs
/// gelten, zusätzlich höchstens ein Drittel der Länge.
pub fn auto_category(kind: &str, pts: &[Point], length_m: f64, loop_radius_m: f64) -> Option<&'static str> {
    if kind != "route" {
        let g = grade_pct(pts, length_m)?;
        return Some(if g >= CLIMB_GRADE_PCT {
            "climb"
        } else if g <= -CLIMB_GRADE_PCT {
            "descent"
        } else {
            "rolling"
        });
    }
    let pos: Vec<(i32, i32)> = pts.iter().filter_map(|p| p.pos).collect();
    let (first, last) = (*pos.first()?, *pos.last()?);
    if geo::distance_m(first, last) > loop_radius_m.min(length_m / 3.0) {
        return Some("point_to_point");
    }
    // Proben etwa alle 25 m entlang der Referenz, getrennt in erste und zweite Hälfte
    let mut cum = 0.0;
    let mut next = 0.0;
    let mut samples: Vec<(f64, (i32, i32))> = vec![];
    for (i, p) in pos.iter().enumerate() {
        if i > 0 {
            cum += geo::distance_m(pos[i - 1], *p);
        }
        if cum >= next {
            samples.push((cum, *p));
            next = cum + OUT_AND_BACK_STEP_M;
        }
    }
    let half = cum / 2.0;
    let (out, back): (Vec<_>, Vec<_>) = samples.into_iter().partition(|(d, _)| *d < half);
    if back.is_empty() || out.is_empty() {
        return Some("loop");
    }
    let near = back.iter().filter(|(_, b)| out.iter().any(|(_, a)| geo::distance_m(*a, *b) <= OUT_AND_BACK_TOL_M)).count();
    Some(if near as f64 >= OUT_AND_BACK_SHARE * back.len() as f64 { "out_and_back" } else { "loop" })
}

/// Trägt fehlende automatische Kategorien nach (nach Migration 4) und ordnet Strecken neu ein, falls sich
/// `[segments] route_loop_radius_m` geändert hat; Anzahl der geänderten Einträge. Kategorien von Hand bleiben.
pub async fn backfill_categories(db: &Db, loop_radius_m: f64) -> anyhow::Result<u64> {
    let rows = sqlx::query(
        "SELECT id, kind, length_m, reference, reference_point_count, category_auto FROM segments          WHERE category_auto IS NULL OR kind = 'route'",
    )
    .fetch_all(&db.reader)
    .await?;
    let mut n = 0;
    for r in rows {
        let pts = track::decode(&r.get::<Vec<u8>, _>("reference"), Some(r.get::<i64, _>("reference_point_count") as usize))?;
        let Some(c) = auto_category(&r.get::<String, _>("kind"), &pts, r.get("length_m"), loop_radius_m) else { continue };
        if r.get::<Option<String>, _>("category_auto").as_deref() == Some(c) {
            continue;
        }
        sqlx::query("UPDATE segments SET category_auto = ? WHERE id = ?")
            .bind(c)
            .bind(r.get::<String, _>("id"))
            .execute(&db.writer)
            .await?;
        n += 1;
    }
    Ok(n)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Create {
    pub activity_id: String,
    pub section_index: i64,
    pub start_index: i64,
    pub end_index: i64,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub sport_class: String,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub location: Option<String>,
    /// Von Hand gewählte Kategorie; ohne Angabe automatisch
    #[serde(default)]
    pub category: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum CreateError {
    #[error("Aktivität nicht gefunden")]
    ActivityNotFound,
    #[error("{field}: {message}")]
    Validation { field: &'static str, code: &'static str, message: &'static str },
    #[error("unter der Mindestlänge")]
    TooShort,
    #[error("Richtung nicht bestimmbar")]
    Direction,
    #[error("Aktivität passt schon zu einer Strecke")]
    RouteAlreadyMatched,
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl From<sqlx::Error> for CreateError {
    fn from(e: sqlx::Error) -> Self {
        Self::Internal(e.into())
    }
}

fn invalid(field: &'static str, code: &'static str, message: &'static str) -> CreateError {
    CreateError::Validation { field, code, message }
}

fn optional_text(s: &Option<String>) -> Option<String> {
    s.as_ref().map(|t| t.trim().to_string()).filter(|t| !t.is_empty())
}

/// Länge entlang der Punkte mit Position (Oberflächengeometrie ohne Höhe, RFC-0016 „Mindestdaten“).
fn along(pts: &[Point]) -> f64 {
    let pos: Vec<_> = pts.iter().filter_map(|p| p.pos).collect();
    pos.windows(2).map(|w| geo::distance_m(w[0], w[1])).sum()
}

/// Punkte eines Abschnitts der Aktivität; Fehler, wenn Aktivität oder Abschnitt fehlen.
async fn section_points(db: &Db, activity_id: &str, section_index: i64) -> Result<Vec<Point>, CreateError> {
    let mut r = db.reader.acquire().await?;
    let exists: Option<String> =
        sqlx::query_scalar("SELECT id FROM activities WHERE id = ?").bind(activity_id).fetch_optional(&mut *r).await?;
    if exists.is_none() {
        return Err(CreateError::ActivityNotFound);
    }
    let Some(row) = sqlx::query("SELECT point_count, data FROM track_sections WHERE activity_id = ? AND section_index = ?")
        .bind(activity_id)
        .bind(section_index)
        .fetch_optional(&mut *r)
        .await?
    else {
        return Err(invalid("section_index", "not_found", "Section does not exist"));
    };
    let n: i64 = row.get("point_count");
    Ok(track::decode(&row.get::<Vec<u8>, _>("data"), Some(n as usize)).map_err(anyhow::Error::from)?)
}

/// Prüft Start- und Zielindex im Abschnitt.
fn selection(pts: &[Point], start_index: i64, end_index: i64) -> Result<(usize, usize), CreateError> {
    let in_range = |i: i64| i >= 0 && (i as usize) < pts.len();
    if !in_range(start_index) {
        return Err(invalid("start_index", "out_of_range", "Index outside the section"));
    }
    if !in_range(end_index) {
        return Err(invalid("end_index", "out_of_range", "Index outside the section"));
    }
    if start_index >= end_index {
        return Err(invalid("end_index", "must_follow_start", "End must lie after start"));
    }
    let (start, end) = (start_index as usize, end_index as usize);
    if pts[start].pos.is_none() {
        return Err(invalid("start_index", "no_position", "Start point has no GPS position"));
    }
    if pts[end].pos.is_none() {
        return Err(invalid("end_index", "no_position", "End point has no GPS position"));
    }
    Ok((start, end))
}

/// Referenz: Position und Höhe, ohne Zeit und Herzfrequenz (keine Bindung an die Quellaktivität)
fn reference_of(pts: &[Point], start: usize, end: usize) -> Vec<Point> {
    pts[start..=end].iter().filter_map(|p| p.pos.map(|pos| Point { t_ms: None, pos: Some(pos), ele_dm: p.ele_dm, hr: None })).collect()
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SimilarQuery {
    pub activity_id: String,
    pub section_index: i64,
    pub start_index: i64,
    pub end_index: i64,
    pub sport_class: String,
}

/// Strecken gleicher Sportklasse mit fast gleichem Verlauf wie die Auswahl: mindestens 90 % in beide
/// Richtungen innerhalb der Toleranz von `[segments.route_matching]`. Ergebnis nach Überdeckung absteigend.
pub async fn similar_routes(cfg: &Validated, db: &Db, q: &SimilarQuery) -> Result<Vec<Value>, CreateError> {
    if !RANKED.contains(&q.sport_class.as_str()) {
        return Err(invalid("sport_class", "invalid_value", "Sport class must be cycling, ebike, running or skiing"));
    }
    let pts = section_points(db, &q.activity_id, q.section_index).await?;
    let (start, end) = selection(&pts, q.start_index, q.end_index)?;
    let sel: Vec<(i32, i32)> = reference_of(&pts, start, end).iter().filter_map(|p| p.pos).collect();
    let tol = cfg.raw.segments.route_matching.tolerance_m;
    let rows =
        sqlx::query("SELECT id, name, length_m, reference, reference_point_count FROM segments WHERE kind = 'route' AND sport_class = ?")
            .bind(&q.sport_class)
            .fetch_all(&db.reader)
            .await?;
    let mut found: Vec<(f64, Value)> = vec![];
    for r in rows {
        let refp = track::decode(&r.get::<Vec<u8>, _>("reference"), Some(r.get::<i64, _>("reference_point_count") as usize))
            .map_err(anyhow::Error::from)?;
        let other: Vec<(i32, i32)> = refp.iter().filter_map(|p| p.pos).collect();
        let (a, b) = similarity::overlap(&sel, &other, tol);
        if a >= similarity::SIMILAR_SHARE && b >= similarity::SIMILAR_SHARE {
            let id: String = r.get("id");
            let name: String = r.get("name");
            let length: f64 = r.get("length_m");
            found.push((a.min(b), json!({ "id": id, "name": name, "length_m": length, "overlap": a.min(b) })));
        }
    }
    found.sort_by(|x, y| y.0.total_cmp(&x.0));
    Ok(found.into_iter().map(|(_, v)| v).collect())
}

/// Legt ein Segment an und reiht die Auswertung gegen die Historie ein (RFC-0016 „Erstellen aus Aktivität“).
pub async fn create(cfg: &Validated, db: &Db, c: &Create) -> Result<Value, CreateError> {
    let name = c.name.trim();
    if name.is_empty() {
        return Err(invalid("name", "must_not_be_empty", "Name must not be empty"));
    }
    if !RANKED.contains(&c.sport_class.as_str()) {
        return Err(invalid("sport_class", "invalid_value", "Sport class must be cycling, ebike, running or skiing"));
    }
    let kind = c.kind.as_deref().unwrap_or("segment");
    if !KINDS.contains(&kind) {
        return Err(invalid("kind", "invalid_value", "Kind must be segment or route"));
    }
    if c.category.as_deref().is_some_and(|k| !categories_for(kind).contains(&k)) {
        return Err(invalid("category", "invalid_value", "Category does not fit the kind"));
    }
    // Eine Aktivität mit irgendeinem Versuch auf einer Strecke (auch ungewertet) ergibt keine weitere Strecke (Uli 09.11.2026)
    if kind == "route" {
        let matched: Option<String> = sqlx::query_scalar(
            "SELECT e.id FROM segment_efforts e JOIN segments s ON s.id = e.segment_id              WHERE e.activity_id = ? AND s.kind = 'route'              AND e.calculation_version_id = (SELECT id FROM calculation_versions WHERE status = 'published') LIMIT 1",
        )
        .bind(&c.activity_id)
        .fetch_optional(&db.reader)
        .await?;
        if matched.is_some() {
            return Err(CreateError::RouteAlreadyMatched);
        }
    }
    let pts = section_points(db, &c.activity_id, c.section_index).await?;
    let (start, end) = selection(&pts, c.start_index, c.end_index)?;
    // Strecke: Start und Ziel höchstens `route_edge_m` entlang der Aufzeichnung vom ersten bzw. letzten GPS-Punkt (RFC-0016 „Strecken“)
    if kind == "route" {
        let first = pts.iter().position(|p| p.pos.is_some()).unwrap_or(start);
        let last = pts.iter().rposition(|p| p.pos.is_some()).unwrap_or(end);
        let edge = cfg.raw.segments.route_edge_m;
        if along(&pts[first..=start]) > edge {
            return Err(invalid("start_index", "route_edge_exceeded", "Route start is too far from the beginning of the section"));
        }
        if along(&pts[end..=last]) > edge {
            return Err(invalid("end_index", "route_edge_exceeded", "Route end is too far from the end of the section"));
        }
    }

    // Referenz: Position und Höhe, ohne Zeit und Herzfrequenz (keine Bindung an die Quellaktivität)
    let reference = reference_of(&pts, start, end);
    let length = along(&reference);
    if reference.len() < 2 || length < cfg.raw.segments.minimum_length_m {
        return Err(CreateError::TooShort);
    }
    let degrees: Vec<(f64, f64)> =
        reference.iter().map(|p| p.pos.map(|(la, lo)| (e7_to_deg(la), e7_to_deg(lo))).expect("Position")).collect();
    match Reference::new(&degrees, &cfg.raw.segments.params_for(kind)) {
        Ok(_) => {}
        Err(ReferenceError::TooFewPoints) => return Err(CreateError::TooShort),
        Err(ReferenceError::Direction) => return Err(CreateError::Direction),
    }
    let blob = track::encode(&reference).map_err(anyhow::Error::from)?;
    let (s0, s1) = (degrees[0], degrees[degrees.len() - 1]);
    let auto = auto_category(kind, &reference, length, cfg.raw.segments.route_loop_radius_m);
    let manual = c.category.as_deref().filter(|m| Some(*m) != auto);

    let id = new_id();
    let now = chrono::Utc::now().timestamp_millis();
    let mut tx = db.writer.begin().await?;
    let creator: String = sqlx::query_scalar("SELECT id FROM users LIMIT 1").fetch_one(&mut *tx).await?;
    sqlx::query(
        "INSERT INTO segments (id, creator_id, name, description, sport_class, kind, length_m, start_lat, start_lng, end_lat, end_lng, \
         reference_format_version, reference_point_count, reference, created_at_ms, updated_at_ms, category_auto, category_manual, \
         location) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&creator)
    .bind(name)
    .bind(optional_text(&c.description))
    .bind(&c.sport_class)
    .bind(kind)
    .bind(length)
    .bind(s0.0)
    .bind(s0.1)
    .bind(s1.0)
    .bind(s1.1)
    .bind(track::FORMAT_VERSION as i64)
    .bind(reference.len() as i64)
    .bind(blob)
    .bind(now)
    .bind(now)
    .bind(auto)
    .bind(manual)
    .bind(optional_text(&c.location))
    .execute(&mut *tx)
    .await?;
    sqlx::query("INSERT INTO jobs (id, kind, segment_id, status, created_at_ms) VALUES (?, 'segment_history', ?, 'queued', ?)")
        .bind(new_id())
        .bind(&id)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    db::bump_revision(&mut tx, now).await?;
    tx.commit().await?;
    Ok(get(cfg, db, &id).await?.expect("eben angelegt"))
}

// ---------------------------------------------------------------------------------------------------------
// Lesen

/// Versuch als `Effort` (OpenAPI) mit abgeleiteten Werten.
pub fn effort_json(r: &sqlx::sqlite::SqliteRow, derived: Option<&Derived>, rank: Option<i64>) -> Value {
    json!({
        "id": r.get::<String, _>("id"),
        "segment_id": r.get::<String, _>("segment_id"),
        "segment_name": r.get::<String, _>("segment_name"),
        "activity_id": r.get::<String, _>("activity_id"),
        "activity_title": r.get::<String, _>("activity_title"),
        "activity_sport_class": r.get::<String, _>("activity_sport_class"),
        "activity_sub_sport": r.get::<Option<String>, _>("activity_sub_sport"),
        "attempt_position": r.get::<i64, _>("attempt_position"),
        "status": r.get::<String, _>("status"),
        "not_ranked_reason": r.get::<Option<String>, _>("not_ranked_reason"),
        "start_time": r.get::<Option<i64>, _>("start_time_ms").map(timestamp),
        "end_time": r.get::<Option<i64>, _>("end_time_ms").map(timestamp),
        "duration_ms": r.get::<Option<i64>, _>("duration_ms"),
        "rank": rank.or(derived.map(|d| d.rank)),
        "record_kind": derived.and_then(|d| d.record_kind).map(|k| k.code()),
        "improvement_ms": derived.and_then(|d| d.improvement_ms),
        "coverage_ratio": r.get::<Option<f64>, _>("coverage_ratio"),
        "avg_heart_rate_bpm": r.get::<Option<i64>, _>("avg_heart_rate_bpm"),
        "max_heart_rate_bpm": r.get::<Option<i64>, _>("max_heart_rate_bpm"),
    })
}

/// Versuche des veröffentlichten Stands mit Segment- und Aktivitätsnamen.
pub const EFFORTS: &str = "SELECT e.id, e.segment_id, s.name AS segment_name, e.activity_id, a.title AS activity_title, \
    a.sport_class AS activity_sport_class, a.sub_sport AS activity_sub_sport, e.attempt_position, e.status, e.not_ranked_reason, e.start_time_ms, e.end_time_ms, e.duration_ms, e.coverage_ratio, \
    e.avg_heart_rate_bpm, e.max_heart_rate_bpm \
    FROM segment_efforts e JOIN segments s ON s.id = e.segment_id JOIN activities a ON a.id = e.activity_id \
    WHERE e.calculation_version_id = (SELECT id FROM calculation_versions WHERE status = 'published')";

/// Abgeleitete Werte aller gültigen Versuche eines Segments.
pub async fn derived(conn: &mut SqliteConnection, segment_id: &str) -> sqlx::Result<HashMap<String, Derived>> {
    let rows = sqlx::query(
        "SELECT id, duration_ms, start_time_ms, activity_id, attempt_position FROM segment_efforts \
         WHERE segment_id = ? AND status = 'valid' \
         AND calculation_version_id = (SELECT id FROM calculation_versions WHERE status = 'published')",
    )
    .bind(segment_id)
    .fetch_all(conn)
    .await?;
    let valid: Vec<Valid> = rows
        .iter()
        .map(|r| Valid {
            id: r.get("id"),
            duration_ms: r.get("duration_ms"),
            start_ms: r.get("start_time_ms"),
            activity_id: r.get("activity_id"),
            position: r.get("attempt_position"),
        })
        .collect();
    Ok(ranking::derive(&valid))
}

/// Fortschritt der Auswertung gegen die Historie (`HistoryState`).
async fn history(conn: &mut SqliteConnection, segment_id: &str) -> sqlx::Result<Value> {
    let job = sqlx::query(
        "SELECT status, processed_items, total_items FROM jobs WHERE segment_id = ? AND kind = 'segment_history' \
         ORDER BY created_at_ms DESC, id DESC LIMIT 1",
    )
    .bind(segment_id)
    .fetch_optional(conn)
    .await?;
    Ok(match job {
        None => json!({ "status": "complete", "processed_items": 0, "total_items": 0 }),
        Some(j) => {
            let status = match j.get::<String, _>("status").as_str() {
                "queued" | "running" => "running",
                "failed" => "failed",
                _ => "complete",
            };
            json!({ "status": status, "processed_items": j.get::<i64, _>("processed_items"), "total_items": j.get::<i64, _>("total_items") })
        }
    })
}

/// `SegmentSummary`: Zählungen, die drei besten gültigen Versuche, Ø Dauer, erster und letzter Versuch.
async fn summary(conn: &mut SqliteConnection, segment_id: &str) -> sqlx::Result<Value> {
    let rows = sqlx::query(sqlx::AssertSqlSafe(format!("{EFFORTS} AND e.segment_id = ?"))).bind(segment_id).fetch_all(&mut *conn).await?;
    let d = derived(conn, segment_id).await?;
    let mut valid: Vec<(&sqlx::sqlite::SqliteRow, &Derived)> =
        rows.iter().filter_map(|r| d.get(&r.get::<String, _>("id")).map(|x| (r, x))).collect();
    valid.sort_by_key(|(_, x)| x.rank);
    let top = |i: usize| valid.get(i).map(|(r, x)| effort_json(r, Some(x), None));
    let durations: Vec<i64> = valid.iter().map(|(r, _)| r.get::<i64, _>("duration_ms")).collect();
    let starts: Vec<i64> = rows.iter().filter_map(|r| r.get::<Option<i64>, _>("start_time_ms")).collect();
    Ok(json!({
        "attempt_count": rows.len(),
        "valid_attempt_count": valid.len(),
        "best": top(0),
        "second": top(1),
        "third": top(2),
        "average_duration_ms": if durations.is_empty() { None } else { Some(durations.iter().sum::<i64>() as f64 / durations.len() as f64) },
        "first_attempt_time": starts.iter().min().copied().map(timestamp),
        "last_attempt_time": starts.iter().max().copied().map(timestamp),
    }))
}

/// Ø-Steigung in Prozent aus Start- und Zielhöhe der Referenz, auf eine Nachkommastelle.
fn grade_pct(pts: &[Point], length_m: f64) -> Option<f64> {
    let ele = |p: Option<&Point>| p.and_then(|p| p.ele_dm).map(|dm| dm as f64 / 10.0);
    match (ele(pts.first()), ele(pts.last())) {
        (Some(a), Some(b)) if length_m > 0.0 => Some(((b - a) / length_m * 1000.0).round() / 10.0),
        _ => None,
    }
}

const SEGMENT_COLUMNS: &str = "SELECT id, name, description, sport_class, kind, length_m, start_lat, start_lng, end_lat, end_lng, \
    reference, reference_point_count, created_at_ms, updated_at_ms, category_auto, category_manual, location FROM segments";

/// Wirksame Kategorie: von Hand gewählt, sonst automatisch.
const CATEGORY_SQL: &str = "coalesce(category_manual, category_auto)";

async fn segment_json(cfg: &Validated, conn: &mut SqliteConnection, r: &sqlx::sqlite::SqliteRow) -> anyhow::Result<Value> {
    let id: String = r.get("id");
    // Höhen aus der Referenz mit Hysterese (RFC-0009 „Kennzahlquelle“)
    let pts = track::decode(&r.get::<Vec<u8>, _>("reference"), Some(r.get::<i64, _>("reference_point_count") as usize))?;
    let length: f64 = r.get("length_m");
    let grade = grade_pct(&pts, length);
    let elevation = process::hysteresis(&[pts], cfg.raw.processing.elevation.hysteresis_m);
    Ok(json!({
        "id": id,
        "name": r.get::<String, _>("name"),
        "description": r.get::<Option<String>, _>("description"),
        "sport_class": r.get::<String, _>("sport_class"),
        "kind": r.get::<String, _>("kind"),
        "category": r.get::<Option<String>, _>("category_manual").or(r.get::<Option<String>, _>("category_auto")),
        "category_auto": r.get::<Option<String>, _>("category_auto"),
        "location": r.get::<Option<String>, _>("location"),
        "length_m": length,
        "elevation_gain_m": elevation.map(|e| e.gain_m),
        "elevation_loss_m": elevation.map(|e| e.loss_m),
        "average_grade_pct": grade,
        "start": { "lat": r.get::<f64, _>("start_lat"), "lon": r.get::<f64, _>("start_lng") },
        "end": { "lat": r.get::<f64, _>("end_lat"), "lon": r.get::<f64, _>("end_lng") },
        "history": history(conn, &id).await?,
        "summary": summary(conn, &id).await?,
        "created_at": timestamp(r.get("created_at_ms")),
        "updated_at": timestamp(r.get("updated_at_ms")),
    }))
}

pub async fn get(cfg: &Validated, db: &Db, id: &str) -> anyhow::Result<Option<Value>> {
    let mut conn = db.reader.acquire().await?;
    let Some(r) = sqlx::query(sqlx::AssertSqlSafe(format!("{SEGMENT_COLUMNS} WHERE id = ?"))).bind(id).fetch_optional(&mut *conn).await?
    else {
        return Ok(None);
    };
    Ok(Some(segment_json(cfg, &mut conn, &r).await?))
}

#[derive(Debug, Clone, Default)]
pub struct ListQuery {
    pub q: Option<String>,
    pub sport_class: Option<String>,
    pub kind: Option<String>,
    pub category: Option<String>,
    /// SQL-Sortierung aus einer festen Liste
    pub order: &'static str,
    pub limit: i64,
    pub offset: i64,
}

pub fn order_by(sort: &str) -> Option<&'static str> {
    Some(match sort {
        "name" => "name COLLATE NOCASE, id",
        "-name" => "name COLLATE NOCASE DESC, id",
        "created_at" => "created_at_ms, id",
        "-created_at" => "created_at_ms DESC, id",
        "length_m" => "length_m, id",
        "-length_m" => "length_m DESC, id",
        "location" => "location IS NULL, location COLLATE NOCASE, name COLLATE NOCASE, id",
        "-location" => "location IS NULL, location COLLATE NOCASE DESC, name COLLATE NOCASE, id",
        _ => return None,
    })
}

/// Liste mit `SegmentListSummary` über alle gefilterten Segmente (RFC-0006 „Segmentliste“).
pub async fn list(cfg: &Validated, db: &Db, q: &ListQuery) -> anyhow::Result<(Vec<Value>, i64, Value)> {
    let mut w = vec!["1 = 1".to_string()];
    let mut binds: Vec<String> = vec![];
    if let Some(t) = &q.q {
        w.push(
            "(instr(lower(name), lower(?)) > 0 OR instr(lower(coalesce(description, '')), lower(?)) > 0 \
             OR instr(lower(coalesce(location, '')), lower(?)) > 0)"
                .into(),
        );
        binds.push(t.clone());
        binds.push(t.clone());
        binds.push(t.clone());
    }
    if let Some(c) = &q.sport_class {
        w.push("sport_class = ?".into());
        binds.push(c.clone());
    }
    if let Some(k) = &q.kind {
        w.push("kind = ?".into());
        binds.push(k.clone());
    }
    if let Some(c) = &q.category {
        w.push(format!("{CATEGORY_SQL} = ?"));
        binds.push(c.clone());
    }
    let filter = w.join(" AND ");
    let mut conn = db.reader.acquire().await?;
    let count_sql = format!("SELECT count(*) FROM segments WHERE {filter}");
    let mut cq = sqlx::query_scalar(sqlx::AssertSqlSafe(count_sql));
    for b in &binds {
        cq = cq.bind(b);
    }
    let total: i64 = cq.fetch_one(&mut *conn).await?;
    let ids_sql = format!("SELECT id, length_m FROM segments WHERE {filter}");
    let mut iq = sqlx::query(sqlx::AssertSqlSafe(ids_sql));
    for b in &binds {
        iq = iq.bind(b);
    }
    let (mut length_m, mut attempts, mut improvements) = (0.0, 0i64, 0usize);
    for r in iq.fetch_all(&mut *conn).await? {
        let id: String = r.get("id");
        length_m += r.get::<f64, _>("length_m");
        attempts += sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM segment_efforts WHERE segment_id = ? \
             AND calculation_version_id = (SELECT id FROM calculation_versions WHERE status = 'published')",
        )
        .bind(&id)
        .fetch_one(&mut *conn)
        .await?;
        improvements += derived(&mut conn, &id).await?.values().filter(|d| d.record_kind == Some(ranking::RecordKind::Improvement)).count();
    }
    let summary = json!({ "count": total, "attempt_count": attempts, "length_m": length_m, "improvement_count": improvements });
    let order = if q.order.is_empty() { "name COLLATE NOCASE, id" } else { q.order };
    let sql = format!("{SEGMENT_COLUMNS} WHERE {filter} ORDER BY {order} LIMIT ? OFFSET ?");
    let mut lq = sqlx::query(sqlx::AssertSqlSafe(sql));
    for b in &binds {
        lq = lq.bind(b);
    }
    let rows = lq.bind(q.limit).bind(q.offset).fetch_all(&mut *conn).await?;
    let mut out = Vec::with_capacity(rows.len());
    for r in &rows {
        out.push(segment_json(cfg, &mut conn, r).await?);
    }
    Ok((out, total, summary))
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Update {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default, deserialize_with = "crate::activities::nullable")]
    pub description: Option<Option<String>>,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default, deserialize_with = "crate::activities::nullable")]
    pub location: Option<Option<String>>,
    /// `null`: zurück zur automatischen Kategorie
    #[serde(default, deserialize_with = "crate::activities::nullable")]
    pub category: Option<Option<String>>,
}

/// Name, Beschreibung, Ort, Kategorie und Art ändern; ein Artwechsel wertet die Historie neu aus (RFC-0016). `Ok(None)`: unbekannt.
pub async fn update(cfg: &Validated, db: &Db, id: &str, u: &Update) -> Result<Option<Value>, CreateError> {
    if u.name.is_none() && u.description.is_none() && u.kind.is_none() && u.location.is_none() && u.category.is_none() {
        return Err(invalid("body", "empty", "At least one field is required"));
    }
    if u.name.as_deref().is_some_and(|n| n.trim().is_empty()) {
        return Err(invalid("name", "must_not_be_empty", "Name must not be empty"));
    }
    if u.kind.as_deref().is_some_and(|k| !KINDS.contains(&k)) {
        return Err(invalid("kind", "invalid_value", "Kind must be segment or route"));
    }
    let now = chrono::Utc::now().timestamp_millis();
    let mut tx = db.writer.begin().await?;
    let exists: Option<String> = sqlx::query_scalar("SELECT id FROM segments WHERE id = ?").bind(id).fetch_optional(&mut *tx).await?;
    if exists.is_none() {
        return Ok(None);
    }
    if let Some(n) = &u.name {
        sqlx::query("UPDATE segments SET name = ? WHERE id = ?").bind(n.trim()).bind(id).execute(&mut *tx).await?;
    }
    if let Some(d) = &u.description {
        sqlx::query("UPDATE segments SET description = ? WHERE id = ?").bind(optional_text(d)).bind(id).execute(&mut *tx).await?;
    }
    if let Some(l) = &u.location {
        sqlx::query("UPDATE segments SET location = ? WHERE id = ?").bind(optional_text(l)).bind(id).execute(&mut *tx).await?;
    }
    let row = sqlx::query("SELECT kind, length_m, reference, reference_point_count, category_auto FROM segments WHERE id = ?")
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    let old: String = row.get("kind");
    let new_kind = u.kind.clone().unwrap_or_else(|| old.clone());
    if let Some(Some(c)) = &u.category
        && !categories_for(&new_kind).contains(&c.as_str())
    {
        return Err(invalid("category", "invalid_value", "Category does not fit the kind"));
    }
    let mut auto: Option<String> = row.get("category_auto");
    if new_kind != old {
        // Kategorien hängen von der Art ab: neu bestimmen, eine Wahl von Hand gilt nicht mehr
        let pts = track::decode(&row.get::<Vec<u8>, _>("reference"), Some(row.get::<i64, _>("reference_point_count") as usize))
            .map_err(anyhow::Error::from)?;
        auto = auto_category(&new_kind, &pts, row.get("length_m"), cfg.raw.segments.route_loop_radius_m).map(str::to_string);
        sqlx::query("UPDATE segments SET category_auto = ?, category_manual = NULL WHERE id = ?")
            .bind(&auto)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    if let Some(c) = &u.category {
        let manual = c.as_deref().filter(|m| Some(*m) != auto.as_deref());
        sqlx::query("UPDATE segments SET category_manual = ? WHERE id = ?").bind(manual).bind(id).execute(&mut *tx).await?;
    }
    if new_kind != old {
        sqlx::query("UPDATE segments SET kind = ? WHERE id = ?").bind(&new_kind).bind(id).execute(&mut *tx).await?;
        // Strecken haben eigene Matching-Werte: Historie neu auswerten (RFC-0016 „Art ändern“)
        sqlx::query("INSERT INTO jobs (id, kind, segment_id, status, created_at_ms) VALUES (?, 'segment_history', ?, 'queued', ?)")
            .bind(new_id())
            .bind(id)
            .bind(now)
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query("UPDATE segments SET updated_at_ms = ? WHERE id = ?").bind(now).bind(id).execute(&mut *tx).await?;
    db::bump_revision(&mut tx, now).await?;
    tx.commit().await?;
    Ok(get(cfg, db, id).await?)
}

pub async fn deletion_preview(db: &Db, id: &str) -> anyhow::Result<Option<Value>> {
    let mut conn = db.reader.acquire().await?;
    let exists: Option<String> = sqlx::query_scalar("SELECT id FROM segments WHERE id = ?").bind(id).fetch_optional(&mut *conn).await?;
    if exists.is_none() {
        return Ok(None);
    }
    let efforts: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM segment_efforts WHERE segment_id = ? \
         AND calculation_version_id = (SELECT id FROM calculation_versions WHERE status = 'published')",
    )
    .bind(id)
    .fetch_one(&mut *conn)
    .await?;
    Ok(Some(json!({ "object_type": "segment", "effort_count": efforts, "affected_segment_count": 0, "original_file_removed": null })))
}

/// Löscht Segment, Referenz, Versuche und Aufträge; nie Aktivitäten oder Originale (RFC-0016).
pub async fn delete(db: &Db, id: &str) -> anyhow::Result<bool> {
    let now = chrono::Utc::now().timestamp_millis();
    let mut tx = db.writer.begin().await?;
    let Some(name): Option<String> = sqlx::query_scalar("SELECT name FROM segments WHERE id = ?").bind(id).fetch_optional(&mut *tx).await?
    else {
        return Ok(false);
    };
    sqlx::query("DELETE FROM segments WHERE id = ?").bind(id).execute(&mut *tx).await?;
    audit::record(&mut tx, Action::Delete, "segment", Some(id), true, json!({ "name": name })).await?;
    db::bump_revision(&mut tx, now).await?;
    tx.commit().await?;
    Ok(true)
}

/// Referenz spaltenweise (`SegmentTrack`), gleichmäßig ausgedünnt wie die Aktivität.
pub async fn track(db: &Db, id: &str, max_points: Option<usize>) -> anyhow::Result<Option<Value>> {
    let Some(r) =
        sqlx::query("SELECT reference, reference_point_count FROM segments WHERE id = ?").bind(id).fetch_optional(&db.reader).await?
    else {
        return Ok(None);
    };
    let pts = track::decode(&r.get::<Vec<u8>, _>("reference"), Some(r.get::<i64, _>("reference_point_count") as usize))?;
    let mut dist = Vec::with_capacity(pts.len());
    let mut acc = 0.0;
    for (i, p) in pts.iter().enumerate() {
        if i > 0 {
            acc += geo::distance_m(pts[i - 1].pos.expect("Referenz mit Position"), p.pos.expect("Referenz mit Position"));
        }
        dist.push(acc);
    }
    let idx = crate::activities::thin_indices(pts.len(), max_points);
    let pos = |i: usize| pts[i].pos.expect("Referenz mit Position");
    Ok(Some(json!({ "data": {
        "point_count": pts.len(),
        "returned_point_count": idx.len(),
        "simplified": idx.len() < pts.len(),
        "index": idx,
        "lat": idx.iter().map(|&i| e7_to_deg(pos(i).0)).collect::<Vec<_>>(),
        "lon": idx.iter().map(|&i| e7_to_deg(pos(i).1)).collect::<Vec<_>>(),
        "ele_m": idx.iter().map(|&i| pts[i].ele_dm.map(|d| d as f64 / 10.0)).collect::<Vec<_>>(),
        "dist_m": idx.iter().map(|&i| (dist[i] * 10.0).round() / 10.0).collect::<Vec<_>>(),
    }})))
}

/// Zeitraum der Rangliste (OpenAPI `getSegmentLeaderboard`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Period {
    All,
    Year(i32),
    Month(i32, u32),
    /// Lokale Kalendertage einschließlich, z. B. eine Saison
    Days(chrono::NaiveDate, chrono::NaiveDate),
}

/// Rangliste der gültigen Versuche; bei Jahr/Monat zählt der lokale Segmentstart in der Zeitzone der TOML.
pub async fn leaderboard(
    cfg: &Validated,
    db: &Db,
    id: &str,
    period: Period,
    limit: i64,
    offset: i64,
) -> anyhow::Result<Option<(Vec<Value>, i64)>> {
    let mut conn = db.reader.acquire().await?;
    let exists: Option<String> = sqlx::query_scalar("SELECT id FROM segments WHERE id = ?").bind(id).fetch_optional(&mut *conn).await?;
    if exists.is_none() {
        return Ok(None);
    }
    let rows = sqlx::query(sqlx::AssertSqlSafe(format!("{EFFORTS} AND e.segment_id = ? AND e.status = 'valid'")))
        .bind(id)
        .fetch_all(&mut *conn)
        .await?;
    let d = derived(&mut conn, id).await?;
    let tz = cfg.timezone;
    let in_period = |ms: i64| {
        let Some(local) = chrono::DateTime::from_timestamp_millis(ms).map(|t| t.with_timezone(&tz)) else { return false };
        match period {
            Period::All => true,
            Period::Year(y) => local.year() == y,
            Period::Month(y, m) => local.year() == y && local.month() == m,
            Period::Days(from, to) => (from..=to).contains(&local.date_naive()),
        }
    };
    let mut list: Vec<(&sqlx::sqlite::SqliteRow, &Derived)> = rows
        .iter()
        .filter(|r| in_period(r.get("start_time_ms")))
        .filter_map(|r| d.get(&r.get::<String, _>("id")).map(|x| (r, x)))
        .collect();
    list.sort_by_key(|(_, x)| x.rank);
    let total = list.len() as i64;
    let out = list
        .iter()
        .enumerate()
        .skip(offset as usize)
        .take(limit as usize)
        .map(|(i, (r, x))| effort_json(r, Some(x), Some(i as i64 + 1)))
        .collect();
    Ok(Some((out, total)))
}

/// Alle Versuche eines Segments inklusive nicht gewerteter mit Grund.
pub async fn attempts(
    db: &Db,
    id: &str,
    status: Option<&str>,
    order: &str,
    limit: i64,
    offset: i64,
) -> anyhow::Result<Option<(Vec<Value>, i64)>> {
    let mut conn = db.reader.acquire().await?;
    let exists: Option<String> = sqlx::query_scalar("SELECT id FROM segments WHERE id = ?").bind(id).fetch_optional(&mut *conn).await?;
    if exists.is_none() {
        return Ok(None);
    }
    let filter = match status {
        Some("valid") => " AND e.status = 'valid'",
        Some(_) => " AND e.status = 'not_ranked'",
        None => "",
    };
    let total: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT count(*) FROM segment_efforts e WHERE e.segment_id = ?{filter} \
         AND e.calculation_version_id = (SELECT id FROM calculation_versions WHERE status = 'published')"
    )))
    .bind(id)
    .fetch_one(&mut *conn)
    .await?;
    let rows = sqlx::query(sqlx::AssertSqlSafe(format!("{EFFORTS} AND e.segment_id = ?{filter} ORDER BY {order} LIMIT ? OFFSET ?")))
        .bind(id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&mut *conn)
        .await?;
    let d = derived(&mut conn, id).await?;
    Ok(Some((rows.iter().map(|r| effort_json(r, d.get(&r.get::<String, _>("id")), None)).collect(), total)))
}

/// Sortierung der Versuchsliste.
pub fn attempts_order(sort: &str) -> Option<&'static str> {
    Some(match sort {
        "-start_time" => "e.start_time_ms DESC, e.activity_id, e.attempt_position",
        "start_time" => "e.start_time_ms, e.activity_id, e.attempt_position",
        "duration_ms" => "e.duration_ms IS NULL, e.duration_ms, e.start_time_ms",
        "-duration_ms" => "e.duration_ms IS NULL, e.duration_ms DESC, e.start_time_ms",
        _ => return None,
    })
}

/// Versuche einer Aktivität mit abgeleiteten Werten je Segment.
pub async fn activity_efforts(cfg: &Validated, db: &Db, activity_id: &str) -> anyhow::Result<Vec<Value>> {
    let mut conn = db.reader.acquire().await?;
    let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
        "{EFFORTS} AND e.activity_id = ? ORDER BY e.start_time_ms IS NULL, e.start_time_ms, s.name, e.attempt_position"
    )))
    .bind(activity_id)
    .fetch_all(&mut *conn)
    .await?;
    let mut cache: HashMap<String, (HashMap<String, Derived>, Value)> = HashMap::new();
    let mut out = vec![];
    for r in &rows {
        let seg: String = r.get("segment_id");
        if !cache.contains_key(&seg) {
            let d = derived(&mut conn, &seg).await?;
            let info = effort_segment(cfg, &mut conn, &seg, d.len()).await?;
            cache.insert(seg.clone(), (d, info));
        }
        let (d, info) = &cache[&seg];
        let mut e = effort_json(r, d.get(&r.get::<String, _>("id")), None);
        e["segment"] = info.clone();
        out.push(e);
    }
    Ok(out)
}

/// Kurzangaben zum Segment für die Segmenttabelle des Aktivitätsdetails (RFC-0006): Art, Sportklasse, Länge,
/// Aufstieg mit Hysterese, Steigung aus Start- und Zielhöhe, Zahl der gewerteten Versuche für „Rang n von m“.
async fn effort_segment(cfg: &Validated, conn: &mut SqliteConnection, id: &str, ranked: usize) -> anyhow::Result<Value> {
    let r = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT kind, sport_class, length_m, reference, reference_point_count, {CATEGORY_SQL} AS category FROM segments WHERE id = ?"
    )))
    .bind(id)
    .fetch_one(conn)
    .await?;
    let pts = track::decode(&r.get::<Vec<u8>, _>("reference"), Some(r.get::<i64, _>("reference_point_count") as usize))?;
    let length: f64 = r.get("length_m");
    let elevation = process::hysteresis(std::slice::from_ref(&pts), cfg.raw.processing.elevation.hysteresis_m);
    let grade = grade_pct(&pts, length);
    Ok(json!({
        "kind": r.get::<String, _>("kind"),
        "category": r.get::<Option<String>, _>("category"),
        "sport_class": r.get::<String, _>("sport_class"),
        "length_m": length,
        "elevation_gain_m": elevation.map(|e| e.gain_m),
        "average_grade_pct": grade,
        "ranked_count": ranked,
    }))
}

/// Referenzpunkte (Grad) eines Segments, z. B. für Tests.
pub async fn reference(db: &Db, id: &str) -> anyhow::Result<Option<Vec<(f64, f64)>>> {
    let Some(r) =
        sqlx::query("SELECT reference, reference_point_count FROM segments WHERE id = ?").bind(id).fetch_optional(&db.reader).await?
    else {
        return Ok(None);
    };
    Ok(Some(reference_points(&r.get::<Vec<u8>, _>("reference"), r.get("reference_point_count"))?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::track::deg_to_e7;

    /// Punkte aus (Ost, Nord) in Metern um 48° N, 8° O mit Höhe in Metern.
    fn pts(xy: &[(f64, f64, f64)]) -> Vec<Point> {
        let k = 48f64.to_radians().cos() * 111_320.0;
        xy.iter()
            .map(|&(x, y, h)| Point {
                t_ms: None,
                pos: Some((deg_to_e7(48.0 + y / 110_574.0), deg_to_e7(8.0 + x / k))),
                ele_dm: Some((h * 10.0) as i32),
                hr: None,
            })
            .collect()
    }

    fn line(from: (f64, f64), to: (f64, f64), n: usize) -> Vec<(f64, f64, f64)> {
        (0..=n).map(|i| (from.0 + (to.0 - from.0) * i as f64 / n as f64, from.1 + (to.1 - from.1) * i as f64 / n as f64, 0.0)).collect()
    }

    #[test]
    fn segment_category_follows_average_grade_with_two_percent_threshold() {
        let climb = pts(&[(0.0, 0.0, 100.0), (1000.0, 0.0, 120.0)]);
        assert_eq!(auto_category("segment", &climb, 1000.0, 750.0), Some("climb"));
        let barely = pts(&[(0.0, 0.0, 100.0), (1000.0, 0.0, 119.0)]);
        assert_eq!(auto_category("segment", &barely, 1000.0, 750.0), Some("rolling"));
        let down = pts(&[(0.0, 0.0, 100.0), (1000.0, 0.0, 70.0)]);
        assert_eq!(auto_category("segment", &down, 1000.0, 750.0), Some("descent"));
        let mut no_ele = climb.clone();
        no_ele[0].ele_dm = None;
        assert_eq!(auto_category("segment", &no_ele, 1000.0, 750.0), None);
    }

    #[test]
    fn route_category_distinguishes_loop_out_and_back_and_point_to_point() {
        let a2b = pts(&line((0.0, 0.0), (3000.0, 0.0), 300));
        assert_eq!(auto_category("route", &a2b, 3000.0, 750.0), Some("point_to_point"));
        // Quadrat 1 km: Start = Ziel, Rückweg woanders
        let mut sq = line((0.0, 0.0), (1000.0, 0.0), 100);
        sq.extend(line((1000.0, 0.0), (1000.0, 1000.0), 100));
        sq.extend(line((1000.0, 1000.0), (0.0, 1000.0), 100));
        sq.extend(line((0.0, 1000.0), (0.0, 0.0), 100));
        assert_eq!(auto_category("route", &pts(&sq), 4000.0, 750.0), Some("loop"));
        // Hin und auf demselben Weg 10 m daneben zurück
        let mut ob = line((0.0, 0.0), (2000.0, 0.0), 200);
        ob.extend(line((2000.0, 10.0), (0.0, 10.0), 200));
        assert_eq!(auto_category("route", &pts(&ob), 4000.0, 750.0), Some("out_and_back"));
    }

    #[test]
    fn route_ending_360_m_from_start_is_still_a_loop() {
        // Runde, deren Aufzeichnung 360 m vor dem Start endet
        let mut r = line((0.0, 0.0), (5000.0, 0.0), 500);
        r.extend(line((5000.0, 0.0), (5000.0, 3000.0), 300));
        r.extend(line((5000.0, 3000.0), (0.0, 3000.0), 500));
        r.extend(line((0.0, 3000.0), (0.0, 360.0), 264));
        assert_eq!(auto_category("route", &pts(&r), 15640.0, 750.0), Some("loop"));
        // Mit dem alten Radius von 200 m wäre es eine Strecke gewesen
        assert_eq!(auto_category("route", &pts(&r), 15640.0, 200.0), Some("point_to_point"));
        // Ende 800 m vom Start: Strecke
        let mut far = line((0.0, 0.0), (5000.0, 0.0), 500);
        far.extend(line((5000.0, 0.0), (0.0, 800.0), 500));
        assert_eq!(auto_category("route", &pts(&far), 10100.0, 750.0), Some("point_to_point"));
        // Kurze Gerade (333 m) bleibt Strecke, obwohl Start und Ziel weniger als 750 m auseinander liegen
        let short = pts(&line((0.0, 0.0), (333.0, 0.0), 30));
        assert_eq!(auto_category("route", &short, 333.0, 750.0), Some("point_to_point"));
    }
}
