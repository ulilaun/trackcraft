//! Trainingsplanung, Stufe 1 (E8, #12): geplante Einheiten mit Belastung, Zieltermin, Verknüpfen mit Aktivitäten
//! und die Tagesbelastung für die Vorschau „Plan“ der Formkurve (RFC-0009 „Trainingsplanung“).

use std::collections::BTreeMap;

use chrono::NaiveDate;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::{Row, SqliteConnection};

use crate::activities::nullable;
use crate::config::{SPORT_CLASSES, Validated};
use crate::db::{Db, bump_revision, new_id};
use crate::load::model::{self, Computed, DayLoad, ZONES};
use crate::load::{self, Settings, ZoneSet};
use crate::sports;

#[derive(Debug)]
pub enum Error {
    NotFound,
    ActivityNotFound,
    /// Die Aktivität ist schon mit einer anderen Einheit verknüpft.
    ActivityTaken,
    Validation {
        field: &'static str,
        code: &'static str,
        message: &'static str,
    },
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

const UNIT_COLUMNS: &str = "u.id, u.date, u.sport_class, u.sub_sport, u.title, u.note, u.mode, u.duration_min, u.target_load, \
    u.intensive_pct, u.zone_minutes, u.status, u.activity_id";

#[derive(Debug, Clone, PartialEq)]
pub struct Unit {
    pub id: String,
    pub date: NaiveDate,
    pub sport_class: String,
    pub sub_sport: Option<String>,
    pub title: String,
    pub note: Option<String>,
    pub mode: Mode,
    pub status: String,
    pub activity_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Mode {
    Target { minutes: u32, load: f64, intensive_pct: u32 },
    Zones([u32; ZONES]),
}

impl Unit {
    fn minutes(&self) -> u32 {
        match &self.mode {
            Mode::Target { minutes, .. } => *minutes,
            Mode::Zones(z) => z.iter().sum(),
        }
    }

    /// Belastung mit dem am Tag gültigen Zonensatz; Zonenminuten ohne Zonensatz werden aus der Dauer geschätzt.
    pub fn load(&self, sets: &[ZoneSet], s: &Settings) -> (Computed, &'static str) {
        match &self.mode {
            Mode::Target { load, intensive_pct, .. } => {
                let ana = load * *intensive_pct as f64 / 100.0;
                (Computed { trimp: *load, aerobic: load - ana, anaerobic: ana, zone_seconds: None }, "target")
            }
            Mode::Zones(z) => match load::applicable(sets, &self.sport_class, self.date) {
                Some(set) => (model::from_zone_minutes(&z.map(|m| m as f64), &set.zones, s.sex_factor), "zones"),
                None => {
                    let per_hour = s.estimate_per_hour.get(&self.sport_class).copied().unwrap_or(0.0);
                    (model::estimated(self.minutes() as i64 * 60_000, per_hour), "estimated")
                }
            },
        }
    }

    /// Erledigt = mit einer Aktivität verknüpft.
    fn effective_status(&self) -> &str {
        if self.activity_id.is_some() { "done" } else { &self.status }
    }

    fn to_json(&self, sets: &[ZoneSet], s: &Settings) -> Value {
        let (c, method) = self.load(sets, s);
        let (duration_min, target_load, intensive_pct, zone_minutes) = match &self.mode {
            Mode::Target { minutes, load, intensive_pct } => (Some(*minutes), Some(*load), *intensive_pct, None),
            Mode::Zones(z) => (None, None, 0, Some(*z)),
        };
        json!({
            "id": self.id,
            "date": self.date.to_string(),
            "sport_class": self.sport_class,
            "sub_sport": self.sub_sport,
            "title": self.title,
            "note": self.note,
            "mode": match self.mode { Mode::Target { .. } => "target", Mode::Zones(_) => "zones" },
            "duration_min": duration_min,
            "target_load": target_load,
            "intensive_pct": intensive_pct,
            "zone_minutes": zone_minutes,
            "status": self.effective_status(),
            "activity_id": self.activity_id,
            "load": {
                "total": round1(c.trimp),
                "aerobic": round1(c.aerobic),
                "anaerobic": round1(c.anaerobic),
                "minutes": self.minutes(),
                "method": method,
            },
        })
    }
}

fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

fn unit_from_row(r: &sqlx::sqlite::SqliteRow) -> anyhow::Result<Unit> {
    let date: String = r.get("date");
    let mode: String = r.get("mode");
    let mode = if mode == "zones" {
        let z: String = r.get("zone_minutes");
        Mode::Zones(serde_json::from_str(&z)?)
    } else {
        Mode::Target {
            minutes: r.get::<i64, _>("duration_min") as u32,
            load: r.get("target_load"),
            intensive_pct: r.get::<i64, _>("intensive_pct") as u32,
        }
    };
    Ok(Unit {
        id: r.get("id"),
        date: date.parse()?,
        sport_class: r.get("sport_class"),
        sub_sport: r.get("sub_sport"),
        title: r.get("title"),
        note: r.get("note"),
        mode,
        status: r.get("status"),
        activity_id: r.get("activity_id"),
    })
}

async fn units_where(db: &Db, cond: &str, binds: &[String]) -> anyhow::Result<Vec<Unit>> {
    let sql = format!("SELECT {UNIT_COLUMNS} FROM planned_units u WHERE {cond} ORDER BY u.date, u.created_at_ms, u.id");
    let mut q = sqlx::query(sqlx::AssertSqlSafe(sql));
    for b in binds {
        q = q.bind(b.clone());
    }
    q.fetch_all(&db.reader).await?.iter().map(unit_from_row).collect()
}

async fn unit(db: &Db, id: &str) -> anyhow::Result<Option<Unit>> {
    Ok(units_where(db, "u.id = ?", &[id.to_string()]).await?.into_iter().next())
}

/// Ist-Werte einer verknüpften Aktivität für den Vergleich Plan ↔ Ist.
async fn actual(db: &Db, activity_id: &str) -> anyhow::Result<Value> {
    let Some(r) = sqlx::query(
        "SELECT a.id, a.title, a.start_time_ms, a.total_time_ms, l.trimp, l.trimp_anaerobic, l.zone_seconds, l.method \
         FROM activities a LEFT JOIN activity_loads l ON l.activity_id = a.id WHERE a.id = ?",
    )
    .bind(activity_id)
    .fetch_optional(&db.reader)
    .await?
    else {
        return Ok(Value::Null);
    };
    let zs: Option<String> = r.get("zone_seconds");
    let zone_seconds: Option<Vec<i64>> = zs.map(|s| serde_json::from_str(&s)).transpose()?;
    Ok(json!({
        "activity_id": r.get::<String, _>("id"),
        "title": r.get::<String, _>("title"),
        "start_time": crate::import_job::timestamp(r.get("start_time_ms")),
        "total_time_ms": r.get::<i64, _>("total_time_ms"),
        "load": r.get::<Option<f64>, _>("trimp").map(round1),
        "anaerobic": r.get::<Option<f64>, _>("trimp_anaerobic").map(round1),
        "load_method": r.get::<Option<String>, _>("method"),
        "zone_seconds": zone_seconds,
    }))
}

async fn unit_json(db: &Db, u: &Unit, sets: &[ZoneSet], s: &Settings) -> anyhow::Result<Value> {
    let mut v = u.to_json(sets, s);
    v["actual"] = match &u.activity_id {
        Some(a) => actual(db, a).await?,
        None => Value::Null,
    };
    Ok(v)
}

// ---------------------------------------------------------------------------------------------------------
// Lesen

pub async fn goal(db: &Db) -> anyhow::Result<Value> {
    let r = sqlx::query("SELECT goal_date, goal_name FROM plan_settings WHERE id = 1").fetch_one(&db.reader).await?;
    let date: Option<String> = r.get("goal_date");
    Ok(match date {
        Some(d) => json!({ "date": d, "name": r.get::<String, _>("goal_name") }),
        None => Value::Null,
    })
}

/// Einheiten im Zeitraum (beide Grenzen eingeschlossen) mit Belastung und Ist-Werten, dazu das Ziel.
pub async fn list(db: &Db, from: NaiveDate, to: NaiveDate) -> anyhow::Result<Value> {
    let units = units_where(db, "u.date BETWEEN ? AND ?", &[from.to_string(), to.to_string()]).await?;
    let sets = load::zone_sets(db).await?;
    let s = load::settings(db).await?;
    let mut out = Vec::with_capacity(units.len());
    for u in &units {
        out.push(unit_json(db, u, &sets, &s).await?);
    }
    Ok(json!({ "units": out, "goal": goal(db).await?, "zones_configured": !sets.is_empty() }))
}

pub async fn get_one(db: &Db, id: &str) -> Result<Value, Error> {
    let u = unit(db, id).await?.ok_or(Error::NotFound)?;
    let sets = load::zone_sets(db).await?;
    let s = load::settings(db).await?;
    Ok(unit_json(db, &u, &sets, &s).await?)
}

/// Tagesbelastung der noch offenen Einheiten ab `after` (ausschließlich), für die Vorschau „Plan“. Tage ohne Einheit
/// fehlen und zählen damit als Ruhe (Uli 23.11.2026). Liefert auch den letzten geplanten Tag.
pub async fn day_loads(db: &Db, after: NaiveDate, s: &Settings) -> anyhow::Result<(BTreeMap<NaiveDate, DayLoad>, Option<NaiveDate>)> {
    let units = units_where(db, "u.date > ? AND u.status = 'planned' AND u.activity_id IS NULL", &[after.to_string()]).await?;
    let sets = load::zone_sets(db).await?;
    let mut out: BTreeMap<NaiveDate, DayLoad> = BTreeMap::new();
    for u in &units {
        let (c, method) = u.load(&sets, s);
        let e = out.entry(u.date).or_default();
        e.total += c.trimp;
        if method == "estimated" {
            e.estimated += c.trimp;
        } else {
            e.aerobic += c.aerobic;
            e.anaerobic += c.anaerobic;
        }
    }
    let last = out.keys().next_back().copied();
    Ok((out, last))
}

/// Zieltermin, falls gesetzt.
pub async fn goal_date(db: &Db) -> anyhow::Result<Option<NaiveDate>> {
    let d: Option<String> = sqlx::query_scalar("SELECT goal_date FROM plan_settings WHERE id = 1").fetch_one(&db.reader).await?;
    Ok(d.and_then(|d| d.parse().ok()))
}

// ---------------------------------------------------------------------------------------------------------
// Schreiben

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnitInput {
    pub date: Option<String>,
    pub sport_class: Option<String>,
    #[serde(default, deserialize_with = "nullable")]
    pub sub_sport: Option<Option<String>>,
    pub title: Option<String>,
    #[serde(default, deserialize_with = "nullable")]
    pub note: Option<Option<String>>,
    pub mode: Option<String>,
    pub duration_min: Option<u32>,
    pub target_load: Option<f64>,
    pub intensive_pct: Option<u32>,
    pub zone_minutes: Option<Vec<u32>>,
    /// Nur `planned` oder `skipped`; „erledigt“ entsteht durch Verknüpfen.
    pub status: Option<String>,
}

fn parse_day(field: &'static str, s: &str) -> Result<NaiveDate, Error> {
    let d = NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").map_err(|_| invalid(field, "invalid", "Date must be YYYY-MM-DD"))?;
    if !(2000..=2100).contains(&chrono::Datelike::year(&d)) {
        return Err(invalid(field, "out_of_range", "Date out of range"));
    }
    Ok(d)
}

fn text(v: &Option<String>) -> Option<String> {
    v.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string)
}

/// Wendet die Eingabe auf `u` an und prüft das Ergebnis.
async fn apply(conn: &mut SqliteConnection, u: &mut Unit, i: &UnitInput) -> Result<(), Error> {
    if let Some(d) = &i.date {
        u.date = parse_day("date", d)?;
    }
    if let Some(c) = &i.sport_class {
        if !SPORT_CLASSES.contains(&c.as_str()) {
            return Err(invalid("sport_class", "invalid", "Unknown sport class"));
        }
        if *c != u.sport_class && i.sub_sport.is_none() {
            u.sub_sport = None;
        }
        u.sport_class = c.clone();
    }
    if let Some(s) = &i.sub_sport {
        u.sub_sport = text(s);
    }
    if let Some(k) = &u.sub_sport
        && !sports::sub_exists(conn, &u.sport_class, k).await?
    {
        return Err(invalid("sub_sport", "invalid", "Unknown sub sport for this sport class"));
    }
    if let Some(t) = &i.title {
        let t = t.trim();
        if t.is_empty() || t.chars().count() > 120 {
            return Err(invalid("title", "invalid", "Title must have 1 to 120 characters"));
        }
        u.title = t.to_string();
    }
    if let Some(n) = &i.note {
        u.note = text(n);
    }
    if let Some(s) = &i.status {
        if !["planned", "skipped"].contains(&s.as_str()) {
            return Err(invalid("status", "invalid", "Status must be planned or skipped"));
        }
        u.status = s.clone();
    }
    let mode = i.mode.as_deref().unwrap_or(match u.mode {
        Mode::Target { .. } => "target",
        Mode::Zones(_) => "zones",
    });
    u.mode = match mode {
        "target" => {
            let (m0, l0, p0) = match u.mode {
                Mode::Target { minutes, load, intensive_pct } => (Some(minutes), Some(load), intensive_pct),
                Mode::Zones(_) => (None, None, 0),
            };
            let minutes = i.duration_min.or(m0).ok_or(invalid("duration_min", "required", "Duration is required"))?;
            let load = i.target_load.or(l0).ok_or(invalid("target_load", "required", "Target load is required"))?;
            let pct = i.intensive_pct.unwrap_or(p0);
            if !(1..=1440).contains(&minutes) {
                return Err(invalid("duration_min", "out_of_range", "Duration must be 1 to 1440 minutes"));
            }
            if !load.is_finite() || !(0.0..=1000.0).contains(&load) {
                return Err(invalid("target_load", "out_of_range", "Target load must be 0 to 1000"));
            }
            if pct > 100 {
                return Err(invalid("intensive_pct", "out_of_range", "Intensive share must be 0 to 100 percent"));
            }
            Mode::Target { minutes, load, intensive_pct: pct }
        }
        "zones" => {
            let z: [u32; ZONES] = match (&i.zone_minutes, &u.mode) {
                (Some(v), _) => {
                    v.as_slice().try_into().map_err(|_| invalid("zone_minutes", "invalid", "Exactly five zone values are required"))?
                }
                (None, Mode::Zones(z)) => *z,
                (None, _) => return Err(invalid("zone_minutes", "required", "Minutes per zone are required")),
            };
            let sum: u32 = z.iter().sum();
            if z.iter().any(|m| *m > 1440) || !(1..=1440).contains(&sum) {
                return Err(invalid("zone_minutes", "out_of_range", "Zone minutes must add up to 1 to 1440"));
            }
            Mode::Zones(z)
        }
        _ => return Err(invalid("mode", "invalid", "Mode must be target or zones")),
    };
    Ok(())
}

async fn save(conn: &mut SqliteConnection, u: &Unit, insert: bool, now: i64) -> Result<(), Error> {
    let (dur, load, pct, zones) = match &u.mode {
        Mode::Target { minutes, load, intensive_pct } => (Some(*minutes as i64), Some(*load), *intensive_pct as i64, None),
        Mode::Zones(z) => (None, None, 0, Some(serde_json::to_string(z)?)),
    };
    let mode = match u.mode {
        Mode::Target { .. } => "target",
        Mode::Zones(_) => "zones",
    };
    let sql = if insert {
        "INSERT INTO planned_units (date, sport_class, sub_sport, title, note, mode, duration_min, target_load, intensive_pct, \
         zone_minutes, status, updated_at_ms, created_at_ms, id) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?12, ?13)"
    } else {
        "UPDATE planned_units SET date = ?1, sport_class = ?2, sub_sport = ?3, title = ?4, note = ?5, mode = ?6, duration_min = ?7, \
         target_load = ?8, intensive_pct = ?9, zone_minutes = ?10, status = ?11, updated_at_ms = ?12 WHERE id = ?13"
    };
    sqlx::query(sql)
        .bind(u.date.to_string())
        .bind(&u.sport_class)
        .bind(&u.sub_sport)
        .bind(&u.title)
        .bind(&u.note)
        .bind(mode)
        .bind(dur)
        .bind(load)
        .bind(pct)
        .bind(zones)
        .bind(&u.status)
        .bind(now)
        .bind(&u.id)
        .execute(&mut *conn)
        .await?;
    bump_revision(conn, now).await?;
    Ok(())
}

pub async fn create(db: &Db, i: &UnitInput) -> Result<Value, Error> {
    for (v, f, m) in [
        (i.date.is_none(), "date", "Date is required"),
        (i.sport_class.is_none(), "sport_class", "Sport class is required"),
        (i.title.is_none(), "title", "Title is required"),
        (i.mode.is_none(), "mode", "Mode is required"),
    ] {
        if v {
            return Err(invalid(f, "required", m));
        }
    }
    let now = chrono::Utc::now().timestamp_millis();
    let mut u = Unit {
        id: new_id(),
        date: NaiveDate::MIN,
        sport_class: String::new(),
        sub_sport: None,
        title: String::new(),
        note: None,
        mode: Mode::Zones([0; ZONES]),
        status: "planned".into(),
        activity_id: None,
    };
    let mut tx = db.writer.begin().await?;
    apply(&mut tx, &mut u, i).await?;
    save(&mut tx, &u, true, now).await?;
    tx.commit().await?;
    get_one(db, &u.id).await
}

pub async fn update(db: &Db, id: &str, i: &UnitInput) -> Result<Value, Error> {
    let mut u = unit(db, id).await?.ok_or(Error::NotFound)?;
    let now = chrono::Utc::now().timestamp_millis();
    let mut tx = db.writer.begin().await?;
    apply(&mut tx, &mut u, i).await?;
    save(&mut tx, &u, false, now).await?;
    tx.commit().await?;
    get_one(db, id).await
}

pub async fn delete(db: &Db, id: &str) -> Result<(), Error> {
    let now = chrono::Utc::now().timestamp_millis();
    let mut tx = db.writer.begin().await?;
    let n = sqlx::query("DELETE FROM planned_units WHERE id = ?").bind(id).execute(&mut *tx).await?.rows_affected();
    if n == 0 {
        return Err(Error::NotFound);
    }
    bump_revision(&mut tx, now).await?;
    tx.commit().await?;
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GoalInput {
    pub date: String,
    pub name: String,
}

/// Ziel setzen (`Some`) oder entfernen (`None`).
pub async fn set_goal(db: &Db, g: Option<&GoalInput>) -> Result<Value, Error> {
    let (date, name) = match g {
        Some(g) => {
            let d = parse_day("date", &g.date)?;
            let n = g.name.trim();
            if n.is_empty() || n.chars().count() > 80 {
                return Err(invalid("name", "invalid", "Name must have 1 to 80 characters"));
            }
            (Some(d.to_string()), Some(n.to_string()))
        }
        None => (None, None),
    };
    let now = chrono::Utc::now().timestamp_millis();
    let mut tx = db.writer.begin().await?;
    sqlx::query("UPDATE plan_settings SET goal_date = ?, goal_name = ?, updated_at_ms = ? WHERE id = 1")
        .bind(date)
        .bind(name)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    bump_revision(&mut tx, now).await?;
    tx.commit().await?;
    Ok(goal(db).await?)
}

// ---------------------------------------------------------------------------------------------------------
// Verknüpfen mit Aktivitäten

/// Lokales Datum und Sportklasse einer Aktivität.
async fn activity_day(cfg: &Validated, db: &Db, activity_id: &str) -> anyhow::Result<Option<(NaiveDate, String)>> {
    let r =
        sqlx::query("SELECT start_time_ms, sport_class FROM activities WHERE id = ?").bind(activity_id).fetch_optional(&db.reader).await?;
    Ok(r.map(|r| {
        let start: i64 = r.get("start_time_ms");
        let day = chrono::DateTime::from_timestamp_millis(start).unwrap_or_default().with_timezone(&cfg.timezone).date_naive();
        (day, r.get("sport_class"))
    }))
}

/// Für die Aktivitätsseite: verknüpfte Einheit (mit Vergleich) und Vorschläge (gleicher Tag und gleiche Sportklasse,
/// noch offen, nicht abgelehnt).
pub async fn for_activity(cfg: &Validated, db: &Db, activity_id: &str) -> Result<Value, Error> {
    let (day, class) = activity_day(cfg, db, activity_id).await?.ok_or(Error::ActivityNotFound)?;
    let sets = load::zone_sets(db).await?;
    let s = load::settings(db).await?;
    let linked = units_where(db, "u.activity_id = ?", &[activity_id.to_string()]).await?;
    let linked = match linked.first() {
        Some(u) => unit_json(db, u, &sets, &s).await?,
        None => Value::Null,
    };
    let candidates = units_where(
        db,
        "u.date = ? AND u.sport_class = ? AND u.activity_id IS NULL AND u.status = 'planned' \
         AND NOT EXISTS (SELECT 1 FROM planned_unit_declines d WHERE d.unit_id = u.id AND d.activity_id = ?)",
        &[day.to_string(), class, activity_id.to_string()],
    )
    .await?;
    Ok(json!({
        "linked": linked,
        "candidates": candidates.iter().map(|u| u.to_json(&sets, &s)).collect::<Vec<_>>(),
    }))
}

pub async fn link(db: &Db, id: &str, activity_id: &str) -> Result<Value, Error> {
    let now = chrono::Utc::now().timestamp_millis();
    let mut tx = db.writer.begin().await?;
    let exists: Option<String> =
        sqlx::query_scalar("SELECT id FROM activities WHERE id = ?").bind(activity_id).fetch_optional(&mut *tx).await?;
    if exists.is_none() {
        return Err(Error::ActivityNotFound);
    }
    let taken: Option<String> = sqlx::query_scalar("SELECT id FROM planned_units WHERE activity_id = ? AND id <> ?")
        .bind(activity_id)
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
    if taken.is_some() {
        return Err(Error::ActivityTaken);
    }
    let n = sqlx::query("UPDATE planned_units SET activity_id = ?, status = 'planned', updated_at_ms = ? WHERE id = ?")
        .bind(activity_id)
        .bind(now)
        .bind(id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if n == 0 {
        return Err(Error::NotFound);
    }
    bump_revision(&mut tx, now).await?;
    tx.commit().await?;
    get_one(db, id).await
}

pub async fn unlink(db: &Db, id: &str) -> Result<Value, Error> {
    let now = chrono::Utc::now().timestamp_millis();
    let mut tx = db.writer.begin().await?;
    let n = sqlx::query("UPDATE planned_units SET activity_id = NULL, updated_at_ms = ? WHERE id = ?")
        .bind(now)
        .bind(id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if n == 0 {
        return Err(Error::NotFound);
    }
    bump_revision(&mut tx, now).await?;
    tx.commit().await?;
    get_one(db, id).await
}

/// „Nicht verknüpfen“: Vorschlag dieser Einheit für diese Aktivität ausblenden.
pub async fn decline(db: &Db, id: &str, activity_id: &str) -> Result<(), Error> {
    let now = chrono::Utc::now().timestamp_millis();
    let mut tx = db.writer.begin().await?;
    let exists: Option<String> = sqlx::query_scalar("SELECT id FROM planned_units WHERE id = ?").bind(id).fetch_optional(&mut *tx).await?;
    if exists.is_none() {
        return Err(Error::NotFound);
    }
    let act: Option<String> =
        sqlx::query_scalar("SELECT id FROM activities WHERE id = ?").bind(activity_id).fetch_optional(&mut *tx).await?;
    if act.is_none() {
        return Err(Error::ActivityNotFound);
    }
    sqlx::query("INSERT OR IGNORE INTO planned_unit_declines (unit_id, activity_id) VALUES (?, ?)")
        .bind(id)
        .bind(activity_id)
        .execute(&mut *tx)
        .await?;
    bump_revision(&mut tx, now).await?;
    tx.commit().await?;
    Ok(())
}
