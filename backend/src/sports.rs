//! Sportart-Zuordnung in der Datenbank (RFC-0008 Abschnitt 4): `source_sport` → Sportklasse und
//! Untertyp sowie der Untertyp-Katalog je Sportklasse. Die Sportklassen selbst stehen fest im Code.
//!
//! Untertypen haben wie die Sportklassen einen festen Schlüssel (`road`, `mtb` …), den die Oberfläche übersetzt.
//! Ein eigener Name (`name`) überschreibt die Übersetzung; eigene Einträge haben immer einen Namen und einen
//! erzeugten Schlüssel `custom_…`. Aktivitäten und Zuordnungen speichern den Schlüssel.
//!
//! Beim Start übernimmt [`ensure`] einmalig die alten TOML-Abschnitte und trägt danach alle eingebauten Vorgaben
//! ein, die noch nie angewendet wurden. Was der Benutzer ändert oder löscht, bringt ein Update nicht zurück.
//! Eine geänderte Zuordnung gilt nur für neue Importe; vorhandene Aktivitäten behalten ihre Werte.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::{Row, SqliteConnection};

use crate::activities::SPORT_CLASSES;
use crate::config::{self, ImportSection, Validated};
use crate::db::Db;

/// Sportklassen mit pflegbarem Untertyp-Katalog (alle außer `unknown`).
pub use crate::config::SPORT_CLASSES as CATALOG_CLASSES;

/// Höchstlänge eines `source_sport` in Zeichen.
pub const SOURCE_MAX_CHARS: usize = 64;
/// Höchstlänge eines eigenen Untertyp-Namens in Zeichen.
pub const SUB_SPORT_MAX_CHARS: usize = 40;

const TOML_TAKEOVER_KEY: &str = "toml-takeover";

/// Eingebauter Untertyp-Katalog `(Klasse, Schlüssel, deutsche Bezeichnung)`; Vorgaben-Schlüssel
/// `catalog:<klasse>:<schlüssel>`. Die Bezeichnung dient den Ersatztiteln beim Import und der TOML-Übernahme;
/// die Anzeige übersetzt die Oberfläche (`subSport.<schlüssel>`). Migration 7 kennt dieselbe Liste.
pub const DEFAULT_CATALOG: &[(&str, &str, &str)] = &[
    ("cycling", "road", "Rennrad"),
    ("cycling", "mtb", "MTB"),
    ("cycling", "indoor", "Indoor"),
    ("running", "street", "Straße"),
    ("running", "trail", "Trail"),
    ("running", "walking", "Gehen"),
    ("skiing", "nordic", "Nordic"),
    ("skiing", "skating", "Skating"),
    ("swimming", "indoor", "Indoor"),
    ("studio", "strength", "Krafttraining"),
];

/// Eingebaute Zuordnungen `(source_sport, Sportklasse, Untertyp-Schlüssel)`; Vorgaben-Schlüssel
/// `mapping:<source_sport>`. Ändert sich eine Vorgabe inhaltlich, braucht sie einen neuen Schlüssel.
const DEFAULT_MAPPINGS: &[(&str, &str, Option<&str>)] = &[
    ("cycling", "cycling", None),
    ("cycling-road", "cycling", Some("road")),
    ("cycling-mountain", "cycling", Some("mtb")),
    ("cycling-indoor_cycling", "cycling", Some("indoor")),
    ("e_biking", "ebike", None),
    ("mountain_biking", "cycling", Some("mtb")), // GPX von Strava
    ("mountain", "cycling", Some("mtb")),        // GPX von SportTracks
    ("road", "cycling", Some("road")),           // GPX von SportTracks
    ("running", "running", None),
    ("running-street", "running", Some("street")),
    ("running-trail", "running", Some("trail")),
    ("walking", "running", Some("walking")),
    ("hiking", "running", Some("walking")),
    ("cross_country_skiing", "skiing", Some("nordic")),
    ("cross_country_skiing-skate_skiing", "skiing", Some("skating")),
    ("swimming", "swimming", Some("indoor")),
    ("swimming-lap_swimming", "swimming", Some("indoor")),
    ("fitness_equipment", "studio", Some("strength")),
    ("training", "studio", Some("strength")),
    ("training-strength_training", "studio", Some("strength")),
    // Garmin-Typen aus dem Abruf; der Typ ersetzt dort den source_sport der Datei.
    ("road_biking", "cycling", Some("road")),
    ("gravel_cycling", "cycling", None),
    ("indoor_cycling", "cycling", Some("indoor")),
    ("virtual_ride", "cycling", Some("indoor")),
    ("e_bike_fitness", "ebike", None),
    ("e_bike_mountain", "ebike", None),
    ("trail_running", "running", Some("trail")),
    ("street_running", "running", Some("street")),
    ("treadmill_running", "running", None),
    ("cross_country_skiing_ws", "skiing", Some("nordic")),
    ("skate_skiing_ws", "skiing", Some("skating")),
    ("resort_skiing", "skiing", None),
    ("resort_skiing_snowboarding_ws", "skiing", None),
    ("backcountry_skiing", "skiing", None),
    ("lap_swimming", "swimming", Some("indoor")),
    ("open_water_swimming", "swimming", None),
    ("strength_training", "studio", Some("strength")),
];

/// Skating-Zuordnungen, die bis Schema 8 auf `nordic` zeigten. Der Start stellt sie einmalig um,
/// solange der Benutzer sie nicht geändert hat; vorhandene Aktivitäten behalten ihren Untertyp.
const SKATING_SOURCES: &[&str] = &["cross_country_skiing-skate_skiing", "skate_skiing_ws"];

/// Kennungen des festen Symbolsatzes; die Zeichnungen liegen in der Oberfläche.
pub const ICONS: &[&str] = &[
    "bike",
    "road_bike",
    "mtb",
    "indoor_bike",
    "ebike",
    "run",
    "street_run",
    "trail_run",
    "walk",
    "ski",
    "nordic",
    "skating",
    "swim",
    "pool",
    "studio",
    "strength",
    "unknown",
    "tag",
    "star",
    "mountain",
    "flag",
];

/// Die feste Schreibweise einer Sportklasse, `unknown` für alles andere.
pub fn static_class(class: &str) -> &'static str {
    SPORT_CLASSES.iter().copied().find(|c| *c == class).unwrap_or("unknown")
}

/// Deutsche Bezeichnung eines eingebauten Untertyps.
pub fn builtin_label(class: &str, key: &str) -> Option<&'static str> {
    DEFAULT_CATALOG.iter().find(|(c, k, _)| *c == class && *k == key).map(|(_, _, l)| *l)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub sport_class: &'static str,
    /// Schlüssel im Katalog der Klasse
    pub sub_sport: Option<String>,
}

/// Geladene Zuordnung für den Import, mit Bezeichnungen für Ersatztitel.
#[derive(Debug, Clone, Default)]
pub struct SportMapping {
    entries: BTreeMap<String, Entry>,
    labels: BTreeMap<(String, String), String>,
}

impl SportMapping {
    /// Nur die eingebauten Vorgaben (für Tests und Werkzeuge ohne Datenbank).
    pub fn defaults() -> Self {
        Self {
            entries: DEFAULT_MAPPINGS
                .iter()
                .map(|(s, c, k)| (s.to_string(), Entry { sport_class: static_class(c), sub_sport: k.map(str::to_string) }))
                .collect(),
            labels: DEFAULT_CATALOG.iter().map(|(c, k, l)| ((c.to_string(), k.to_string()), l.to_string())).collect(),
        }
    }

    /// Erst der volle Schlüssel mit Klasse und Untertyp, dann der Teil vor dem ersten `-` nur für die Klasse.
    pub fn resolve(&self, source: Option<&str>) -> Entry {
        let unknown = Entry { sport_class: "unknown", sub_sport: None };
        let Some(s) = source else { return unknown };
        if let Some(e) = self.entries.get(s) {
            return e.clone();
        }
        match s.split_once('-').and_then(|(sport, _)| self.entries.get(sport)) {
            Some(e) => Entry { sport_class: e.sport_class, sub_sport: None },
            None => unknown,
        }
    }

    /// Bezeichnung eines Untertyps für Ersatztitel: eigener Name, sonst die eingebaute deutsche Bezeichnung.
    pub fn label(&self, class: &str, key: &str) -> Option<&str> {
        self.labels.get(&(class.to_string(), key.to_string())).map(String::as_str)
    }
}

pub async fn load(db: &Db) -> anyhow::Result<SportMapping> {
    let rows = sqlx::query("SELECT source_sport, sport_class, sub_sport FROM sport_mappings").fetch_all(&db.reader).await?;
    let entries = rows
        .iter()
        .map(|r| {
            let class: String = r.get("sport_class");
            (r.get("source_sport"), Entry { sport_class: static_class(&class), sub_sport: r.get("sub_sport") })
        })
        .collect();
    let labels = list_sub_sports(db).await?.into_iter().filter_map(|s| s.label().map(|l| ((s.sport_class, s.key), l))).collect();
    Ok(SportMapping { entries, labels })
}

/// Katalogeintrag, wie `GET /config` ihn je Sportklasse liefert.
pub async fn catalog(db: &Db) -> anyhow::Result<BTreeMap<String, Vec<Value>>> {
    let mut out: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    for s in list_sub_sports(db).await? {
        out.entry(s.sport_class.clone()).or_default().push(json!({ "key": s.key, "name": s.name, "icon": s.icon, "builtin": s.builtin() }));
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------------------------
// Katalog lesen

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubSportRow {
    pub sport_class: String,
    pub key: String,
    /// eigener Name; bei eingebauten Einträgen nur nach Umbenennen gesetzt
    pub name: Option<String>,
    /// gewähltes Symbol; `None` = Vorgabe der Oberfläche
    pub icon: Option<String>,
    pub activity_count: i64,
    pub mapping_count: i64,
}

impl SubSportRow {
    pub fn builtin(&self) -> bool {
        builtin_label(&self.sport_class, &self.key).is_some()
    }

    /// Angezeigter Name in der Programmsprache (deutsch), für Prüfungen und Ersatztitel.
    pub fn label(&self) -> Option<String> {
        self.name.clone().or_else(|| builtin_label(&self.sport_class, &self.key).map(str::to_string))
    }

    pub fn to_json(&self) -> Value {
        json!({
            "sport_class": self.sport_class,
            "key": self.key,
            "name": self.name,
            "icon": self.icon,
            "builtin": self.builtin(),
            "activity_count": self.activity_count,
            "mapping_count": self.mapping_count,
        })
    }
}

const SUB_COLUMNS: &str = "SELECT s.sport_class, s.key, s.name, s.icon, \
     (SELECT count(*) FROM activities a WHERE a.sport_class = s.sport_class AND a.sub_sport = s.key) AS activity_count, \
     (SELECT count(*) FROM sport_mappings m WHERE m.sport_class = s.sport_class AND m.sub_sport = s.key) AS mapping_count \
     FROM sub_sports s";

fn sub_from_row(r: &sqlx::sqlite::SqliteRow) -> SubSportRow {
    SubSportRow {
        sport_class: r.get("sport_class"),
        key: r.get("key"),
        name: r.get("name"),
        icon: r.get("icon"),
        activity_count: r.get("activity_count"),
        mapping_count: r.get("mapping_count"),
    }
}

/// Katalog mit Verwendung, je Klasse in Anzeigereihenfolge.
pub async fn list_sub_sports(db: &Db) -> anyhow::Result<Vec<SubSportRow>> {
    let rows =
        sqlx::query(sqlx::AssertSqlSafe(format!("{SUB_COLUMNS} ORDER BY s.sport_class, s.position, s.key"))).fetch_all(&db.reader).await?;
    Ok(rows.iter().map(sub_from_row).collect())
}

async fn sub_row(conn: &mut SqliteConnection, class: &str, key: &str) -> sqlx::Result<Option<SubSportRow>> {
    let row = sqlx::query(sqlx::AssertSqlSafe(format!("{SUB_COLUMNS} WHERE s.sport_class = ? AND s.key = ?")))
        .bind(class)
        .bind(key)
        .fetch_optional(conn)
        .await?;
    Ok(row.as_ref().map(sub_from_row))
}

pub async fn sub_exists(conn: &mut SqliteConnection, class: &str, key: &str) -> sqlx::Result<bool> {
    Ok(sqlx::query_scalar::<_, i64>("SELECT 1 FROM sub_sports WHERE sport_class = ? AND key = ?")
        .bind(class)
        .bind(key)
        .fetch_optional(conn)
        .await?
        .is_some())
}

/// Schlüssel des Eintrags, dessen angezeigter Name (ohne Groß-/Kleinschreibung) `label` ist, außer `except`.
async fn key_for_label(conn: &mut SqliteConnection, class: &str, label: &str, except: Option<&str>) -> sqlx::Result<Option<String>> {
    let rows = sqlx::query("SELECT key, name FROM sub_sports WHERE sport_class = ?").bind(class).fetch_all(conn).await?;
    let wanted = label.to_lowercase();
    Ok(rows
        .iter()
        .map(|r| (r.get::<String, _>("key"), r.get::<Option<String>, _>("name")))
        .filter(|(k, _)| Some(k.as_str()) != except)
        .find(|(k, n)| n.clone().or_else(|| builtin_label(class, k).map(str::to_string)).is_some_and(|l| l.to_lowercase() == wanted))
        .map(|(k, _)| k))
}

/// Hängt einen Eintrag hinten an; `false`, wenn der Schlüssel schon im Katalog steht.
async fn insert_sub(conn: &mut SqliteConnection, class: &str, key: &str, name: Option<&str>) -> sqlx::Result<bool> {
    let r = sqlx::query(
        "INSERT OR IGNORE INTO sub_sports (sport_class, key, name, position) \
         VALUES (?1, ?2, ?3, (SELECT coalesce(max(position), -1) + 1 FROM sub_sports WHERE sport_class = ?1))",
    )
    .bind(class)
    .bind(key)
    .bind(name)
    .execute(conn)
    .await?;
    Ok(r.rows_affected() > 0)
}

fn custom_key() -> String {
    format!("custom_{}", uuid::Uuid::now_v7().simple())
}

// ---------------------------------------------------------------------------------------------------------
// Start: TOML-Übernahme und Vorgaben

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Ensured {
    pub toml_taken_over: bool,
    pub defaults_applied: usize,
}

async fn applied(conn: &mut SqliteConnection, key: &str) -> sqlx::Result<bool> {
    Ok(sqlx::query_scalar::<_, i64>("SELECT 1 FROM sport_defaults_applied WHERE key = ?").bind(key).fetch_optional(conn).await?.is_some())
}

async fn mark(conn: &mut SqliteConnection, key: &str, now: i64) -> sqlx::Result<()> {
    sqlx::query("INSERT OR IGNORE INTO sport_defaults_applied (key, applied_at_ms) VALUES (?, ?)")
        .bind(key)
        .bind(now)
        .execute(conn)
        .await?;
    Ok(())
}

fn has_legacy_sections(imp: &ImportSection) -> bool {
    !imp.sport_class_mapping.is_empty() || !imp.sub_sport_mapping.is_empty() || !imp.sub_sport_catalog.is_empty()
}

/// Untertyp aus einem TOML-Text: vorhandener Eintrag mit diesem Namen, sonst der eingebaute Eintrag mit dieser
/// deutschen Bezeichnung, sonst ein neuer eigener Eintrag.
async fn take_over_label(conn: &mut SqliteConnection, class: &str, label: &str) -> sqlx::Result<String> {
    if let Some(k) = key_for_label(conn, class, label, None).await? {
        return Ok(k);
    }
    if let Some((_, k, _)) = DEFAULT_CATALOG.iter().find(|(c, _, l)| *c == class && l.to_lowercase() == label.to_lowercase()) {
        if !insert_sub(conn, class, k, None).await? {
            // Schlüssel vorhanden, aber umbenannt: die TOML-Bezeichnung wird ein eigener Eintrag.
            let key = custom_key();
            insert_sub(conn, class, &key, Some(label)).await?;
            return Ok(key);
        }
        return Ok(k.to_string());
    }
    let key = custom_key();
    insert_sub(conn, class, &key, Some(label)).await?;
    Ok(key)
}

async fn upsert_mapping(conn: &mut SqliteConnection, source: &str, class: &str, sub: Option<&str>, now: i64) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO sport_mappings (source_sport, sport_class, sub_sport, created_at_ms, updated_at_ms) VALUES (?, ?, ?, ?, ?) \
         ON CONFLICT (source_sport) DO UPDATE SET sport_class = excluded.sport_class, sub_sport = excluded.sub_sport, \
         updated_at_ms = excluded.updated_at_ms",
    )
    .bind(source)
    .bind(class)
    .bind(sub)
    .bind(now)
    .bind(now)
    .execute(conn)
    .await?;
    Ok(())
}

/// Übernimmt die drei alten TOML-Abschnitte; ungültige Einträge werden mit Warnung übersprungen.
async fn take_over_toml(conn: &mut SqliteConnection, imp: &ImportSection, now: i64) -> sqlx::Result<()> {
    for (class, names) in &imp.sub_sport_catalog {
        if !CATALOG_CLASSES.contains(&class.as_str()) {
            tracing::warn!(class, "[import.sub_sport_catalog]: keine Sportklasse, Eintrag nicht übernommen");
            continue;
        }
        for name in names.iter().map(|n| n.trim()).filter(|n| !n.is_empty() && n.chars().count() <= SUB_SPORT_MAX_CHARS) {
            take_over_label(conn, class, name).await?;
        }
    }
    for (source, class) in &imp.sport_class_mapping {
        let source = source.trim();
        if source.is_empty() || !CATALOG_CLASSES.contains(&class.as_str()) {
            tracing::warn!(source, class, "[import.sport_class_mapping]: ungültiger Eintrag, nicht übernommen");
            continue;
        }
        upsert_mapping(conn, source, class, None, now).await?;
    }
    for (source, sub) in &imp.sub_sport_mapping {
        let source = source.trim();
        let Some(class) = config::sport_class_for(&imp.sport_class_mapping, source).filter(|c| CATALOG_CLASSES.contains(c)) else {
            tracing::warn!(source, "[import.sub_sport_mapping]: ergibt keine Sportklasse, nicht übernommen");
            continue;
        };
        let Some(key) = key_for_label(conn, class, sub.trim(), None).await? else {
            tracing::warn!(source, sub, class, "[import.sub_sport_mapping]: Untertyp nicht im Katalog, nicht übernommen");
            continue;
        };
        upsert_mapping(conn, source, class, Some(&key), now).await?;
    }
    Ok(())
}

/// Startschritt nach dem Profilabgleich: einmalige TOML-Übernahme, dann noch nicht angewendete Vorgaben.
pub async fn ensure(cfg: &Validated, db: &Db) -> anyhow::Result<Ensured> {
    let now = chrono::Utc::now().timestamp_millis();
    let imp = &cfg.raw.import;
    let mut out = Ensured::default();
    let mut tx = db.writer.begin().await?;

    if !applied(&mut tx, TOML_TAKEOVER_KEY).await? {
        if has_legacy_sections(imp) {
            take_over_toml(&mut tx, imp, now).await?;
            out.toml_taken_over = true;
        }
        mark(&mut tx, TOML_TAKEOVER_KEY, now).await?;
    } else if has_legacy_sections(imp) {
        tracing::info!(
            "Die Abschnitte [import.sport_class_mapping], [import.sub_sport_mapping] und [import.sub_sport_catalog] werden \
             nicht mehr gelesen; die Zuordnung steht in der Datenbank (Einstellungen → Sportarten). Bitte aus der TOML entfernen."
        );
    }

    for (class, key, _) in DEFAULT_CATALOG {
        let marker = format!("catalog:{class}:{key}");
        if !applied(&mut tx, &marker).await? {
            insert_sub(&mut tx, class, key, None).await?;
            mark(&mut tx, &marker, now).await?;
            out.defaults_applied += 1;
        }
    }
    for (source, class, sub) in DEFAULT_MAPPINGS {
        let marker = format!("mapping:{source}");
        if applied(&mut tx, &marker).await? {
            continue;
        }
        // Ein Untertyp, den der Benutzer aus dem Katalog gelöscht hat, bleibt weg; die Klasse kommt trotzdem.
        let sub = match sub {
            Some(k) if sub_exists(&mut tx, class, k).await? => Some(*k),
            _ => None,
        };
        sqlx::query(
            "INSERT OR IGNORE INTO sport_mappings (source_sport, sport_class, sub_sport, created_at_ms, updated_at_ms) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(source)
        .bind(class)
        .bind(sub)
        .bind(now)
        .bind(now)
        .execute(&mut *tx)
        .await?;
        mark(&mut tx, &marker, now).await?;
        out.defaults_applied += 1;
    }
    // Skating ist seit Schema 9 ein eigener Untertyp: alte Vorgaben umstellen, eigene Änderungen bleiben.
    for source in SKATING_SOURCES {
        let marker = format!("mapping-skating:{source}");
        if applied(&mut tx, &marker).await? {
            continue;
        }
        if sub_exists(&mut tx, "skiing", "skating").await? {
            sqlx::query(
                "UPDATE sport_mappings SET sub_sport = 'skating', updated_at_ms = ? \
                 WHERE source_sport = ? AND sport_class = 'skiing' AND sub_sport = 'nordic'",
            )
            .bind(now)
            .bind(source)
            .execute(&mut *tx)
            .await?;
        }
        mark(&mut tx, &marker, now).await?;
    }
    tx.commit().await?;
    Ok(out)
}

// ---------------------------------------------------------------------------------------------------------
// Pflege über die API

#[derive(Debug)]
pub enum Error {
    MappingNotFound,
    SubSportNotFound,
    Validation {
        field: &'static str,
        code: &'static str,
        message: &'static str,
    },
    /// Name steht (ohne Groß-/Kleinschreibung) schon im Katalog der Klasse
    NameTaken,
    /// Untertyp wird noch verwendet
    InUse {
        activity_count: i64,
        mapping_count: i64,
    },
    Internal(anyhow::Error),
}

impl From<anyhow::Error> for Error {
    fn from(e: anyhow::Error) -> Self {
        Self::Internal(e)
    }
}

impl From<sqlx::Error> for Error {
    fn from(e: sqlx::Error) -> Self {
        Self::Internal(e.into())
    }
}

fn invalid(field: &'static str, code: &'static str, message: &'static str) -> Error {
    Error::Validation { field, code, message }
}

pub fn check_source(source: &str) -> Result<&str, Error> {
    let s = source.trim();
    if s.is_empty() {
        return Err(invalid("source_sport", "must_not_be_empty", "source_sport must not be empty"));
    }
    if s.chars().count() > SOURCE_MAX_CHARS {
        return Err(invalid("source_sport", "too_long", "source_sport must have at most 64 characters"));
    }
    Ok(s)
}

fn check_catalog_class(class: &str) -> Result<&'static str, Error> {
    CATALOG_CLASSES.iter().copied().find(|c| *c == class).ok_or_else(|| invalid("sport_class", "invalid_value", "Unknown sport class"))
}

fn check_sub_name(name: &str) -> Result<String, Error> {
    let n = name.trim();
    if n.is_empty() {
        return Err(invalid("name", "must_not_be_empty", "Name must not be empty"));
    }
    if n.chars().count() > SUB_SPORT_MAX_CHARS {
        return Err(invalid("name", "too_long", "Name must have at most 40 characters"));
    }
    Ok(n.to_string())
}

/// Eine Zeile der Seite „Sportarten“: jeder `source_sport` mit Eintrag oder aus Importen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MappingRow {
    pub source_sport: String,
    /// eigener Eintrag, sonst `None`
    pub mapped: Option<Entry>,
    /// was ein neuer Import ergibt (mit Rückfall auf den Teil vor dem ersten `-`)
    pub effective: Entry,
    pub activity_count: i64,
}

impl MappingRow {
    pub fn to_json(&self) -> Value {
        json!({
            "source_sport": self.source_sport,
            "mapped": self.mapped.is_some(),
            "sport_class": self.mapped.as_ref().map(|e| e.sport_class),
            "sub_sport": self.mapped.as_ref().and_then(|e| e.sub_sport.clone()),
            "effective_sport_class": self.effective.sport_class,
            "effective_sub_sport": self.effective.sub_sport,
            "activity_count": self.activity_count,
        })
    }
}

/// Alle `source_sport` mit Eintrag oder aus vorhandenen Aktivitäten, alphabetisch.
pub async fn list_mappings(db: &Db) -> anyhow::Result<Vec<MappingRow>> {
    let mapping = load(db).await?;
    let counts: BTreeMap<String, i64> =
        sqlx::query("SELECT source_sport, count(*) AS n FROM activities WHERE source_sport IS NOT NULL GROUP BY source_sport")
            .fetch_all(&db.reader)
            .await?
            .iter()
            .map(|r| (r.get("source_sport"), r.get("n")))
            .collect();
    let mut keys: Vec<&String> = mapping.entries.keys().chain(counts.keys()).collect();
    keys.sort();
    keys.dedup();
    Ok(keys
        .into_iter()
        .map(|k| MappingRow {
            source_sport: k.clone(),
            mapped: mapping.entries.get(k).cloned(),
            effective: mapping.resolve(Some(k)),
            activity_count: counts.get(k).copied().unwrap_or(0),
        })
        .collect())
}

async fn mapping_row(db: &Db, source: &str) -> anyhow::Result<MappingRow> {
    let list = list_mappings(db).await?;
    list.into_iter().find(|r| r.source_sport == source).ok_or_else(|| anyhow::anyhow!("Zuordnung {source} fehlt nach dem Speichern"))
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MappingInput {
    #[serde(default)]
    pub sport_class: Option<String>,
    /// Schlüssel im Katalog der Klasse
    #[serde(default)]
    pub sub_sport: Option<String>,
}

/// Legt die Zuordnung eines `source_sport` an oder ersetzt sie.
pub async fn put_mapping(db: &Db, source: &str, input: &MappingInput) -> Result<MappingRow, Error> {
    let source = check_source(source)?;
    let class = input.sport_class.as_deref().ok_or_else(|| invalid("sport_class", "required", "sport_class is required"))?;
    if !SPORT_CLASSES.contains(&class) {
        return Err(invalid("sport_class", "invalid_value", "Unknown sport class"));
    }
    let sub = input.sub_sport.as_deref().filter(|s| !s.is_empty());
    let now = chrono::Utc::now().timestamp_millis();
    let mut tx = db.writer.begin().await?;
    if let Some(s) = sub
        && !sub_exists(&mut tx, class, s).await?
    {
        return Err(invalid("sub_sport", "not_in_catalog", "Sub sport is not in the catalog of this sport class"));
    }
    upsert_mapping(&mut tx, source, class, sub, now).await?;
    tx.commit().await?;
    Ok(mapping_row(db, source).await?)
}

/// `false`: kein eigener Eintrag. Danach gilt der Rückfall auf den Teil vor dem ersten `-` oder `unknown`.
pub async fn delete_mapping(db: &Db, source: &str) -> anyhow::Result<bool> {
    Ok(sqlx::query("DELETE FROM sport_mappings WHERE source_sport = ?").bind(source).execute(&db.writer).await?.rows_affected() > 0)
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubSportInput {
    #[serde(default)]
    pub sport_class: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    /// Symbol aus [`ICONS`]; ohne Angabe die Vorgabe
    #[serde(default)]
    pub icon: Option<String>,
}

fn check_icon(icon: &str) -> Result<&str, Error> {
    if ICONS.contains(&icon) { Ok(icon) } else { Err(invalid("icon", "invalid", "unknown icon")) }
}

/// Legt einen eigenen Untertyp mit erzeugtem Schlüssel an.
pub async fn create_sub_sport(db: &Db, input: &SubSportInput) -> Result<SubSportRow, Error> {
    let class =
        check_catalog_class(input.sport_class.as_deref().ok_or_else(|| invalid("sport_class", "required", "sport_class is required"))?)?;
    let name = check_sub_name(input.name.as_deref().ok_or_else(|| invalid("name", "required", "Name is required"))?)?;
    let icon = input.icon.as_deref().map(check_icon).transpose()?;
    let key = custom_key();
    let mut tx = db.writer.begin().await?;
    if key_for_label(&mut tx, class, &name, None).await?.is_some() {
        return Err(Error::NameTaken);
    }
    insert_sub(&mut tx, class, &key, Some(&name)).await?;
    if let Some(icon) = icon {
        sqlx::query("UPDATE sub_sports SET icon = ? WHERE sport_class = ? AND key = ?")
            .bind(icon)
            .bind(class)
            .bind(&key)
            .execute(&mut *tx)
            .await?;
    }
    let row = sub_row(&mut tx, class, &key).await?.ok_or_else(|| anyhow::anyhow!("Untertyp fehlt nach dem Anlegen"))?;
    tx.commit().await?;
    Ok(row)
}

/// Gibt einem Untertyp einen eigenen Namen; `None` setzt einen eingebauten auf die übersetzte Bezeichnung zurück.
/// Aktivitäten und Zuordnungen verweisen auf den Schlüssel und zeigen den neuen Namen ohne Änderung.
pub async fn rename_sub_sport(db: &Db, class: &str, key: &str, new_name: Option<&str>) -> Result<SubSportRow, Error> {
    update_sub_sport(db, class, key, Some(new_name), None).await
}

/// Ändert Name und/oder Symbol (`Some(None)` setzt jeweils auf die Vorgabe zurück, `None` lässt es unverändert).
pub async fn update_sub_sport(
    db: &Db,
    class: &str,
    key: &str,
    new_name: Option<Option<&str>>,
    new_icon: Option<Option<&str>>,
) -> Result<SubSportRow, Error> {
    let icon = match new_icon {
        Some(Some(i)) => Some(Some(check_icon(i)?)),
        other => other.map(|_| None),
    };
    let mut tx = db.writer.begin().await?;
    let Some(_) = sub_row(&mut tx, class, key).await? else {
        return Err(Error::SubSportNotFound);
    };
    if let Some(icon) = icon {
        sqlx::query("UPDATE sub_sports SET icon = ? WHERE sport_class = ? AND key = ?")
            .bind(icon)
            .bind(class)
            .bind(key)
            .execute(&mut *tx)
            .await?;
    }
    let Some(new_name) = new_name else {
        let row = sub_row(&mut tx, class, key).await?.ok_or_else(|| anyhow::anyhow!("Untertyp fehlt nach dem Ändern"))?;
        tx.commit().await?;
        return Ok(row);
    };
    let name = new_name.map(check_sub_name).transpose()?;
    let label = match &name {
        Some(n) => n.clone(),
        None => match builtin_label(class, key) {
            Some(l) => l.to_string(),
            None => return Err(invalid("name", "required", "Name is required for custom sub sports")),
        },
    };
    if key_for_label(&mut tx, class, &label, Some(key)).await?.is_some() {
        return Err(Error::NameTaken);
    }
    sqlx::query("UPDATE sub_sports SET name = ? WHERE sport_class = ? AND key = ?")
        .bind(&name)
        .bind(class)
        .bind(key)
        .execute(&mut *tx)
        .await?;
    let row = sub_row(&mut tx, class, key).await?.ok_or_else(|| anyhow::anyhow!("Untertyp fehlt nach dem Umbenennen"))?;
    tx.commit().await?;
    Ok(row)
}

/// Löscht einen Untertyp, solange ihn keine Aktivität und keine Zuordnung verwendet (Uli 14.11.2026).
pub async fn delete_sub_sport(db: &Db, class: &str, key: &str) -> Result<(), Error> {
    let mut tx = db.writer.begin().await?;
    let Some(row) = sub_row(&mut tx, class, key).await? else {
        return Err(Error::SubSportNotFound);
    };
    if row.activity_count > 0 || row.mapping_count > 0 {
        return Err(Error::InUse { activity_count: row.activity_count, mapping_count: row.mapping_count });
    }
    sqlx::query("DELETE FROM sub_sports WHERE sport_class = ? AND key = ?").bind(class).bind(key).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_exact_then_prefix() {
        let m = SportMapping::defaults();
        assert_eq!(m.resolve(Some("cycling-road")), Entry { sport_class: "cycling", sub_sport: Some("road".into()) });
        assert_eq!(m.resolve(Some("cycling-gravel")), Entry { sport_class: "cycling", sub_sport: None });
        assert_eq!(m.resolve(Some("walking")), Entry { sport_class: "running", sub_sport: Some("walking".into()) });
        assert_eq!(m.resolve(Some("rowing")).sport_class, "unknown");
        assert_eq!(m.resolve(None).sport_class, "unknown");
        assert_eq!(m.label("running", "walking"), Some("Gehen"));
    }

    #[test]
    fn defaults_are_consistent() {
        for (source, class, sub) in DEFAULT_MAPPINGS {
            assert!(SPORT_CLASSES.contains(class), "{source}");
            if let Some(k) = sub {
                assert!(builtin_label(class, k).is_some(), "{source}: {k} fehlt im Katalog von {class}");
            }
        }
        for (class, key, _) in DEFAULT_CATALOG {
            assert!(CATALOG_CLASSES.contains(class));
            assert!(key.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'), "{key}");
            assert!(!key.starts_with("custom_"));
        }
        // Migration 7 wandelt Texte über dieselbe Liste um (Stand Schema 7; später ergänzte Einträge bringt `ensure`).
        let sql = include_str!("../migrations/0007_sport_mappings.sql");
        let later = [("skiing", "skating")];
        for (class, key, label) in DEFAULT_CATALOG.iter().filter(|(c, k, _)| !later.contains(&(*c, *k))) {
            assert!(sql.contains(&format!("('{class}', '{key}', '{label}',")), "Migration 7 kennt {class}/{key} nicht");
        }
    }
}
