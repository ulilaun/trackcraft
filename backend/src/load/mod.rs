//! Belastung und Form (E7): HF-Zonensätze, Belastung je Aktivität (Banister-TRIMP) und Formkurve
//! (Fitness, Ermüdung, Form) mit einfacher Vorschau.
//!
//! Die Belastung ist ein abgeleiteter Wert in `activity_loads`. Jede Zeile trägt einen Hash ihrer Eingaben; weicht er
//! vom erwarteten ab (neuer Zonensatz, geänderte Faktoren, geänderte Aktivität, neue Formelversion), rechnet
//! [`refresh`] sie neu. Das läuft im Hintergrund ([`worker`]) nach dem Start, nach Änderungen und regelmäßig.

pub mod model;

use std::collections::{BTreeMap, HashMap};
use std::time::Duration;

use chrono::{Datelike, NaiveDate};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::Row;
use tokio::sync::{Mutex, Notify};

use crate::activities::{SPORT_CLASSES, nullable};
use crate::config::Validated;
use crate::db::{Db, bump_revision, new_id};
use model::{Computed, DayLoad, DayValue, Model, ZONES, Zones};

/// Weckt den Hintergrund nach Änderungen.
static WAKE: Notify = Notify::const_new();
/// Nur eine Neuberechnung gleichzeitig.
static RUNNING: Mutex<()> = Mutex::const_new(());

/// Abstand, in dem der Hintergrund ohne Anstoß nach fehlenden Werten sieht (neue Importe).
const POLL: Duration = Duration::from_secs(30);
/// Zeilen je Schreibtransaktion
const BATCH: usize = 50;

pub fn wake() {
    WAKE.notify_one();
}

#[derive(Debug)]
pub enum Error {
    NotFound,
    Conflict,
    Validation { field: &'static str, code: &'static str, message: &'static str },
    Internal(anyhow::Error),
}

impl<E: Into<anyhow::Error>> From<E> for Error {
    fn from(e: E) -> Self {
        Error::Internal(e.into())
    }
}

fn invalid(field: &'static str, code: &'static str, message: &'static str) -> Error {
    Error::Validation { field, code, message }
}

// ---------------------------------------------------------------------------------------------------------
// Einstellungen

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub sex_factor: f64,
    pub fitness_days: u32,
    pub fatigue_days: u32,
    pub estimate_per_hour: BTreeMap<String, f64>,
    /// `standard` oder `two_curve`
    pub curve_model: String,
    pub aerobic_days: u32,
    pub anaerobic_days: u32,
    pub rest_bonus_pct: u32,
    pub foster_surcharge: bool,
    pub foster_surcharge_pct: u32,
    /// Form = Fitness − Gewicht × Ermüdung (#24)
    pub fatigue_weight: f64,
}

/// Erlaubter Bereich des Ermüdungsgewichts.
pub const FATIGUE_WEIGHT_RANGE: (f64, f64) = (0.25, 3.0);

pub const CURVE_MODELS: [&str; 2] = ["standard", "two_curve"];

impl Settings {
    pub fn to_json(&self) -> Value {
        json!({
            "sex_factor": self.sex_factor,
            "fitness_days": self.fitness_days,
            "fatigue_days": self.fatigue_days,
            "estimate_per_hour": self.estimate_per_hour,
            "curve_model": self.curve_model,
            "aerobic_days": self.aerobic_days,
            "anaerobic_days": self.anaerobic_days,
            "rest_bonus_pct": self.rest_bonus_pct,
            "foster_surcharge": self.foster_surcharge,
            "foster_surcharge_pct": self.foster_surcharge_pct,
            "fatigue_weight": self.fatigue_weight,
        })
    }

    fn per_hour(&self, class: &str) -> f64 {
        self.estimate_per_hour.get(class).copied().unwrap_or(0.0)
    }

    /// Parameter der Formkurve aus den Einstellungen.
    pub fn curve(&self) -> model::CurveParams {
        model::CurveParams {
            fitness_days: self.fitness_days as f64,
            fatigue_weight: self.fatigue_weight,
            model: if self.curve_model == "two_curve" {
                model::CurveModel::TwoCurve {
                    aerobic_days: self.aerobic_days as f64,
                    anaerobic_days: self.anaerobic_days as f64,
                    rest_bonus: self.rest_bonus_pct as f64 / 100.0,
                }
            } else {
                model::CurveModel::Standard { fatigue_days: self.fatigue_days as f64 }
            },
            foster_surcharge: self.foster_surcharge.then_some(self.foster_surcharge_pct as f64 / 100.0),
        }
    }
}

pub const SEX_FACTORS: [f64; 2] = [1.92, 1.67];

pub async fn settings(db: &Db) -> anyhow::Result<Settings> {
    let r = sqlx::query(
        "SELECT sex_factor, fitness_days, fatigue_days, estimate_per_hour, curve_model, aerobic_days, anaerobic_days, \
         rest_bonus_pct, foster_surcharge, foster_surcharge_pct, fatigue_weight FROM load_settings WHERE id = 1",
    )
    .fetch_one(&db.reader)
    .await?;
    let est: String = r.get("estimate_per_hour");
    let mut estimate_per_hour: BTreeMap<String, f64> = serde_json::from_str(&est)?;
    for c in SPORT_CLASSES {
        estimate_per_hour.entry((*c).to_string()).or_insert(0.0);
    }
    let int = |k: &str| r.get::<i64, _>(k) as u32;
    Ok(Settings {
        sex_factor: r.get("sex_factor"),
        fitness_days: int("fitness_days"),
        fatigue_days: int("fatigue_days"),
        estimate_per_hour,
        curve_model: r.get("curve_model"),
        aerobic_days: int("aerobic_days"),
        anaerobic_days: int("anaerobic_days"),
        rest_bonus_pct: int("rest_bonus_pct"),
        foster_surcharge: r.get::<i64, _>("foster_surcharge") == 1,
        foster_surcharge_pct: int("foster_surcharge_pct"),
        fatigue_weight: r.get("fatigue_weight"),
    })
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SettingsInput {
    pub sex_factor: Option<f64>,
    pub fitness_days: Option<u32>,
    pub fatigue_days: Option<u32>,
    /// Nur die genannten Sportklassen ändern sich.
    pub estimate_per_hour: Option<BTreeMap<String, f64>>,
    pub curve_model: Option<String>,
    pub aerobic_days: Option<u32>,
    pub anaerobic_days: Option<u32>,
    pub rest_bonus_pct: Option<u32>,
    pub foster_surcharge: Option<bool>,
    pub foster_surcharge_pct: Option<u32>,
    pub fatigue_weight: Option<f64>,
}

fn in_range(field: &'static str, v: Option<u32>, lo: u32, hi: u32, message: &'static str) -> Result<Option<u32>, Error> {
    match v {
        Some(d) if !(lo..=hi).contains(&d) => Err(invalid(field, "out_of_range", message)),
        _ => Ok(v),
    }
}

pub async fn update_settings(db: &Db, input: &SettingsInput) -> Result<Settings, Error> {
    let old = settings(db).await?;
    let mut new = old.clone();
    if let Some(f) = input.sex_factor {
        if !SEX_FACTORS.iter().any(|v| (v - f).abs() < 1e-9) {
            return Err(invalid("sex_factor", "invalid", "Factor must be 1.92 or 1.67"));
        }
        new.sex_factor = f;
    }
    if let Some(d) = in_range("fitness_days", input.fitness_days, 7, 120, "Fitness time constant must be 7 to 120 days")? {
        new.fitness_days = d;
    }
    if let Some(d) = in_range("fatigue_days", input.fatigue_days, 2, 30, "Fatigue time constant must be 2 to 30 days")? {
        new.fatigue_days = d;
    }
    if let Some(d) = in_range("aerobic_days", input.aerobic_days, 2, 30, "Aerobic fatigue time constant must be 2 to 30 days")? {
        new.aerobic_days = d;
    }
    if let Some(d) = in_range("anaerobic_days", input.anaerobic_days, 2, 30, "Anaerobic fatigue time constant must be 2 to 30 days")? {
        new.anaerobic_days = d;
    }
    if let Some(p) = in_range("rest_bonus_pct", input.rest_bonus_pct, 0, 50, "Rest day bonus must be 0 to 50 percent")? {
        new.rest_bonus_pct = p;
    }
    if let Some(p) = in_range("foster_surcharge_pct", input.foster_surcharge_pct, 1, 50, "Surcharge must be 1 to 50 percent")? {
        new.foster_surcharge_pct = p;
    }
    if let Some(w) = input.fatigue_weight {
        let (lo, hi) = FATIGUE_WEIGHT_RANGE;
        if !w.is_finite() || !(lo..=hi).contains(&w) {
            return Err(invalid("fatigue_weight", "out_of_range", "Fatigue weight must be 0.25 to 3"));
        }
        // Zwei Nachkommastellen genügen, die Oberfläche zeigt nicht mehr
        new.fatigue_weight = (w * 100.0).round() / 100.0;
    }
    if let Some(b) = input.foster_surcharge {
        new.foster_surcharge = b;
    }
    if let Some(m) = &input.curve_model {
        if !CURVE_MODELS.contains(&m.as_str()) {
            return Err(invalid("curve_model", "invalid", "Curve model must be standard or two_curve"));
        }
        new.curve_model = m.clone();
    }
    if new.fatigue_days >= new.fitness_days {
        return Err(invalid("fatigue_days", "not_below_fitness", "Fatigue must decay faster than fitness"));
    }
    if new.aerobic_days.max(new.anaerobic_days) >= new.fitness_days {
        return Err(invalid("anaerobic_days", "not_below_fitness", "Fatigue must decay faster than fitness"));
    }
    if let Some(m) = &input.estimate_per_hour {
        for (k, v) in m {
            if !SPORT_CLASSES.contains(&k.as_str()) {
                return Err(invalid("estimate_per_hour", "unknown_sport_class", "Unknown sport class"));
            }
            if !(0.0..=300.0).contains(v) || !v.is_finite() {
                return Err(invalid("estimate_per_hour", "out_of_range", "Load per hour must be 0 to 300"));
            }
            new.estimate_per_hour.insert(k.clone(), *v);
        }
    }
    if new == old {
        return Ok(old);
    }
    let now = chrono::Utc::now().timestamp_millis();
    let mut tx = db.writer.begin().await?;
    sqlx::query(
        "UPDATE load_settings SET sex_factor = ?, fitness_days = ?, fatigue_days = ?, estimate_per_hour = ?, curve_model = ?, \
         aerobic_days = ?, anaerobic_days = ?, rest_bonus_pct = ?, foster_surcharge = ?, foster_surcharge_pct = ?, fatigue_weight = ?, updated_at_ms = ? \
         WHERE id = 1",
    )
    .bind(new.sex_factor)
    .bind(new.fitness_days as i64)
    .bind(new.fatigue_days as i64)
    .bind(serde_json::to_string(&new.estimate_per_hour)?)
    .bind(&new.curve_model)
    .bind(new.aerobic_days as i64)
    .bind(new.anaerobic_days as i64)
    .bind(new.rest_bonus_pct as i64)
    .bind(new.foster_surcharge as i64)
    .bind(new.foster_surcharge_pct as i64)
    .bind(new.fatigue_weight)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    bump_revision(&mut tx, now).await?;
    tx.commit().await?;
    wake();
    Ok(new)
}

// ---------------------------------------------------------------------------------------------------------
// Zonensätze

#[derive(Debug, Clone, PartialEq)]
pub struct ZoneSet {
    pub id: String,
    /// `None` = allgemeiner Satz
    pub sport_class: Option<String>,
    pub valid_from: NaiveDate,
    pub zones: Zones,
}

impl ZoneSet {
    /// Eingaben, die die Belastung bestimmen (für den Hash).
    fn fingerprint(&self) -> Value {
        let z = &self.zones;
        json!([self.id, z.max_hr, z.resting_hr, z.threshold_hr, z.model.as_str(), z.bounds])
    }

    pub fn to_json(&self) -> Value {
        let z = &self.zones;
        let lower = z.lower_bpm();
        let ranges: Vec<Value> =
            (0..ZONES).map(|i| json!([lower[i], if i + 1 < ZONES { Some(lower[i + 1].saturating_sub(1)) } else { None }])).collect();
        json!({
            "id": self.id,
            "sport_class": self.sport_class,
            "valid_from": self.valid_from.to_string(),
            "max_hr": z.max_hr,
            "resting_hr": z.resting_hr,
            "threshold_hr": z.threshold_hr,
            "model": z.model.as_str(),
            "bounds": z.bounds,
            "zones_bpm": ranges,
            "anaerobic_from_bpm": z.anaerobic_from(),
        })
    }
}

fn set_from_row(r: &sqlx::sqlite::SqliteRow) -> anyhow::Result<ZoneSet> {
    let bounds: String = r.get("bounds");
    let model: String = r.get("model");
    let from: String = r.get("valid_from");
    Ok(ZoneSet {
        id: r.get("id"),
        sport_class: r.get("sport_class"),
        valid_from: from.parse()?,
        zones: Zones {
            max_hr: r.get::<i64, _>("max_hr") as u32,
            resting_hr: r.get::<i64, _>("resting_hr") as u32,
            threshold_hr: r.get::<Option<i64>, _>("threshold_hr").map(|v| v as u32),
            model: Model::parse(&model).ok_or_else(|| anyhow::anyhow!("Zonenmodell {model} unbekannt"))?,
            bounds: serde_json::from_str(&bounds)?,
        },
    })
}

/// Alle Sätze, sortiert nach Sportklasse (allgemein zuerst) und Datum.
pub async fn zone_sets(db: &Db) -> anyhow::Result<Vec<ZoneSet>> {
    let rows = sqlx::query(
        "SELECT id, sport_class, valid_from, max_hr, resting_hr, threshold_hr, model, bounds FROM hr_zone_sets \
         ORDER BY sport_class IS NOT NULL, sport_class, valid_from",
    )
    .fetch_all(&db.reader)
    .await?;
    rows.iter().map(set_from_row).collect()
}

/// Gültiger Satz für eine Aktivität: eigener Satz der Sportklasse ab seinem Datum, sonst der allgemeine;
/// vor dem ältesten allgemeinen Satz gilt dieser rückwirkend. `sets` wie von [`zone_sets`] sortiert.
pub fn applicable<'a>(sets: &'a [ZoneSet], class: &str, day: NaiveDate) -> Option<&'a ZoneSet> {
    let own = sets.iter().rfind(|s| s.sport_class.as_deref() == Some(class) && s.valid_from <= day);
    own.or_else(|| {
        let general: Vec<&ZoneSet> = sets.iter().filter(|s| s.sport_class.is_none()).collect();
        general.iter().rev().find(|s| s.valid_from <= day).or(general.first()).copied()
    })
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ZoneSetInput {
    #[serde(default, deserialize_with = "nullable")]
    pub sport_class: Option<Option<String>>,
    pub valid_from: Option<String>,
    pub max_hr: Option<u32>,
    pub resting_hr: Option<u32>,
    #[serde(default, deserialize_with = "nullable")]
    pub threshold_hr: Option<Option<u32>>,
    pub model: Option<String>,
    pub bounds: Option<Vec<u32>>,
}

fn check_set(sport_class: &Option<String>, z: &Zones) -> Result<(), Error> {
    if let Some(c) = sport_class
        && (!SPORT_CLASSES.contains(&c.as_str()) || c == "unknown")
    {
        return Err(invalid("sport_class", "invalid", "Unknown sport class"));
    }
    if !(100..=240).contains(&z.max_hr) {
        return Err(invalid("max_hr", "out_of_range", "Maximum heart rate must be 100 to 240"));
    }
    if !(25..=120).contains(&z.resting_hr) || z.resting_hr + 20 > z.max_hr {
        return Err(invalid("resting_hr", "out_of_range", "Resting heart rate must be 25 to 120 and well below the maximum"));
    }
    if let Some(t) = z.threshold_hr
        && (t <= z.resting_hr || t > z.max_hr || t < 60)
    {
        return Err(invalid("threshold_hr", "out_of_range", "Threshold heart rate must lie between resting and maximum"));
    }
    if z.model == Model::Threshold && z.threshold_hr.is_none() {
        return Err(invalid("threshold_hr", "required", "This zone model needs a threshold heart rate"));
    }
    if z.bounds.windows(2).any(|w| w[0] >= w[1]) || z.bounds.iter().any(|b| !(30..=120).contains(b)) {
        return Err(invalid("bounds", "invalid", "Zone bounds must rise from 30 to 120 percent"));
    }
    Ok(())
}

fn parse_day(field: &'static str, s: &str) -> Result<NaiveDate, Error> {
    NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").map_err(|_| invalid(field, "invalid", "Date must be YYYY-MM-DD"))
}

fn bounds_of(v: &[u32]) -> Result<[u32; ZONES], Error> {
    v.try_into().map_err(|_| invalid("bounds", "invalid", "Exactly five zone bounds are required"))
}

async fn save_set(db: &Db, set: &ZoneSet, insert: bool) -> Result<(), Error> {
    let now = chrono::Utc::now().timestamp_millis();
    let z = &set.zones;
    let mut tx = db.writer.begin().await?;
    let taken: Option<String> =
        sqlx::query_scalar("SELECT id FROM hr_zone_sets WHERE coalesce(sport_class, '') = coalesce(?, '') AND valid_from = ? AND id <> ?")
            .bind(&set.sport_class)
            .bind(set.valid_from.to_string())
            .bind(&set.id)
            .fetch_optional(&mut *tx)
            .await?;
    if taken.is_some() {
        return Err(Error::Conflict);
    }
    // Gleiche Spaltenfolge für beide Fälle; beim Einfügen ist `created_at_ms` gleich `updated_at_ms`.
    let sql = if insert {
        "INSERT INTO hr_zone_sets (sport_class, valid_from, max_hr, resting_hr, threshold_hr, model, bounds, updated_at_ms, created_at_ms, id) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8, ?9)"
    } else {
        "UPDATE hr_zone_sets SET sport_class = ?1, valid_from = ?2, max_hr = ?3, resting_hr = ?4, threshold_hr = ?5, model = ?6, \
         bounds = ?7, updated_at_ms = ?8 WHERE id = ?9"
    };
    sqlx::query(sql)
        .bind(&set.sport_class)
        .bind(set.valid_from.to_string())
        .bind(z.max_hr as i64)
        .bind(z.resting_hr as i64)
        .bind(z.threshold_hr.map(|v| v as i64))
        .bind(z.model.as_str())
        .bind(serde_json::to_string(&z.bounds)?)
        .bind(now)
        .bind(&set.id)
        .execute(&mut *tx)
        .await?;
    bump_revision(&mut tx, now).await?;
    tx.commit().await?;
    wake();
    Ok(())
}

pub async fn create_set(db: &Db, input: &ZoneSetInput) -> Result<ZoneSet, Error> {
    let valid_from = parse_day("valid_from", input.valid_from.as_deref().ok_or(invalid("valid_from", "required", "Date is required"))?)?;
    let model = match input.model.as_deref() {
        Some(m) => Model::parse(m).ok_or(invalid("model", "invalid", "Unknown zone model"))?,
        None => Model::Reserve,
    };
    let zones = Zones {
        max_hr: input.max_hr.ok_or(invalid("max_hr", "required", "Maximum heart rate is required"))?,
        resting_hr: input.resting_hr.ok_or(invalid("resting_hr", "required", "Resting heart rate is required"))?,
        threshold_hr: input.threshold_hr.flatten(),
        model,
        bounds: match &input.bounds {
            Some(b) => bounds_of(b)?,
            None => model.default_bounds(),
        },
    };
    let set = ZoneSet { id: new_id(), sport_class: input.sport_class.clone().flatten(), valid_from, zones };
    check_set(&set.sport_class, &set.zones)?;
    save_set(db, &set, true).await?;
    Ok(set)
}

pub async fn update_set(db: &Db, id: &str, input: &ZoneSetInput) -> Result<ZoneSet, Error> {
    let old = zone_sets(db).await?.into_iter().find(|s| s.id == id).ok_or(Error::NotFound)?;
    let mut set = old.clone();
    if let Some(c) = &input.sport_class {
        set.sport_class = c.clone();
    }
    if let Some(d) = &input.valid_from {
        set.valid_from = parse_day("valid_from", d)?;
    }
    if let Some(v) = input.max_hr {
        set.zones.max_hr = v;
    }
    if let Some(v) = input.resting_hr {
        set.zones.resting_hr = v;
    }
    if let Some(v) = input.threshold_hr {
        set.zones.threshold_hr = v;
    }
    if let Some(m) = &input.model {
        let m = Model::parse(m).ok_or(invalid("model", "invalid", "Unknown zone model"))?;
        if m != set.zones.model && input.bounds.is_none() {
            set.zones.bounds = m.default_bounds();
        }
        set.zones.model = m;
    }
    if let Some(b) = &input.bounds {
        set.zones.bounds = bounds_of(b)?;
    }
    check_set(&set.sport_class, &set.zones)?;
    if set != old {
        save_set(db, &set, false).await?;
    }
    Ok(set)
}

pub async fn delete_set(db: &Db, id: &str) -> Result<(), Error> {
    let now = chrono::Utc::now().timestamp_millis();
    let mut tx = db.writer.begin().await?;
    let n = sqlx::query("DELETE FROM hr_zone_sets WHERE id = ?").bind(id).execute(&mut *tx).await?.rows_affected();
    if n == 0 {
        return Err(Error::NotFound);
    }
    bump_revision(&mut tx, now).await?;
    tx.commit().await?;
    wake();
    Ok(())
}

/// Vorschlag für die maximale HF aus dem Geburtstag im Profil.
pub fn suggestion(birthday: Option<NaiveDate>, today: NaiveDate) -> Value {
    match birthday {
        Some(b) => {
            let mut age = today.year() - b.year();
            if (today.month(), today.day()) < (b.month(), b.day()) {
                age -= 1;
            }
            let age = age.max(0) as u32;
            json!({ "age": age, "max_hr": model::tanaka(age) })
        }
        None => json!({ "age": null, "max_hr": null }),
    }
}

// ---------------------------------------------------------------------------------------------------------
// Belastung je Aktivität

struct ActivityRow {
    id: String,
    class: String,
    day: NaiveDate,
    total_ms: i64,
    avg_hr: Option<u32>,
}

enum Plan<'a> {
    Hr(&'a ZoneSet),
    Estimate,
}

struct Todo<'a> {
    row: ActivityRow,
    plan: Plan<'a>,
    hash: String,
}

fn hash(v: &Value) -> String {
    Sha256::digest(v.to_string().as_bytes()).iter().take(16).map(|b| format!("{b:02x}")).collect()
}

async fn activity_rows(cfg: &Validated, db: &Db) -> anyhow::Result<Vec<(ActivityRow, i64)>> {
    let rows = sqlx::query("SELECT id, sport_class, start_time_ms, total_time_ms, avg_heart_rate_bpm, revision FROM activities")
        .fetch_all(&db.reader)
        .await?;
    Ok(rows
        .iter()
        .map(|r| {
            let start: i64 = r.get("start_time_ms");
            let day = chrono::DateTime::from_timestamp_millis(start).unwrap_or_default().with_timezone(&cfg.timezone).date_naive();
            (
                ActivityRow {
                    id: r.get("id"),
                    class: r.get("sport_class"),
                    day,
                    total_ms: r.get("total_time_ms"),
                    avg_hr: r.get::<Option<i64>, _>("avg_heart_rate_bpm").map(|v| v as u32),
                },
                r.get::<i64, _>("revision"),
            )
        })
        .collect())
}

/// Aktivitäten, deren Belastung fehlt oder nicht mehr zu den Eingaben passt.
async fn todo<'a>(cfg: &Validated, db: &Db, s: &Settings, sets: &'a [ZoneSet]) -> anyhow::Result<Vec<Todo<'a>>> {
    let stored: HashMap<String, String> = sqlx::query("SELECT activity_id, inputs_hash FROM activity_loads")
        .fetch_all(&db.reader)
        .await?
        .iter()
        .map(|r| (r.get("activity_id"), r.get("inputs_hash")))
        .collect();
    let mut out = Vec::new();
    for (row, revision) in activity_rows(cfg, db).await? {
        let set = row.avg_hr.and(applicable(sets, &row.class, row.day));
        let (plan, inputs) = match set {
            Some(z) => (Plan::Hr(z), json!(["hr", model::FORMULA_VERSION, revision, s.sex_factor, z.fingerprint()])),
            None => (Plan::Estimate, json!(["est", model::FORMULA_VERSION, revision, s.per_hour(&row.class)])),
        };
        let h = hash(&inputs);
        if stored.get(&row.id) != Some(&h) {
            out.push(Todo { row, plan, hash: h });
        }
    }
    Ok(out)
}

/// Zahl der Aktivitäten, deren Belastung noch (neu) berechnet werden muss.
pub async fn pending(cfg: &Validated, db: &Db) -> anyhow::Result<usize> {
    let s = settings(db).await?;
    let sets = zone_sets(db).await?;
    Ok(todo(cfg, db, &s, &sets).await?.len())
}

/// Rechnet alle fehlenden oder veralteten Belastungen; liefert die Zahl der berechneten Aktivitäten.
pub async fn refresh(cfg: &Validated, db: &Db) -> anyhow::Result<usize> {
    let _guard = RUNNING.lock().await;
    let s = settings(db).await?;
    let sets = zone_sets(db).await?;
    let list = todo(cfg, db, &s, &sets).await?;
    let total = list.len();
    let now = chrono::Utc::now().timestamp_millis();
    for chunk in list.chunks(BATCH) {
        let mut computed = Vec::with_capacity(chunk.len());
        {
            let mut conn = db.reader.acquire().await?;
            for t in chunk {
                let (method, set_id, c) = match &t.plan {
                    Plan::Hr(set) => {
                        let pts = crate::calculation::sections(&mut conn, &t.row.id).await?;
                        match model::from_samples(&pts, &set.zones, s.sex_factor) {
                            Some(c) => ("samples", Some(set.id.clone()), c),
                            None => (
                                "average",
                                Some(set.id.clone()),
                                model::from_average(t.row.avg_hr.unwrap_or(0), t.row.total_ms, &set.zones, s.sex_factor),
                            ),
                        }
                    }
                    Plan::Estimate => ("estimated", None, model::estimated(t.row.total_ms, s.per_hour(&t.row.class))),
                };
                computed.push((t, method, set_id, c));
            }
        }
        let mut tx = db.writer.begin().await?;
        for (t, method, set_id, c) in computed {
            sqlx::query(
                "INSERT OR REPLACE INTO activity_loads (activity_id, inputs_hash, method, zone_set_id, trimp, trimp_aerobic, \
                 trimp_anaerobic, zone_seconds, computed_at_ms) \
                 SELECT ?, ?, ?, ?, ?, ?, ?, ?, ? WHERE EXISTS (SELECT 1 FROM activities WHERE id = ?)",
            )
            .bind(&t.row.id)
            .bind(&t.hash)
            .bind(method)
            .bind(set_id)
            .bind(c.trimp)
            .bind(c.aerobic)
            .bind(c.anaerobic)
            .bind(c.zone_seconds.map(|z| serde_json::to_string(&z.map(|v| v.round() as i64)).expect("JSON")))
            .bind(now)
            .bind(&t.row.id)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
    }
    if total > 0 {
        tracing::info!(total, "Belastung berechnet");
    }
    Ok(total)
}

/// Läuft bis zum Abbruch: beim Start, nach Änderungen und alle [`POLL`] Sekunden.
pub async fn worker(cfg: std::sync::Arc<Validated>, db: Db) {
    loop {
        if let Err(e) = refresh(&cfg, &db).await {
            tracing::error!(error = format!("{e:#}"), "Belastung nicht berechenbar");
        }
        let _ = tokio::time::timeout(POLL, WAKE.notified()).await;
    }
}

fn computed_to_json(method: &str, c: &Computed) -> Value {
    json!({
        "trimp": round1(c.trimp),
        "aerobic": round1(c.aerobic),
        "anaerobic": round1(c.anaerobic),
        "method": method,
        "zone_seconds": c.zone_seconds.map(|z| z.map(|v| v.round() as i64)),
    })
}

fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

/// Belastung einer Aktivität für die Detailansicht, mit dem verwendeten Zonensatz; `null`, solange nicht berechnet.
pub async fn activity_load(db: &Db, id: &str) -> anyhow::Result<Value> {
    let Some(r) = sqlx::query(
        "SELECT method, zone_set_id, trimp, trimp_aerobic, trimp_anaerobic, zone_seconds FROM activity_loads WHERE activity_id = ?",
    )
    .bind(id)
    .fetch_optional(&db.reader)
    .await?
    else {
        return Ok(Value::Null);
    };
    let zs: Option<String> = r.get("zone_seconds");
    let zone_seconds: Option<[f64; ZONES]> =
        zs.map(|s| serde_json::from_str::<[i64; ZONES]>(&s).map(|a| a.map(|v| v as f64))).transpose()?;
    let c = Computed { trimp: r.get("trimp"), aerobic: r.get("trimp_aerobic"), anaerobic: r.get("trimp_anaerobic"), zone_seconds };
    let method: String = r.get("method");
    let mut v = computed_to_json(&method, &c);
    let set_id: Option<String> = r.get("zone_set_id");
    v["zone_set"] = match set_id {
        Some(sid) => zone_sets(db).await?.into_iter().find(|s| s.id == sid).map(|s| s.to_json()).unwrap_or(Value::Null),
        None => Value::Null,
    };
    Ok(v)
}

// ---------------------------------------------------------------------------------------------------------
// Formkurve

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Forecast {
    /// Ab morgen keine Belastung
    Rest,
    /// Mittlere Tagesbelastung der letzten 28 Tage
    Steady,
    /// Geplante Einheiten (E8), Tage ohne Einheit als Ruhe
    Plan,
}

impl Forecast {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "rest" => Some(Self::Rest),
            "steady" => Some(Self::Steady),
            "plan" => Some(Self::Plan),
            _ => None,
        }
    }
}

pub const FORECAST_MAX_DAYS: u32 = 90;
const STEADY_WINDOW: usize = 28;

/// Tagesbelastungen nach lokalem Datum.
async fn day_loads(cfg: &Validated, db: &Db) -> anyhow::Result<BTreeMap<NaiveDate, DayLoad>> {
    let rows = sqlx::query(
        "SELECT a.start_time_ms, l.method, l.trimp, l.trimp_aerobic, l.trimp_anaerobic \
         FROM activity_loads l JOIN activities a ON a.id = l.activity_id",
    )
    .fetch_all(&db.reader)
    .await?;
    let mut out: BTreeMap<NaiveDate, DayLoad> = BTreeMap::new();
    for r in rows {
        let start: i64 = r.get("start_time_ms");
        let day = chrono::DateTime::from_timestamp_millis(start).unwrap_or_default().with_timezone(&cfg.timezone).date_naive();
        let e = out.entry(day).or_default();
        let trimp: f64 = r.get("trimp");
        e.total += trimp;
        if r.get::<String, _>("method") == "estimated" {
            e.estimated += trimp;
        } else {
            e.aerobic += r.get::<f64, _>("trimp_aerobic");
            e.anaerobic += r.get::<f64, _>("trimp_anaerobic");
        }
    }
    Ok(out)
}

fn opt1(v: Option<f64>) -> Value {
    v.map(|x| json!((x * 100.0).round() / 100.0)).unwrap_or(Value::Null)
}

pub(crate) fn day_json(d: &DayValue, with_parts: bool) -> Value {
    let mut v = json!({
        "date": d.date.to_string(),
        "load": round1(d.load.total),
        "fitness": round1(d.fitness),
        "fatigue": round1(d.fatigue),
        "form": round1(d.form),
        "fatigue_aerobic": d.fatigue_parts.map(|p| round1(p.0)),
        "fatigue_anaerobic": d.fatigue_parts.map(|p| round1(p.1)),
        "monotony": opt1(d.monotony),
        "strain": d.strain.map(|s| s.round()),
    });
    if with_parts {
        v["aerobic"] = json!(round1(d.load.aerobic));
        v["anaerobic"] = json!(round1(d.load.anaerobic));
        v["estimated"] = json!(round1(d.load.estimated));
    }
    v
}

/// Zustand der Formkurve nach dem Tag `today` mit den Einstellungen (für die Planungshilfe, E8 Stufe 2).
pub async fn curve_at(cfg: &Validated, db: &Db, today: NaiveDate) -> anyhow::Result<(model::Curve, Settings)> {
    let s = settings(db).await?;
    let loads = day_loads(cfg, db).await?;
    let start = loads.keys().next().copied().unwrap_or(today).min(today);
    let (_, curve) = model::series(&loads, start, today, s.curve());
    Ok((curve, s))
}

pub struct FormQuery {
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
    pub forecast: Forecast,
    pub forecast_days: u32,
}

/// Formkurve im Zeitraum (Vorgabe: die letzten 12 Wochen), mit aktuellem Stand und Vorschau ab morgen.
pub async fn form(cfg: &Validated, db: &Db, q: &FormQuery, today: NaiveDate) -> anyhow::Result<Value> {
    let s = settings(db).await?;
    let loads = day_loads(cfg, db).await?;
    let start = loads.keys().next().copied().unwrap_or(today).min(today);
    let (all, curve) = model::series(&loads, start, today, s.curve());
    let to = q.to.unwrap_or(today).min(today);
    let from = q.from.unwrap_or(today - chrono::Days::new(83));
    let days: Vec<Value> = all.iter().filter(|d| d.date >= from && d.date <= to).map(|d| day_json(d, true)).collect();

    let last = all.last().expect("mindestens heute");
    // „Wie bisher“: mittlere Tagesbelastung der letzten 28 Tage, mit ihrer Aufteilung aerob/anaerob/geschätzt
    // Offene Einheiten ab morgen; das letzte Datum braucht die Seite auch ohne Vorschau „Plan“ (Knopf nur mit Plan)
    let (planned, last_planned) = crate::plan::day_loads(db, today, &s).await?;
    let goal = crate::plan::goal_date(db).await?;
    let daily = match q.forecast {
        Forecast::Rest | Forecast::Plan => DayLoad::default(),
        Forecast::Steady => {
            let w = STEADY_WINDOW as f64;
            let recent = all.iter().rev().take(STEADY_WINDOW);
            recent.fold(DayLoad::default(), |a, d| DayLoad {
                total: a.total + d.load.total / w,
                aerobic: a.aerobic + d.load.aerobic / w,
                anaerobic: a.anaerobic + d.load.anaerobic / w,
                estimated: a.estimated + d.load.estimated / w,
            })
        }
    };
    let day_load = |d: NaiveDate| if q.forecast == Forecast::Plan { planned.get(&d).copied().unwrap_or_default() } else { daily };
    // Plan: mindestens bis zum letzten geplanten Tag bzw. Ziel und ein paar Tage darüber hinaus, höchstens 90 Tage
    let horizon = if q.forecast == Forecast::Plan && q.forecast_days > 0 {
        let until = [last_planned, goal].into_iter().flatten().filter(|d| *d > today).max();
        let need = until.map(|d| (d - today).num_days() as u32 + 3).unwrap_or(0);
        q.forecast_days.max(need).min(FORECAST_MAX_DAYS)
    } else {
        q.forecast_days
    };
    let forecast: Vec<Value> = if q.to.is_none_or(|t| t >= today) {
        model::forecast_with(&curve, today, day_load, horizon).iter().map(|d| day_json(d, false)).collect()
    } else {
        Vec::new()
    };
    let n = all.len();
    let week = |a: usize, b: usize| all[n.saturating_sub(b)..n.saturating_sub(a)].iter().map(|d| d.load.total).sum::<f64>();
    let week_ago = &all[n.saturating_sub(8).min(n - 1)];
    // Kachelreihen der Formseite (#25): vor 30 und 7 Tagen sowie in 7 Tagen mit der gewählten Vorschau
    let month_ago = &all[n.saturating_sub(31).min(n - 1)];
    let ahead = model::forecast_with(&curve, today, day_load, 7);
    let snap = |d: &DayValue| {
        json!({
            "date": d.date.to_string(),
            "fitness": round1(d.fitness),
            "fatigue": round1(d.fatigue),
            "form": round1(d.form),
            "monotony": opt1(d.monotony),
            "strain": d.strain.map(|v| v.round()),
        })
    };
    let sets = zone_sets(db).await?;
    Ok(json!({
        "today": today.to_string(),
        "from": from.to_string(),
        "to": to.to_string(),
        "settings": s.to_json(),
        "zones_configured": !sets.is_empty(),
        "pending": todo(cfg, db, &s, &sets).await?.len(),
        "first_date": loads.keys().next().map(|d| d.to_string()),
        "monotony_warn": model::MONOTONY_WARN,
        "current": {
            "fitness": round1(last.fitness),
            "fatigue": round1(last.fatigue),
            "fatigue_aerobic": last.fatigue_parts.map(|p| round1(p.0)),
            "fatigue_anaerobic": last.fatigue_parts.map(|p| round1(p.1)),
            "form": round1(last.form),
            "load_7d": round1(week(0, 7)),
            "load_prev_7d": round1(week(7, 14)),
            "fitness_week_ago": round1(week_ago.fitness),
            "fatigue_week_ago": round1(week_ago.fatigue),
            "monotony": opt1(last.monotony),
            "strain": last.strain.map(|v| v.round()),
            "strain_week_ago": week_ago.strain.map(|v| v.round()),
            "compare": {
                "month_ago": snap(month_ago),
                "week_ago": snap(week_ago),
                "week_ahead": ahead.last().map(snap),
            },
        },
        "days": days,
        "forecast": {
            "mode": match q.forecast { Forecast::Rest => "rest", Forecast::Steady => "steady", Forecast::Plan => "plan" },
            "daily_load": round1(daily.total),
            "days": forecast,
        },
        "plan": { "goal": crate::plan::goal(db).await?, "last_date": last_planned.map(|d| d.to_string()) },
    }))
}
