//! Berechnungsstand (RFC-0001 `calculation_versions`, RFC-0007): genau ein veröffentlichter Stand mit dem
//! Hysteresewert je Aktivität und den Segmentversuchen. Ein neuer Stand löscht den alten nach der Veröffentlichung (C3).
//!
//! Beim Start: Passt der veröffentlichte Stand (Algorithmus und Parameter) zur Konfiguration, werden nur fehlende
//! Werte nachgetragen. Sonst entsteht ein neuer Stand, wird vollständig berechnet (Hysterese und alle Versuche)
//! und ersetzt den alten in einem Schritt; der alte wird danach gelöscht. Der Import trägt den Wert für neue Aktivitäten direkt in den veröffentlichten Stand ein.

use anyhow::Context;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{Row, SqliteConnection};

use crate::config::Validated;
use crate::db::{Db, new_id};
use crate::domain::track::{self, Point};
use crate::import::process;

/// Version des Matching- und Statistikalgorithmus (`VersionResponse.algorithm_version`).
pub const ALGORITHM_VERSION: i64 = 1;

/// Hash der Rechenparameter, die einen Berechnungsstand bestimmen (RFC-0021 „Neuberechnung“).
pub fn parameters_hash(cfg: &Validated) -> String {
    let p = &cfg.raw.processing.elevation;
    let s = &cfg.raw.segments;
    let m = &s.matching;
    let r = &s.route_matching;
    let basis = json!({
        "hysteresis_m": p.hysteresis_m,
        "minimum_length_m": s.minimum_length_m,
        "matching": [m.matching_radius_meters, m.max_vector_angle_deg, m.max_offset_m, m.tolerance_m, m.min_coverage_ratio,
                     m.direction_reference_m, m.max_backstep_m, m.max_inner_gap_m, m.max_interpolation_gap_s, m.coverage_connection_s,
                     m.max_inner_gap_ratio],
        "route_matching": [r.matching_radius_meters, r.tolerance_m, r.max_inner_gap_m, r.recording_edge_m,
                           r.max_vector_angle_deg, r.direction_reference_m],
    });
    Sha256::digest(basis.to_string().as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
}

/// ID des veröffentlichten Stands, falls vorhanden.
pub async fn published(conn: &mut SqliteConnection) -> sqlx::Result<Option<String>> {
    sqlx::query_scalar("SELECT id FROM calculation_versions WHERE status = 'published'").fetch_optional(conn).await
}

/// `meta.calculation` nach RFC-0003 Abschnitt 10.
pub async fn meta(db: &Db) -> anyhow::Result<Value> {
    let row =
        sqlx::query("SELECT id, published_at_ms FROM calculation_versions WHERE status = 'published'").fetch_optional(&db.reader).await?;
    let running: i64 =
        sqlx::query_scalar("SELECT count(*) FROM calculation_versions WHERE status = 'running'").fetch_one(&db.reader).await?;
    Ok(match row {
        None => json!({ "status": if running > 0 { "running" } else { "none" }, "version_id": null, "published_at": null }),
        Some(r) => {
            let at: Option<i64> = r.get("published_at_ms");
            json!({
                "status": if running > 0 { "stale" } else { "current" },
                "version_id": r.get::<String, _>("id"),
                "published_at": at.map(crate::import_job::timestamp),
            })
        }
    })
}

/// Liest alle Abschnitte einer Aktivität.
pub async fn sections(conn: &mut SqliteConnection, activity_id: &str) -> anyhow::Result<Vec<Vec<Point>>> {
    let rows = sqlx::query("SELECT point_count, data FROM track_sections WHERE activity_id = ? ORDER BY section_index")
        .bind(activity_id)
        .fetch_all(conn)
        .await?;
    rows.iter()
        .map(|r| {
            let n: i64 = r.get("point_count");
            let data: Vec<u8> = r.get("data");
            track::decode(&data, Some(n as usize)).with_context(|| format!("Messpunkte von {activity_id} unlesbar"))
        })
        .collect()
}

pub fn hysteresis_gain(sections: &[Vec<Point>], cfg: &Validated) -> Option<f64> {
    process::hysteresis(sections, cfg.raw.processing.elevation.hysteresis_m).map(|e| e.gain_m)
}

async fn insert_value(conn: &mut SqliteConnection, version: &str, activity_id: &str, gain: Option<f64>) -> sqlx::Result<()> {
    sqlx::query("INSERT OR REPLACE INTO activity_calculations (calculation_version_id, activity_id, hysteresis_gain_m) VALUES (?, ?, ?)")
        .bind(version)
        .bind(activity_id)
        .bind(gain)
        .execute(conn)
        .await?;
    Ok(())
}

/// Trägt den Wert einer neuen oder ersetzten Aktivität in den veröffentlichten Stand ein (in der Import-Transaktion).
pub async fn record_activity(conn: &mut SqliteConnection, activity_id: &str, gain: Option<f64>) -> sqlx::Result<()> {
    if let Some(v) = published(conn).await? {
        insert_value(conn, &v, activity_id, gain).await?;
    }
    Ok(())
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Ensured {
    pub new_version: bool,
    pub computed: u64,
    pub positions_filled: u64,
    /// Versuche im neuen Stand
    pub efforts: u64,
}

/// Beim Start: Stand prüfen bzw. neu berechnen und fehlende Werte nachtragen (siehe Moduldoku).
pub async fn ensure_published(cfg: &Validated, db: &Db) -> anyhow::Result<Ensured> {
    let mut out = Ensured::default();
    let hash = parameters_hash(cfg);
    let now = chrono::Utc::now().timestamp_millis();
    let mut conn = db.writer.acquire().await?;

    // Abgebrochene Berechnungen eines früheren Starts verwerfen.
    sqlx::query("DELETE FROM calculation_versions WHERE status = 'running'").execute(&mut *conn).await?;

    let current = sqlx::query("SELECT id, algorithm_version, parameters_hash FROM calculation_versions WHERE status = 'published'")
        .fetch_optional(&mut *conn)
        .await?;
    let fits = current
        .as_ref()
        .is_some_and(|r| r.get::<i64, _>("algorithm_version") == ALGORITHM_VERSION && r.get::<String, _>("parameters_hash") == hash);
    let version = if fits {
        current.as_ref().unwrap().get::<String, _>("id")
    } else {
        let id = new_id();
        sqlx::query("INSERT INTO calculation_versions (id, algorithm_version, parameters_hash, status, created_at_ms) VALUES (?, ?, ?, 'running', ?)")
            .bind(&id)
            .bind(ALGORITHM_VERSION)
            .bind(&hash)
            .bind(now)
            .execute(&mut *conn)
            .await?;
        out.new_version = true;
        tracing::info!(id, "Neuer Berechnungsstand wird berechnet");
        id
    };

    // Aktivitäten ohne Wert im Stand oder ohne bekannte Position
    let todo = sqlx::query(
        "SELECT a.id, a.has_position IS NULL AS no_pos, \
         NOT EXISTS (SELECT 1 FROM activity_calculations c WHERE c.calculation_version_id = ? AND c.activity_id = a.id) AS no_calc \
         FROM activities a WHERE a.has_position IS NULL \
         OR NOT EXISTS (SELECT 1 FROM activity_calculations c WHERE c.calculation_version_id = ? AND c.activity_id = a.id)",
    )
    .bind(&version)
    .bind(&version)
    .fetch_all(&mut *conn)
    .await?;
    for r in todo {
        let id: String = r.get("id");
        let pts = sections(&mut conn, &id).await?;
        let mut tx = sqlx::Connection::begin(&mut *conn).await?;
        if r.get::<bool, _>("no_pos") {
            let has = pts.iter().flatten().any(|p| p.pos.is_some());
            sqlx::query("UPDATE activities SET has_position = ? WHERE id = ?").bind(has).bind(&id).execute(&mut *tx).await?;
            out.positions_filled += 1;
        }
        if r.get::<bool, _>("no_calc") {
            insert_value(&mut tx, &version, &id, hysteresis_gain(&pts, cfg)).await?;
            out.computed += 1;
        }
        tx.commit().await?;
    }

    if out.new_version {
        // Versuche aller Aktivitäten gegen alle Segmente in den neuen Stand (RFC-0007 „Gesamtversion“)
        out.efforts = crate::segments::evaluation::evaluate_all(&mut conn, cfg, &version).await?;
        let mut tx = sqlx::Connection::begin(&mut *conn).await?;
        sqlx::query("UPDATE calculation_versions SET status = 'superseded' WHERE status = 'published'").execute(&mut *tx).await?;
        sqlx::query("UPDATE calculation_versions SET status = 'published', published_at_ms = ? WHERE id = ?")
            .bind(chrono::Utc::now().timestamp_millis())
            .bind(&version)
            .execute(&mut *tx)
            .await?;
        // Alte Stände sofort löschen, samt Werten und Versuchen (C3, Uli 09.11.2026)
        sqlx::query("DELETE FROM calculation_versions WHERE status = 'superseded'").execute(&mut *tx).await?;
        tx.commit().await?;
    }
    if out != Ensured::default() {
        tracing::info!(?out, "Berechnungsstand aktualisiert");
    }
    Ok(out)
}
