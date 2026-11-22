//! Statistikseite (RFC-0009 „Aktivitätsstatistik“, „Aktivitätsrekorde“, RFC-0006 „Statistik“).
//! Alles wird bei Abfrage aus den Aktivitäten gebildet, nichts gespeichert (C4).

use std::collections::BTreeMap;

use chrono::{Datelike, NaiveDate, TimeZone};
use serde_json::{Value, json};
use sqlx::Row;

use crate::activities::BASE;
use crate::config::Validated;
use crate::db::Db;
use crate::import_job::timestamp;
use crate::seasons::Season;

/// Sportklassen mit eigener Zusammenfassung; `unknown` nur in `all` (RFC-0009).
pub const SUMMARY_CLASSES: [&str; 6] = ["cycling", "ebike", "running", "skiing", "swimming", "studio"];
/// Sportklassen mit Rekorden: die mit Bestenlisten (Uli 14.11.2026: Skifahren ja, Schwimmen und Studio nicht).
pub const RANKED: [&str; 4] = ["cycling", "ebike", "running", "skiing"];
/// Reihenfolge der Stapel in den Balken, wie im Dashboard.
const STACK: [&str; 7] = ["cycling", "ebike", "running", "skiing", "swimming", "studio", "unknown"];

/// Auswahl der Seite: Zeitraum aus lokalen Tagen (je Seite offen) und Sportklasse/Untertyp.
#[derive(Debug, Clone, Default)]
pub struct Filter {
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
    pub sport_class: Option<String>,
    pub sub_sport: Option<String>,
}

#[derive(Debug, Clone)]
struct StatRow {
    id: String,
    title: String,
    class: String,
    sub: Option<String>,
    start_ms: i64,
    day: NaiveDate,
    distance: Option<f64>,
    time: i64,
    gain: Option<f64>,
    successes: i64,
}

/// Alle Aktivitäten zu Sportklasse/Untertyp mit lokalem Starttag (korrigierter Start), ältere zuerst.
async fn rows(cfg: &Validated, db: &Db, f: &Filter) -> anyhow::Result<Vec<StatRow>> {
    let sql = format!(
        "SELECT x.id, x.title, x.sport_class, x.sub_sport, x.start_time_ms, x.distance_m, x.total_time_ms, x.elevation, \
         x.segment_success_count FROM ({BASE}) x ORDER BY x.start_time_ms, x.id"
    );
    let all = sqlx::query(sqlx::AssertSqlSafe(sql)).fetch_all(&db.reader).await?;
    let tz = cfg.timezone;
    let mut out = Vec::with_capacity(all.len());
    for r in &all {
        let class: String = r.get("sport_class");
        let sub: Option<String> = r.get("sub_sport");
        if f.sport_class.as_deref().is_some_and(|c| c != class) || f.sub_sport.as_ref().is_some_and(|s| sub.as_ref() != Some(s)) {
            continue;
        }
        let start_ms: i64 = r.get("start_time_ms");
        let Some(day) = tz.timestamp_millis_opt(start_ms).single().map(|d| d.date_naive()) else { continue };
        if f.from.is_some_and(|d| day < d) || f.to.is_some_and(|d| day > d) {
            continue;
        }
        out.push(StatRow {
            id: r.get("id"),
            title: r.get("title"),
            class,
            sub,
            start_ms,
            day,
            distance: r.get("distance_m"),
            time: r.get("total_time_ms"),
            gain: r.get("elevation"),
            successes: r.get("segment_success_count"),
        });
    }
    Ok(out)
}

#[derive(Default)]
struct Acc {
    count: i64,
    distance: f64,
    time: i64,
    gain: f64,
    no_distance: i64,
    no_gain: i64,
    successes: i64,
}

impl Acc {
    fn add(&mut self, r: &StatRow) {
        self.count += 1;
        self.time += r.time;
        self.successes += r.successes;
        match r.distance {
            Some(d) => self.distance += d,
            None => self.no_distance += 1,
        }
        match r.gain {
            Some(g) => self.gain += g,
            None => self.no_gain += 1,
        }
    }

    fn summary(&self, class: &str) -> Value {
        json!({
            "sport_class": class, "count": self.count, "distance_m": self.distance, "total_time_ms": self.time,
            "elevation_gain_m": self.gain, "activities_without_distance": self.no_distance,
            "activities_without_elevation": self.no_gain, "segment_success_count": self.successes,
        })
    }

    fn short(&self) -> Value {
        json!({ "count": self.count, "distance_m": self.distance, "total_time_ms": self.time })
    }
}

/// Wert, nach dem verglichen wird: gerundet wie angezeigt (RFC-0009), größer ist besser.
fn record_key(kind: &str, class: &str, r: &StatRow) -> Option<i64> {
    match kind {
        "distance" => r.distance.map(|d| (d / 10.0).round() as i64),
        "elevation" => r.gain.map(|g| g.round() as i64),
        "time" => Some((r.time as f64 / 1000.0).round() as i64),
        "speed" => {
            let d = r.distance.filter(|d| *d > 0.0)?;
            if r.time <= 0 {
                return None;
            }
            if class == "running" {
                // Pace in s/km, kleinere gewinnt
                Some(-((r.time as f64 / 1000.0) / (d / 1000.0)).round() as i64)
            } else {
                Some((d / (r.time as f64 / 1000.0) * 3.6 * 10.0).round() as i64)
            }
        }
        _ => None,
    }
}

const RECORD_KINDS: [&str; 4] = ["distance", "elevation", "time", "speed"];

/// Rekorde je Sportklasse im Zeitraum; bei Gleichstand behält die frühere Aktivität den Rekord.
fn records(rows: &[StatRow]) -> Vec<Value> {
    let mut out = vec![];
    for class in RANKED {
        for kind in RECORD_KINDS {
            // `rows` ist nach Start sortiert: nur ein strikt besserer Wert löst ab
            let mut best: Option<(i64, &StatRow)> = None;
            let mut equaled = 0;
            for r in rows.iter().filter(|r| r.class == class) {
                let Some(k) = record_key(kind, class, r) else { continue };
                match best {
                    Some((b, _)) if k < b => {}
                    Some((b, _)) if k == b => equaled += 1,
                    _ => {
                        best = Some((k, r));
                        equaled = 0;
                    }
                }
            }
            if let Some((_, r)) = best {
                let value = match kind {
                    "distance" => json!(r.distance),
                    "elevation" => json!(r.gain),
                    "time" => json!(r.time),
                    _ => json!(r.distance.unwrap_or(0.0) / (r.time as f64 / 1000.0) * 3.6),
                };
                out.push(json!({
                    "sport_class": class, "kind": kind, "value": value, "equaled_count": equaled,
                    "activity": { "id": r.id, "title": r.title, "start_time": timestamp(r.start_ms), "sub_sport": r.sub },
                }));
            }
        }
    }
    out
}

fn class_order(c: &str) -> usize {
    STACK.iter().position(|x| *x == c).unwrap_or(STACK.len())
}

/// Kennzahlen, Monats- und Wochenbalken, Bestzeiten und Rekorde für die Auswahl.
pub async fn overview(cfg: &Validated, db: &Db, f: &Filter) -> anyhow::Result<Value> {
    let rows = rows(cfg, db, f).await?;
    let mut summaries = vec![];
    for class in SUMMARY_CLASSES.iter().copied().chain(["all"]) {
        let mut a = Acc::default();
        rows.iter().filter(|r| class == "all" || r.class == class).for_each(|r| a.add(r));
        summaries.push(a.summary(class));
    }
    let mut months: BTreeMap<((i32, u32), usize), Acc> = BTreeMap::new();
    let mut weeks: BTreeMap<((i32, u32), usize), Acc> = BTreeMap::new();
    for r in &rows {
        let o = class_order(&r.class);
        months.entry(((r.day.year(), r.day.month()), o)).or_default().add(r);
        let w = r.day.iso_week();
        weeks.entry(((w.year(), w.week()), o)).or_default().add(r);
    }
    let class_of = |o: usize| STACK.get(o).copied().unwrap_or("unknown");
    let by_month: Vec<Value> = months
        .iter()
        .map(|(((y, m), o), a)| {
            json!({ "month": format!("{y}-{m:02}"), "sport_class": class_of(*o), "count": a.count, "distance_m": a.distance,
                    "total_time_ms": a.time, "elevation_gain_m": a.gain })
        })
        .collect();
    let by_week: Vec<Value> = weeks
        .iter()
        .map(|(((y, w), o), a)| {
            let monday = NaiveDate::from_isoywd_opt(*y, *w, chrono::Weekday::Mon).map(|d| d.to_string());
            json!({ "week": format!("{y}-W{w:02}"), "week_start": monday, "sport_class": class_of(*o), "count": a.count,
                    "distance_m": a.distance, "total_time_ms": a.time })
        })
        .collect();
    Ok(json!({
        "from": f.from.map(|d| d.to_string()),
        "to": f.to.map(|d| d.to_string()),
        "first_day": rows.first().map(|r| r.day.to_string()),
        "last_day": rows.last().map(|r| r.day.to_string()),
        "summaries": summaries,
        "by_month": by_month,
        "by_week": by_week,
        "records": records(&rows),
    }))
}

/// Kurzsummen je Saison und je Kalenderjahr für die Kacheln; Jahre mit Aktivitäten und das laufende Jahr.
pub async fn periods(cfg: &Validated, db: &Db, f: &Filter, seasons: &[Season], today: NaiveDate) -> anyhow::Result<Value> {
    let rows = rows(cfg, db, f).await?;
    let mut years: BTreeMap<i32, Acc> = BTreeMap::new();
    years.entry(today.year()).or_default();
    for r in &rows {
        years.entry(r.day.year()).or_default().add(r);
    }
    let seasons: Vec<Value> = seasons
        .iter()
        .map(|s| {
            let mut a = Acc::default();
            rows.iter().filter(|r| s.contains(r.day)).for_each(|r| a.add(r));
            let mut v = a.short();
            v["season_id"] = json!(s.id);
            v
        })
        .collect();
    let years: Vec<Value> = years
        .iter()
        .rev()
        .map(|(y, a)| {
            let mut v = a.short();
            v["year"] = json!(y);
            v
        })
        .collect();
    Ok(json!({ "seasons": seasons, "years": years }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: &str, class: &str, day: u32, distance: Option<f64>, time_s: i64, gain: Option<f64>) -> StatRow {
        StatRow {
            id: id.into(),
            title: id.into(),
            class: class.into(),
            sub: None,
            start_ms: i64::from(day) * 86_400_000,
            day: NaiveDate::from_ymd_opt(2026, 7, day).unwrap(),
            distance,
            time: time_s * 1000,
            gain,
            successes: 0,
        }
    }

    fn find<'a>(r: &'a [Value], class: &str, kind: &str) -> &'a Value {
        r.iter().find(|x| x["sport_class"] == class && x["kind"] == kind).unwrap()
    }

    #[test]
    fn skiing_has_records_swimming_and_studio_do_not() {
        let rows = vec![
            row("k", "skiing", 1, Some(20_000.0), 5400, Some(250.0)),
            row("w", "swimming", 2, Some(2_000.0), 2400, None),
            row("g", "studio", 3, None, 3600, None),
        ];
        let r = records(&rows);
        assert_eq!(find(&r, "skiing", "distance")["activity"]["id"], "k");
        assert!(r.iter().all(|x| x["sport_class"] == "skiing"));
    }

    #[test]
    fn records_round_like_display_and_keep_the_earlier_on_ties() {
        let rows = vec![
            row("a", "cycling", 1, Some(50_004.0), 7200, Some(300.4)),
            // 50 000 m auf 10 m gerundet gleich: Einstellung, a bleibt
            row("b", "cycling", 2, Some(49_996.0), 7000, None),
            row("c", "cycling", 3, None, 9000, Some(300.6)),
            row("r1", "running", 4, Some(10_000.0), 3000, None), // 5:00 /km
            row("r2", "running", 5, Some(10_001.0), 3000, None), // 299,97 s/km → 5:00, Einstellung
            row("u", "unknown", 6, Some(99_000.0), 99_000, Some(999.0)),
        ];
        let r = records(&rows);
        assert_eq!(find(&r, "cycling", "distance")["activity"]["id"], "a");
        assert_eq!(find(&r, "cycling", "distance")["equaled_count"], 1);
        assert_eq!(find(&r, "cycling", "elevation")["activity"]["id"], "c");
        assert_eq!(find(&r, "cycling", "time")["activity"]["id"], "c");
        assert_eq!(find(&r, "cycling", "speed")["activity"]["id"], "b", "ohne Distanz zählt c nicht");
        assert_eq!(find(&r, "running", "speed")["activity"]["id"], "r1");
        assert_eq!(find(&r, "running", "speed")["equaled_count"], 1);
        assert!(r.iter().all(|x| x["sport_class"] != "unknown"));
        assert!(!r.iter().any(|x| x["sport_class"] == "ebike"));
        assert!(
            !r.iter().any(|x| x["sport_class"] == "running" && x["kind"] == "elevation"),
            "fehlender Wert schließt nur die Kategorie aus"
        );
    }
}
