//! Demo mit erfundenen Daten: Kennzeichen setzen und beim Start alle Daten auf heute verschieben.
//!
//! Die Demo-Datenbank entsteht in der CI (`scripts/demo`), ist also beim Herunterladen Tage bis Wochen alt. Dashboard,
//! Woche, Form und Plan hängen am heutigen Datum. Mit `[demo] enabled = true` verschiebt der Start deshalb alles um
//! ganze Tage, sodass die letzte Aktivität gestern war. Verschoben wird je Aktivität in Ortszeit (10:00 bleibt 10:00
//! über die Sommerzeitumstellung hinweg); Messpunkte und Segmentversuche wandern mit derselben Differenz mit.
//! Ohne Zeile in `demo_state` (Migration 0020) bricht der Start ab: echte Daten werden nie verschoben.
//!
//! Nicht verschoben werden die Originaldateien im Aktivitätenordner, Protokoll-, Import- und Auftragszeiten sowie der
//! Geburtstag.

use anyhow::{Context, bail};
use chrono::{Days, NaiveDate, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Tz;
use sqlx::Row;

use crate::db::{self, Db};
use crate::domain::track;

/// Ergebnis einer Verschiebung.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shifted {
    pub days: i64,
    pub activities: usize,
    pub last_day: NaiveDate,
}

fn local_day(ms: i64, tz: Tz) -> anyhow::Result<NaiveDate> {
    Ok(Utc.timestamp_millis_opt(ms).single().context("Zeitstempel außerhalb des Bereichs")?.with_timezone(&tz).date_naive())
}

/// Lokale Zeit `days` Tage später, zurück nach UTC. Doppelte Stunde: die erste; Lücke: eine Stunde später.
fn shift_local(ms: i64, days: i64, tz: Tz) -> anyhow::Result<i64> {
    let local: NaiveDateTime =
        Utc.timestamp_millis_opt(ms).single().context("Zeitstempel außerhalb des Bereichs")?.with_timezone(&tz).naive_local();
    let moved = local + chrono::Duration::days(days);
    let utc = tz
        .from_local_datetime(&moved)
        .earliest()
        .or_else(|| tz.from_local_datetime(&(moved + chrono::Duration::hours(1))).earliest())
        .context("Ortszeit nicht darstellbar")?;
    Ok(utc.timestamp_millis())
}

async fn last_activity_day(conn: &mut sqlx::SqliteConnection, tz: Tz) -> anyhow::Result<Option<NaiveDate>> {
    let max: Option<i64> = sqlx::query_scalar("SELECT MAX(start_time_ms) FROM activities").fetch_one(&mut *conn).await?;
    max.map(|ms| local_day(ms, tz)).transpose()
}

/// Kennzeichnet die Datenbank als Demo-Datenbank (`trackcraft demo mark`, CI nach `seed_demo.py`).
pub async fn mark(db: &Db, tz: Tz) -> anyhow::Result<NaiveDate> {
    let mut tx = db.writer.begin().await?;
    let Some(last) = last_activity_day(&mut tx, tz).await? else {
        bail!("Die Datenbank enthält keine Aktivitäten; erst die Beispieldaten importieren");
    };
    sqlx::query("INSERT OR REPLACE INTO demo_state (id, last_day, shifted_at_ms) VALUES (1, ?, ?)")
        .bind(last.to_string())
        .bind(Utc::now().timestamp_millis())
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(last)
}

/// Ist die Datenbank eine Demo-Datenbank?
pub async fn is_marked(db: &Db) -> anyhow::Result<bool> {
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM demo_state").fetch_one(&db.reader).await?;
    Ok(n > 0)
}

/// Verschiebt alle Daten so, dass die letzte Aktivität am Vortag von `today` war. Liegt sie schon dort oder später,
/// geschieht nichts (`None`).
pub async fn shift_to(db: &Db, tz: Tz, today: NaiveDate) -> anyhow::Result<Option<Shifted>> {
    if !is_marked(db).await? {
        bail!(
            "[demo] enabled = true, aber das ist keine Demo-Datenbank. Echte Daten werden nie verschoben: \
             [demo] aus der Konfiguration entfernen oder einen eigenen Datenordner für die Demo verwenden"
        );
    }
    let target = today.checked_sub_days(Days::new(1)).context("Datum außerhalb des Bereichs")?;
    let mut tx = db.writer.begin().await?;
    let Some(last) = last_activity_day(&mut tx, tz).await? else {
        return Ok(None);
    };
    let days = (target - last).num_days();
    if days <= 0 {
        return Ok(None);
    }

    // Aktivitäten mit Messpunkten und Segmentversuchen, je Aktivität eigene Differenz (Ortszeit)
    let activities: Vec<(String, i64)> = sqlx::query("SELECT id, start_time_ms FROM activities")
        .fetch_all(&mut *tx)
        .await?
        .into_iter()
        .map(|r| (r.get("id"), r.get("start_time_ms")))
        .collect();
    for (id, start) in &activities {
        let delta = shift_local(*start, days, tz)? - start;
        sqlx::query(
            "UPDATE activities SET start_time_ms = start_time_ms + ?1, \
             start_time_original_ms = start_time_original_ms + ?1 WHERE id = ?2",
        )
        .bind(delta)
        .bind(id)
        .execute(&mut *tx)
        .await?;
        sqlx::query("UPDATE segment_efforts SET start_time_ms = start_time_ms + ?1, end_time_ms = end_time_ms + ?1 WHERE activity_id = ?2")
            .bind(delta)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        let sections =
            sqlx::query("SELECT id, point_count, data FROM track_sections WHERE activity_id = ?").bind(id).fetch_all(&mut *tx).await?;
        for s in sections {
            let sid: String = s.get("id");
            let count: i64 = s.get("point_count");
            let data: Vec<u8> = s.get("data");
            let mut points = track::decode(&data, Some(count as usize)).with_context(|| format!("Messpunkte {sid}"))?;
            for p in &mut points {
                if let Some(t) = p.t_ms.as_mut() {
                    *t += delta;
                }
            }
            let data = track::encode(&points).with_context(|| format!("Messpunkte {sid}"))?;
            sqlx::query(
                "UPDATE track_sections SET data = ?1, first_time_ms = first_time_ms + ?2, \
                 last_time_ms = last_time_ms + ?2 WHERE id = ?3",
            )
            .bind(data)
            .bind(delta)
            .bind(&sid)
            .execute(&mut *tx)
            .await?;
        }
    }

    // Kalendertage. HF-Zonen haben einen eindeutigen Beginn je Sportart: spätere zuerst, damit nichts kollidiert.
    let modifier = format!("+{days} days");
    for sql in [
        "UPDATE seasons SET start_date = date(start_date, ?1), end_date = date(end_date, ?1)",
        "UPDATE planned_units SET date = date(date, ?1)",
        "UPDATE plan_goals SET date = date(date, ?1)",
    ] {
        sqlx::query(sql).bind(&modifier).execute(&mut *tx).await?;
    }
    let zone_sets: Vec<String> = sqlx::query_scalar("SELECT id FROM hr_zone_sets ORDER BY valid_from DESC").fetch_all(&mut *tx).await?;
    for id in zone_sets {
        sqlx::query("UPDATE hr_zone_sets SET valid_from = date(valid_from, ?1) WHERE id = ?2")
            .bind(&modifier)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }

    let now = Utc::now().timestamp_millis();
    sqlx::query("UPDATE demo_state SET last_day = ?, shifted_at_ms = ? WHERE id = 1")
        .bind(target.to_string())
        .bind(now)
        .execute(&mut *tx)
        .await?;
    db::bump_revision(&mut tx, now).await?;
    tx.commit().await?;
    Ok(Some(Shifted { days, activities: activities.len(), last_day: target }))
}

#[cfg(test)]
mod tests {
    use super::*;

    const BERLIN: Tz = chrono_tz::Europe::Berlin;

    #[test]
    fn local_time_survives_dst() {
        // 2026-10-20 10:00 MESZ = 08:00 UTC; 14 Tage später 10:00 MEZ = 09:00 UTC
        let from = BERLIN.with_ymd_and_hms(2026, 10, 20, 10, 0, 0).unwrap().timestamp_millis();
        let to = BERLIN.with_ymd_and_hms(2026, 11, 3, 10, 0, 0).unwrap().timestamp_millis();
        assert_eq!(shift_local(from, 14, BERLIN).unwrap(), to);
        assert_eq!(to - from, 14 * 86_400_000 + 3_600_000);
    }

    #[test]
    fn spring_gap_moves_one_hour_later() {
        // 2026-03-22 02:30 + 7 Tage = 29.03. 02:30, gibt es nicht → 03:30 MESZ
        let from = BERLIN.with_ymd_and_hms(2026, 3, 22, 2, 30, 0).unwrap().timestamp_millis();
        let to = BERLIN.with_ymd_and_hms(2026, 3, 29, 3, 30, 0).unwrap().timestamp_millis();
        assert_eq!(shift_local(from, 7, BERLIN).unwrap(), to);
    }
}
