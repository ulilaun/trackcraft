//! Frei benannte Saisons (RFC-0009 Abschnitt „Saisons“): Name, erster und letzter lokaler Kalendertag,
//! für alle Sportklassen, ohne Überschneidung, Lücken erlaubt.

use chrono::NaiveDate;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;
use sqlx::sqlite::SqliteRow;

use crate::db::{Db, new_id};
use crate::import_job::timestamp;

/// Höchstlänge des Namens in Zeichen.
pub const NAME_MAX_CHARS: usize = 80;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Season {
    pub id: String,
    pub name: String,
    pub start: NaiveDate,
    pub end: NaiveDate,
    created_at_ms: i64,
    updated_at_ms: i64,
}

/// Lage einer Saison zum heutigen lokalen Tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Current,
    Ended,
    Upcoming,
}

impl State {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Current => "current",
            Self::Ended => "ended",
            Self::Upcoming => "upcoming",
        }
    }
}

impl Season {
    pub fn state(&self, today: NaiveDate) -> State {
        if today < self.start {
            State::Upcoming
        } else if today > self.end {
            State::Ended
        } else {
            State::Current
        }
    }

    pub fn contains(&self, day: NaiveDate) -> bool {
        day >= self.start && day <= self.end
    }

    pub fn to_json(&self, today: NaiveDate) -> Value {
        json!({
            "id": self.id,
            "name": self.name,
            "start_date": self.start.to_string(),
            "end_date": self.end.to_string(),
            "state": self.state(today).as_str(),
            "created_at": timestamp(self.created_at_ms),
            "updated_at": timestamp(self.updated_at_ms),
        })
    }
}

/// Nur `YYYY-MM-DD` mit führenden Nullen, wie es die Tabelle für den Textvergleich braucht.
fn parse_day(s: &str) -> Option<NaiveDate> {
    if s.len() != 10 {
        return None;
    }
    NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()
}

fn from_row(r: &SqliteRow) -> anyhow::Result<Season> {
    let day = |k: &str| -> anyhow::Result<NaiveDate> {
        let s: String = r.get(k);
        parse_day(&s).ok_or_else(|| anyhow::anyhow!("Saison mit ungültigem Datum {s}"))
    };
    Ok(Season {
        id: r.get("id"),
        name: r.get("name"),
        start: day("start_date")?,
        end: day("end_date")?,
        created_at_ms: r.get("created_at_ms"),
        updated_at_ms: r.get("updated_at_ms"),
    })
}

const COLUMNS: &str = "SELECT id, name, start_date, end_date, created_at_ms, updated_at_ms FROM seasons";

/// Alle Saisons, neueste zuerst.
pub async fn list(db: &Db) -> anyhow::Result<Vec<Season>> {
    let rows = sqlx::query(sqlx::AssertSqlSafe(format!("{COLUMNS} ORDER BY start_date DESC, id"))).fetch_all(&db.reader).await?;
    rows.iter().map(from_row).collect()
}

pub async fn get(db: &Db, id: &str) -> anyhow::Result<Option<Season>> {
    let row = sqlx::query(sqlx::AssertSqlSafe(format!("{COLUMNS} WHERE id = ?"))).bind(id).fetch_optional(&db.reader).await?;
    row.as_ref().map(from_row).transpose()
}

/// Die Saison, in der `today` liegt, sonst die zuletzt beendete; nur künftige Saisons ergeben keine.
pub fn current_or_last(seasons: &[Season], today: NaiveDate) -> Option<&Season> {
    seasons.iter().find(|s| s.contains(today)).or_else(|| seasons.iter().filter(|s| s.end < today).max_by_key(|s| s.end))
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub start_date: Option<String>,
    #[serde(default)]
    pub end_date: Option<String>,
}

#[derive(Debug)]
pub enum Error {
    NotFound,
    Validation {
        field: &'static str,
        code: &'static str,
        message: &'static str,
    },
    /// Überschneidung mit einer bestehenden Saison
    Overlap(Season),
    /// Name schon vergeben
    NameTaken,
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

fn check_name(name: &str) -> Result<String, Error> {
    let n = name.trim();
    if n.is_empty() {
        return Err(invalid("name", "must_not_be_empty", "Name must not be empty"));
    }
    if n.chars().count() > NAME_MAX_CHARS {
        return Err(invalid("name", "too_long", "Name must have at most 80 characters"));
    }
    Ok(n.to_string())
}

fn check_day(field: &'static str, s: &str) -> Result<NaiveDate, Error> {
    parse_day(s).ok_or_else(|| invalid(field, "invalid_date", "Expected YYYY-MM-DD"))
}

/// Prüft Name und Zeitraum gegen die übrigen Saisons; läuft in der Schreibtransaktion.
async fn check_conflicts(
    conn: &mut sqlx::SqliteConnection,
    id: Option<&str>,
    name: &str,
    start: NaiveDate,
    end: NaiveDate,
) -> Result<(), Error> {
    if start > end {
        return Err(invalid("end_date", "before_start", "End date must not be before start date"));
    }
    let other = id.unwrap_or("");
    let taken: Option<String> = sqlx::query_scalar("SELECT id FROM seasons WHERE lower(trim(name)) = lower(?) AND id <> ?")
        .bind(name)
        .bind(other)
        .fetch_optional(&mut *conn)
        .await?;
    if taken.is_some() {
        return Err(Error::NameTaken);
    }
    let row = sqlx::query(sqlx::AssertSqlSafe(format!(
        "{COLUMNS} WHERE id <> ? AND start_date <= ? AND end_date >= ? ORDER BY start_date LIMIT 1"
    )))
    .bind(other)
    .bind(end.to_string())
    .bind(start.to_string())
    .fetch_optional(&mut *conn)
    .await?;
    match row {
        Some(r) => Err(Error::Overlap(from_row(&r)?)),
        None => Ok(()),
    }
}

pub async fn create(db: &Db, input: &Input) -> Result<Season, Error> {
    let name = check_name(input.name.as_deref().ok_or_else(|| invalid("name", "required", "Name is required"))?)?;
    let start =
        check_day("start_date", input.start_date.as_deref().ok_or_else(|| invalid("start_date", "required", "Start date is required"))?)?;
    let end = check_day("end_date", input.end_date.as_deref().ok_or_else(|| invalid("end_date", "required", "End date is required"))?)?;
    let now = chrono::Utc::now().timestamp_millis();
    let id = new_id();
    let mut tx = db.writer.begin().await?;
    check_conflicts(&mut tx, None, &name, start, end).await?;
    sqlx::query("INSERT INTO seasons (id, name, start_date, end_date, created_at_ms, updated_at_ms) VALUES (?, ?, ?, ?, ?, ?)")
        .bind(&id)
        .bind(&name)
        .bind(start.to_string())
        .bind(end.to_string())
        .bind(now)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Season { id, name, start, end, created_at_ms: now, updated_at_ms: now })
}

pub async fn update(db: &Db, id: &str, input: &Input) -> Result<Season, Error> {
    if input.name.is_none() && input.start_date.is_none() && input.end_date.is_none() {
        return Err(invalid("body", "empty", "At least one field is required"));
    }
    let name = input.name.as_deref().map(check_name).transpose()?;
    let start = input.start_date.as_deref().map(|s| check_day("start_date", s)).transpose()?;
    let end = input.end_date.as_deref().map(|s| check_day("end_date", s)).transpose()?;
    let mut tx = db.writer.begin().await?;
    let row = sqlx::query(sqlx::AssertSqlSafe(format!("{COLUMNS} WHERE id = ?"))).bind(id).fetch_optional(&mut *tx).await?;
    let Some(old) = row.as_ref().map(from_row).transpose()? else {
        return Err(Error::NotFound);
    };
    let now = chrono::Utc::now().timestamp_millis();
    let new = Season {
        name: name.unwrap_or(old.name),
        start: start.unwrap_or(old.start),
        end: end.unwrap_or(old.end),
        updated_at_ms: now,
        ..old
    };
    check_conflicts(&mut tx, Some(id), &new.name, new.start, new.end).await?;
    sqlx::query("UPDATE seasons SET name = ?, start_date = ?, end_date = ?, updated_at_ms = ? WHERE id = ?")
        .bind(&new.name)
        .bind(new.start.to_string())
        .bind(new.end.to_string())
        .bind(now)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(new)
}

/// `false`: unbekannt. Saisons haben keine abhängigen Daten.
pub async fn delete(db: &Db, id: &str) -> anyhow::Result<bool> {
    Ok(sqlx::query("DELETE FROM seasons WHERE id = ?").bind(id).execute(&db.writer).await?.rows_affected() > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(id: &str, start: &str, end: &str) -> Season {
        Season {
            id: id.into(),
            name: id.into(),
            start: parse_day(start).unwrap(),
            end: parse_day(end).unwrap(),
            created_at_ms: 0,
            updated_at_ms: 0,
        }
    }

    #[test]
    fn current_or_last_ended() {
        let list = vec![s("c", "2027-03-01", "2027-10-31"), s("b", "2026-03-01", "2026-10-31"), s("a", "2025-03-01", "2025-10-31")];
        let day = |d: &str| parse_day(d).unwrap();
        assert_eq!(current_or_last(&list, day("2026-10-31")).unwrap().id, "b");
        assert_eq!(current_or_last(&list, day("2026-11-13")).unwrap().id, "b");
        assert_eq!(list[1].state(day("2026-11-13")), State::Ended);
        assert_eq!(current_or_last(&list, day("2027-03-01")).unwrap().id, "c");
        assert!(current_or_last(&list, day("2025-01-01")).is_none());
        assert!(current_or_last(&[], day("2025-01-01")).is_none());
    }
}
