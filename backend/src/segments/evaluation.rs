//! Auswertung mit der Datenbank (RFC-0007 „Nachtrag“, RFC-0002): Versuche einer Aktivität gegen alle Segmente
//! ihrer Sportklasse bzw. eines Segments gegen alle passenden Aktivitäten. Ergebnisse werden je Auswertung in
//! einer Transaktion ersetzt.

use anyhow::Context;
use sqlx::{Row, SqliteConnection};

use super::matching::{self, Attempt, Reference};
use crate::config::{MatchingSection, SegmentsSection, Validated};
use crate::db::{Db, new_id};
use crate::domain::track::{self, Point, e7_to_deg};

/// Sportklassen mit Bestenlisten (`unknown` bleibt ausgeschlossen, RFC-0009).
pub const RANKED: [&str; 4] = ["cycling", "ebike", "running", "skiing"];

/// Segment mit entschlüsselter Referenz und den Matching-Parametern seiner Art.
pub struct SegmentRef {
    pub id: String,
    pub reference: Reference,
    pub params: MatchingSection,
}

/// Positionen (Grad) aus einer gespeicherten Referenz.
pub fn reference_points(blob: &[u8], count: i64) -> anyhow::Result<Vec<(f64, f64)>> {
    let pts = track::decode(blob, Some(count as usize)).context("Segmentreferenz unlesbar")?;
    Ok(pts.iter().filter_map(|p| p.pos.map(|(la, lo)| (e7_to_deg(la), e7_to_deg(lo)))).collect())
}

fn to_ref(id: String, kind: &str, blob: &[u8], count: i64, sg: &SegmentsSection) -> Option<SegmentRef> {
    let params = sg.params_for(kind);
    match reference_points(blob, count).map(|pts| Reference::new(&pts, &params)) {
        Ok(Ok(reference)) => Some(SegmentRef { id, reference: reference.with_open_ends(kind == "route"), params }),
        Ok(Err(e)) => {
            // z. B. nach einer Änderung von `direction_reference_m`: Segment bleibt, liefert aber keine Versuche
            tracing::warn!(segment = id, error = %e, "Segment wird nicht ausgewertet");
            None
        }
        Err(e) => {
            tracing::warn!(segment = id, error = format!("{e:#}"), "Segment wird nicht ausgewertet");
            None
        }
    }
}

pub async fn load_segment(conn: &mut SqliteConnection, id: &str, sg: &SegmentsSection) -> anyhow::Result<Option<(String, SegmentRef)>> {
    let Some(r) = sqlx::query("SELECT sport_class, kind, reference, reference_point_count FROM segments WHERE id = ?")
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?
    else {
        return Ok(None);
    };
    let class: String = r.get("sport_class");
    let kind: String = r.get("kind");
    Ok(to_ref(id.to_string(), &kind, &r.get::<Vec<u8>, _>("reference"), r.get("reference_point_count"), sg).map(|s| (class, s)))
}

pub async fn load_segments(conn: &mut SqliteConnection, class: &str, sg: &SegmentsSection) -> anyhow::Result<Vec<SegmentRef>> {
    let rows = sqlx::query("SELECT id, kind, reference, reference_point_count FROM segments WHERE sport_class = ? ORDER BY id")
        .bind(class)
        .fetch_all(&mut *conn)
        .await?;
    Ok(rows
        .iter()
        .filter_map(|r| {
            let kind: String = r.get("kind");
            to_ref(r.get("id"), &kind, &r.get::<Vec<u8>, _>("reference"), r.get("reference_point_count"), sg)
        })
        .collect())
}

/// Abschnitte einer Aktivität mit ihrer ID.
pub async fn sections(conn: &mut SqliteConnection, activity_id: &str) -> anyhow::Result<Vec<(String, Vec<Point>)>> {
    let rows = sqlx::query("SELECT id, point_count, data FROM track_sections WHERE activity_id = ? ORDER BY section_index")
        .bind(activity_id)
        .fetch_all(&mut *conn)
        .await?;
    rows.iter()
        .map(|r| {
            let n: i64 = r.get("point_count");
            let pts = track::decode(&r.get::<Vec<u8>, _>("data"), Some(n as usize))
                .with_context(|| format!("Messpunkte von {activity_id} unlesbar"))?;
            Ok((r.get("id"), pts))
        })
        .collect()
}

/// Versuche eines Abschnitts auf einem Segment.
pub struct Found {
    pub segment_id: String,
    pub activity_id: String,
    pub section_id: String,
    pub attempts: Vec<Attempt>,
}

pub fn match_all(segments: &[SegmentRef], activity_id: &str, sections: &[(String, Vec<Point>)]) -> Vec<Found> {
    let mut out = vec![];
    for s in segments {
        for (section_id, pts) in sections {
            let attempts = matching::match_section(&s.reference, pts, &s.params);
            if !attempts.is_empty() {
                out.push(Found {
                    segment_id: s.id.clone(),
                    activity_id: activity_id.to_string(),
                    section_id: section_id.clone(),
                    attempts,
                });
            }
        }
    }
    out
}

/// Schreibt Versuche; Abschnitte, die es inzwischen nicht mehr gibt (gelöscht, ersetzt), werden übergangen.
pub async fn insert(conn: &mut SqliteConnection, version: &str, found: &[Found], now: i64) -> sqlx::Result<u64> {
    let mut n = 0;
    for f in found {
        for a in &f.attempts {
            let done = sqlx::query(
                "INSERT INTO segment_efforts (id, calculation_version_id, segment_id, activity_id, section_id, attempt_position, status, \
                 not_ranked_reason, start_time_ms, end_time_ms, duration_ms, start_deviation_m, end_deviation_m, start_angle_deg, \
                 end_angle_deg, coverage_ratio, avg_heart_rate_bpm, max_heart_rate_bpm, created_at_ms) \
                 SELECT ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ? \
                 WHERE EXISTS (SELECT 1 FROM track_sections WHERE id = ? AND activity_id = ?) \
                 AND EXISTS (SELECT 1 FROM segments WHERE id = ?)",
            )
            .bind(new_id())
            .bind(version)
            .bind(&f.segment_id)
            .bind(&f.activity_id)
            .bind(&f.section_id)
            .bind(a.position as i64)
            .bind(if a.is_valid() { "valid" } else { "not_ranked" })
            .bind(a.not_ranked.map(|r| r.code()))
            .bind(a.start_ms)
            .bind(a.end_ms)
            .bind(a.duration_ms)
            .bind(a.start_deviation_m)
            .bind(a.end_deviation_m)
            .bind(a.start_angle_deg)
            .bind(a.end_angle_deg)
            .bind(a.coverage_ratio)
            .bind(a.avg_heart_rate_bpm.map(i64::from))
            .bind(a.max_heart_rate_bpm.map(i64::from))
            .bind(now)
            .bind(&f.section_id)
            .bind(&f.activity_id)
            .bind(&f.segment_id)
            .execute(&mut *conn)
            .await?;
            n += done.rows_affected();
        }
    }
    Ok(n)
}

async fn published(conn: &mut SqliteConnection) -> sqlx::Result<Option<String>> {
    crate::calculation::published(conn).await
}

/// Nachtrag für eine Aktivität: alle Segmente ihrer Sportklasse (Job `activity_evaluation`).
pub async fn evaluate_activity(cfg: &Validated, db: &Db, activity_id: &str) -> anyhow::Result<()> {
    let sg = &cfg.raw.segments;
    let mut r = db.reader.acquire().await?;
    let Some(class): Option<String> =
        sqlx::query_scalar("SELECT sport_class FROM activities WHERE id = ?").bind(activity_id).fetch_optional(&mut *r).await?
    else {
        return Ok(()); // inzwischen gelöscht
    };
    let found = if RANKED.contains(&class.as_str()) {
        let segments = load_segments(&mut r, &class, sg).await?;
        let secs = if segments.is_empty() { vec![] } else { sections(&mut r, activity_id).await? };
        match_all(&segments, activity_id, &secs)
    } else {
        vec![]
    };
    drop(r);

    let now = chrono::Utc::now().timestamp_millis();
    let mut tx = db.writer.begin().await?;
    if let Some(v) = published(&mut tx).await? {
        sqlx::query("DELETE FROM segment_efforts WHERE activity_id = ? AND calculation_version_id = ?")
            .bind(activity_id)
            .bind(&v)
            .execute(&mut *tx)
            .await?;
        insert(&mut tx, &v, &found, now).await?;
    }
    sqlx::query("UPDATE activities SET processing_status = 'done' WHERE id = ?").bind(activity_id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}

/// Segment gegen die Historie (Job `segment_history`); `progress(verarbeitet, gesamt)` nach jeder Aktivität.
pub async fn segment_history<F, Fut>(cfg: &Validated, db: &Db, segment_id: &str, mut progress: F) -> anyhow::Result<()>
where
    F: FnMut(u64, u64) -> Fut,
    Fut: std::future::Future<Output = anyhow::Result<()>>,
{
    let mut r = db.reader.acquire().await?;
    let Some((class, segment)) = load_segment(&mut r, segment_id, &cfg.raw.segments).await? else {
        return Ok(()); // gelöscht oder nicht auswertbar
    };
    let ids: Vec<String> =
        sqlx::query_scalar("SELECT id FROM activities WHERE sport_class = ? AND has_position = 1 ORDER BY start_time_ms, id")
            .bind(&class)
            .fetch_all(&mut *r)
            .await?;
    let total = ids.len() as u64;
    progress(0, total).await?;
    let segments = [segment];
    let mut found = vec![];
    for (i, id) in ids.iter().enumerate() {
        let secs = sections(&mut r, id).await?;
        found.extend(match_all(&segments, id, &secs));
        if (i + 1) % 10 == 0 || i + 1 == ids.len() {
            progress(i as u64 + 1, total).await?;
        }
    }
    drop(r);

    let now = chrono::Utc::now().timestamp_millis();
    let mut tx = db.writer.begin().await?;
    if let Some(v) = published(&mut tx).await? {
        sqlx::query("DELETE FROM segment_efforts WHERE segment_id = ? AND calculation_version_id = ?")
            .bind(segment_id)
            .bind(&v)
            .execute(&mut *tx)
            .await?;
        insert(&mut tx, &v, &found, now).await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Alle Versuche für einen neuen Stand (Neuberechnung beim Start, RFC-0007); `conn` ist die schreibende Verbindung.
pub async fn evaluate_all(conn: &mut SqliteConnection, cfg: &Validated, version: &str) -> anyhow::Result<u64> {
    let mut n = 0;
    for class in RANKED {
        let segments = load_segments(conn, class, &cfg.raw.segments).await?;
        if segments.is_empty() {
            continue;
        }
        let ids: Vec<String> = sqlx::query_scalar("SELECT id FROM activities WHERE sport_class = ? AND has_position = 1")
            .bind(class)
            .fetch_all(&mut *conn)
            .await?;
        for id in ids {
            let secs = sections(conn, &id).await?;
            let found = match_all(&segments, &id, &secs);
            let now = chrono::Utc::now().timestamp_millis();
            n += insert(conn, version, &found, now).await?;
        }
    }
    Ok(n)
}
