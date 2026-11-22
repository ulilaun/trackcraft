//! Ortsvorschlag für Aktivitäten (#22, Uli 22.11.2026): zuerst eigene Daten, sonst OpenStreetMap (Nominatim).
//!
//! Eigene Daten: Aktivitäten und Strecken/Segmente mit Ort, deren Start höchstens [`OWN_RADIUS_M`] vom Start der
//! Aktivität entfernt liegt. Es gewinnt der häufigste Ort, bei Gleichstand der nächste. OpenStreetMap wird nur gefragt,
//! wenn das in den Einstellungen erlaubt ist; gesendet wird allein die auf vier Stellen gerundete Startkoordinate.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;

use crate::config::Validated;
use crate::db::{Db, bump_revision};
use crate::domain::track::{self, deg_to_e7, e7_to_deg};
use crate::geo;

/// Umkreis um den Start, in dem ein bekannter Ort als Vorschlag zählt.
pub const OWN_RADIUS_M: f64 = 1500.0;
/// Nominatim erlaubt höchstens eine Anfrage pro Sekunde.
const OSM_SPACING: Duration = Duration::from_secs(1);

pub async fn settings(db: &Db) -> anyhow::Result<Value> {
    let on: i64 = sqlx::query_scalar("SELECT osm_enabled FROM place_settings WHERE id = 1").fetch_one(&db.reader).await?;
    Ok(json!({ "osm_enabled": on == 1 }))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SettingsInput {
    pub osm_enabled: bool,
}

pub async fn update_settings(db: &Db, input: &SettingsInput) -> anyhow::Result<Value> {
    let now = chrono::Utc::now().timestamp_millis();
    let mut tx = db.writer.begin().await?;
    sqlx::query("UPDATE place_settings SET osm_enabled = ?, updated_at_ms = ? WHERE id = 1")
        .bind(input.osm_enabled as i64)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    bump_revision(&mut tx, now).await?;
    tx.commit().await?;
    settings(db).await
}

/// Startpunkte je Abschnitt (`track_sections.id`): Abschnitte ändern sich nicht, ein Ersetzen legt neue an.
type Position = (i32, i32);
static STARTS: LazyLock<Mutex<HashMap<String, Option<Position>>>> = LazyLock::new(Default::default);

fn first_position(data: &[u8], n: i64) -> anyhow::Result<Option<(i32, i32)>> {
    Ok(track::decode(data, Some(n as usize))?.iter().find_map(|p| p.pos))
}

/// Erster Punkt mit Position über alle Abschnitte; `None` ohne Position.
async fn start_of(db: &Db, activity_id: &str) -> anyhow::Result<Option<(i32, i32)>> {
    let rows = sqlx::query("SELECT point_count, data FROM track_sections WHERE activity_id = ? ORDER BY section_index")
        .bind(activity_id)
        .fetch_all(&db.reader)
        .await?;
    for r in rows {
        if let Some(p) = first_position(&r.get::<Vec<u8>, _>("data"), r.get("point_count"))? {
            return Ok(Some(p));
        }
    }
    Ok(None)
}

/// Orte bekannter Starts: Strecken/Segmente direkt, Aktivitäten über den ersten Abschnitt (zwischengespeichert).
async fn known_places(db: &Db, except: &str) -> anyhow::Result<Vec<(String, (i32, i32))>> {
    let mut out: Vec<(String, (i32, i32))> = sqlx::query("SELECT location, start_lat, start_lng FROM segments WHERE location IS NOT NULL")
        .fetch_all(&db.reader)
        .await?
        .iter()
        .map(|r| (r.get("location"), (deg_to_e7(r.get("start_lat")), deg_to_e7(r.get("start_lng")))))
        .collect();
    let rows = sqlx::query(
        "SELECT a.location, s.id AS section_id FROM activities a \
         JOIN track_sections s ON s.activity_id = a.id AND s.section_index = 0 \
         WHERE a.location IS NOT NULL AND a.id <> ?",
    )
    .bind(except)
    .fetch_all(&db.reader)
    .await?;
    let missing: Vec<String> = {
        let cache = STARTS.lock().expect("Startpunkte");
        rows.iter().map(|r| r.get::<String, _>("section_id")).filter(|id| !cache.contains_key(id)).collect()
    };
    for id in missing {
        let r = sqlx::query("SELECT point_count, data FROM track_sections WHERE id = ?").bind(&id).fetch_optional(&db.reader).await?;
        let start = match r {
            Some(r) => first_position(&r.get::<Vec<u8>, _>("data"), r.get("point_count")).unwrap_or(None),
            None => None,
        };
        STARTS.lock().expect("Startpunkte").insert(id, start);
    }
    let cache = STARTS.lock().expect("Startpunkte");
    out.extend(rows.iter().filter_map(|r| {
        let start = cache.get(&r.get::<String, _>("section_id")).copied().flatten()?;
        Some((r.get::<String, _>("location"), start))
    }));
    Ok(out)
}

/// Häufigster Ort im Umkreis, bei Gleichstand der nächste; mit Abstand des nächsten Vorkommens.
pub fn vote(start: (i32, i32), places: &[(String, (i32, i32))]) -> Option<(String, f64)> {
    let mut seen: HashMap<&str, (usize, f64)> = HashMap::new();
    for (name, p) in places {
        let d = geo::distance_m(start, *p);
        if d <= OWN_RADIUS_M {
            let e = seen.entry(name.as_str()).or_insert((0, f64::MAX));
            e.0 += 1;
            e.1 = e.1.min(d);
        }
    }
    seen.into_iter().max_by(|a, b| a.1.0.cmp(&b.1.0).then(b.1.1.total_cmp(&a.1.1)).then(b.0.cmp(a.0))).map(|(n, (_, d))| (n.to_string(), d))
}

#[derive(Debug, Deserialize)]
struct Reverse {
    #[serde(default)]
    address: HashMap<String, Value>,
}

/// Ortsname aus der Adresse: Gemeinde vor Ortsteil.
fn place_name(r: &Reverse) -> Option<String> {
    ["village", "town", "city", "municipality", "hamlet", "suburb"]
        .iter()
        .find_map(|k| r.address.get(*k).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).map(str::to_string))
}

/// Gerundete Koordinate (Tausendstel Grad) → Ortsname
type OsmKey = (i64, i64);
static OSM_CACHE: LazyLock<Mutex<HashMap<OsmKey, Option<String>>>> = LazyLock::new(Default::default);
static OSM_LAST: LazyLock<tokio::sync::Mutex<Option<Instant>>> = LazyLock::new(Default::default);
static OSM_HTTP: OnceLock<reqwest::Client> = OnceLock::new();

async fn osm(cfg: &Validated, start: (i32, i32)) -> anyhow::Result<Option<String>> {
    // Auf vier Nachkommastellen (gut 10 m) gerundet: genau genug für den Ort, nicht für die Haustür
    let (lat, lon) = ((e7_to_deg(start.0) * 1e4).round() / 1e4, (e7_to_deg(start.1) * 1e4).round() / 1e4);
    let key = ((lat * 1e3).round() as i64, (lon * 1e3).round() as i64);
    if let Some(hit) = OSM_CACHE.lock().expect("OSM-Cache").get(&key) {
        return Ok(hit.clone());
    }
    let http = match OSM_HTTP.get() {
        Some(c) => c.clone(),
        None => {
            let c = crate::garmin::client::https_client(Duration::from_secs(10), Duration::from_secs(20))?;
            OSM_HTTP.get_or_init(|| c).clone()
        }
    };
    {
        let mut last = OSM_LAST.lock().await;
        if let Some(t) = *last {
            let wait = OSM_SPACING.saturating_sub(t.elapsed());
            if !wait.is_zero() {
                tokio::time::sleep(wait).await;
            }
        }
        *last = Some(Instant::now());
    }
    // Nur Zahlen und feste Werte in der Abfrage, darum ohne Kodierung
    let url = format!(
        "{}/reverse?format=jsonv2&zoom=14&layer=address&lat={lat:.4}&lon={lon:.4}",
        cfg.raw.places.nominatim_url.trim_end_matches('/')
    );
    let resp: reqwest::Response = http.get(url).header(reqwest::header::ACCEPT_LANGUAGE, "de").send().await?.error_for_status()?;
    let name = place_name(&serde_json::from_slice::<Reverse>(&resp.bytes().await?)?);
    OSM_CACHE.lock().expect("OSM-Cache").insert(key, name.clone());
    Ok(name)
}

/// Vorschlag für den Ort einer Aktivität. Äußeres `None`: Aktivität unbekannt; `Some(None)`: kein Vorschlag.
pub async fn suggest(cfg: &Validated, db: &Db, activity_id: &str) -> anyhow::Result<Option<Option<Value>>> {
    let exists: Option<String> =
        sqlx::query_scalar("SELECT id FROM activities WHERE id = ?").bind(activity_id).fetch_optional(&db.reader).await?;
    if exists.is_none() {
        return Ok(None);
    }
    let Some(start) = start_of(db, activity_id).await? else { return Ok(Some(None)) };
    if let Some((name, d)) = vote(start, &known_places(db, activity_id).await?) {
        return Ok(Some(Some(json!({ "location": name, "source": "own", "distance_m": d.round() }))));
    }
    let on: i64 = sqlx::query_scalar("SELECT osm_enabled FROM place_settings WHERE id = 1").fetch_one(&db.reader).await?;
    if on == 0 {
        return Ok(Some(None));
    }
    match osm(cfg, start).await {
        Ok(Some(name)) => Ok(Some(Some(json!({ "location": name, "source": "osm", "distance_m": null })))),
        Ok(None) => Ok(Some(None)),
        Err(e) => {
            // Kein Netz oder Dienst gestört: dann eben ohne Vorschlag
            tracing::warn!(error = %e, "Ortsvorschlag über OpenStreetMap fehlgeschlagen");
            Ok(Some(None))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(lat: f64, lon: f64) -> (i32, i32) {
        (deg_to_e7(lat), deg_to_e7(lon))
    }

    #[test]
    fn majority_wins_then_nearest() {
        let start = at(48.0, 8.0);
        // 0,001° Breite ≈ 111 m
        let places = vec![
            ("Nah".to_string(), at(48.001, 8.0)),
            ("Häufig".to_string(), at(48.005, 8.0)),
            ("Häufig".to_string(), at(48.006, 8.0)),
            ("Weit".to_string(), at(48.05, 8.0)),
        ];
        let (name, d) = vote(start, &places).unwrap();
        assert_eq!(name, "Häufig");
        assert!((d - 556.0).abs() < 5.0, "{d}");
        // Gleichstand: der nähere
        let (name, _) = vote(start, &places[..2]).unwrap();
        assert_eq!(name, "Nah");
        // Nichts im Umkreis
        assert!(vote(start, &places[3..]).is_none());
    }

    #[test]
    fn place_name_prefers_municipality() {
        let r: Reverse =
            serde_json::from_value(json!({ "address": { "hamlet": "Unterweiler", "village": "Musterdorf", "county": "Beispielkreis" } }))
                .unwrap();
        assert_eq!(place_name(&r).as_deref(), Some("Musterdorf"));
        let r: Reverse = serde_json::from_value(json!({ "address": { "suburb": "Altstadt" } })).unwrap();
        assert_eq!(place_name(&r).as_deref(), Some("Altstadt"));
        let r: Reverse = serde_json::from_value(json!({ "error": "Unable to geocode" })).unwrap();
        assert_eq!(place_name(&r), None);
    }
}
