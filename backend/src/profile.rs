//! Benutzerprofil: Spitzname, Vorname, Nachname, Geburtstag, Farbe des Benutzersymbols.
//! V1 kennt genau einen Benutzer (ADR-0001); die Datenbank ist führend, `[user] display_name` der TOML gilt nur beim
//! ersten Start.

use chrono::NaiveDate;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;

use crate::activities::nullable;
use crate::db::{Db, bump_revision};

/// Farben des Benutzersymbols; das Frontend kennt dieselben Schlüssel.
pub const AVATAR_COLORS: &[&str] = &["blue", "teal", "green", "amber", "orange", "red", "pink", "violet"];
pub const NICKNAME_MAX: usize = 40;
pub const NAME_MAX: usize = 60;

#[derive(Debug, Clone, PartialEq)]
pub struct Profile {
    pub id: String,
    pub nickname: String,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub birthday: Option<NaiveDate>,
    pub avatar_color: Option<String>,
    pub updated_at_ms: i64,
}

impl Profile {
    pub fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "nickname": self.nickname,
            "first_name": self.first_name,
            "last_name": self.last_name,
            "birthday": self.birthday.map(|d| d.to_string()),
            "avatar_color": self.avatar_color,
            "updated_at": crate::import_job::timestamp(self.updated_at_ms),
        })
    }
}

/// PATCH-Körper: fehlende Felder bleiben, `null` leert ein freiwilliges Feld.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    /// Pflicht: `null` wird als leer abgelehnt.
    #[serde(default, deserialize_with = "nullable")]
    pub nickname: Option<Option<String>>,
    #[serde(default, deserialize_with = "nullable")]
    pub first_name: Option<Option<String>>,
    #[serde(default, deserialize_with = "nullable")]
    pub last_name: Option<Option<String>>,
    #[serde(default, deserialize_with = "nullable")]
    pub birthday: Option<Option<String>>,
    #[serde(default, deserialize_with = "nullable")]
    pub avatar_color: Option<Option<String>>,
}

#[derive(Debug)]
pub enum Error {
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

const COLUMNS: &str = "SELECT id, display_name, first_name, last_name, birthday, avatar_color, updated_at_ms FROM users";

fn from_row(r: &sqlx::sqlite::SqliteRow) -> anyhow::Result<Profile> {
    let birthday: Option<String> = r.get("birthday");
    Ok(Profile {
        id: r.get("id"),
        nickname: r.get("display_name"),
        first_name: r.get("first_name"),
        last_name: r.get("last_name"),
        birthday: birthday.map(|b| b.parse()).transpose()?,
        avatar_color: r.get("avatar_color"),
        updated_at_ms: r.get("updated_at_ms"),
    })
}

/// Das Profil des einzigen Benutzers.
pub async fn get(db: &Db) -> anyhow::Result<Profile> {
    let row = sqlx::query(sqlx::AssertSqlSafe(format!("{COLUMNS} LIMIT 1"))).fetch_one(&db.reader).await?;
    from_row(&row)
}

fn check_nickname(v: &str) -> Result<String, Error> {
    let v = v.trim();
    if v.is_empty() {
        return Err(invalid("nickname", "required", "Nickname must not be empty"));
    }
    if v.chars().count() > NICKNAME_MAX {
        return Err(invalid("nickname", "too_long", "Nickname is too long"));
    }
    Ok(v.to_string())
}

fn check_name(field: &'static str, v: &Option<String>) -> Result<Option<String>, Error> {
    let Some(v) = v.as_deref().map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    if v.chars().count() > NAME_MAX {
        return Err(invalid(field, "too_long", "Name is too long"));
    }
    Ok(Some(v.to_string()))
}

fn check_birthday(v: &Option<String>, today: NaiveDate) -> Result<Option<NaiveDate>, Error> {
    let Some(v) = v.as_deref().map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    let d = NaiveDate::parse_from_str(v, "%Y-%m-%d").map_err(|_| invalid("birthday", "invalid", "Birthday must be YYYY-MM-DD"))?;
    if d > today || d < NaiveDate::from_ymd_opt(1900, 1, 1).expect("Datum") {
        return Err(invalid("birthday", "out_of_range", "Birthday is out of range"));
    }
    Ok(Some(d))
}

fn check_color(v: &Option<String>) -> Result<Option<String>, Error> {
    match v.as_deref() {
        None => Ok(None),
        Some(c) if AVATAR_COLORS.contains(&c) => Ok(Some(c.to_string())),
        Some(_) => Err(invalid("avatar_color", "invalid", "Unknown avatar color")),
    }
}

/// Ändert das Profil; eine echte Änderung ist sicherungsrelevant und erhöht den Änderungszähler.
pub async fn update(db: &Db, input: &Input, today: NaiveDate) -> Result<Profile, Error> {
    if input.nickname.is_none()
        && input.first_name.is_none()
        && input.last_name.is_none()
        && input.birthday.is_none()
        && input.avatar_color.is_none()
    {
        return Err(invalid("body", "empty", "At least one field is required"));
    }
    let nickname = input.nickname.as_ref().map(|v| check_nickname(v.as_deref().unwrap_or(""))).transpose()?;
    let first_name = input.first_name.as_ref().map(|v| check_name("first_name", v)).transpose()?;
    let last_name = input.last_name.as_ref().map(|v| check_name("last_name", v)).transpose()?;
    let birthday = input.birthday.as_ref().map(|v| check_birthday(v, today)).transpose()?;
    let avatar_color = input.avatar_color.as_ref().map(check_color).transpose()?;

    let mut tx = db.writer.begin().await?;
    let row = sqlx::query(sqlx::AssertSqlSafe(format!("{COLUMNS} LIMIT 1"))).fetch_one(&mut *tx).await?;
    let old = from_row(&row)?;
    let now = chrono::Utc::now().timestamp_millis();
    let new = Profile {
        nickname: nickname.unwrap_or_else(|| old.nickname.clone()),
        first_name: first_name.unwrap_or_else(|| old.first_name.clone()),
        last_name: last_name.unwrap_or_else(|| old.last_name.clone()),
        birthday: birthday.unwrap_or(old.birthday),
        avatar_color: avatar_color.unwrap_or_else(|| old.avatar_color.clone()),
        updated_at_ms: now,
        id: old.id.clone(),
    };
    if (Profile { updated_at_ms: old.updated_at_ms, ..new.clone() }) == old {
        return Ok(old);
    }
    sqlx::query(
        "UPDATE users SET display_name = ?, first_name = ?, last_name = ?, birthday = ?, avatar_color = ?, updated_at_ms = ? WHERE id = ?",
    )
    .bind(&new.nickname)
    .bind(&new.first_name)
    .bind(&new.last_name)
    .bind(new.birthday.map(|d| d.to_string()))
    .bind(&new.avatar_color)
    .bind(now)
    .bind(&new.id)
    .execute(&mut *tx)
    .await?;
    bump_revision(&mut tx, now).await?;
    tx.commit().await?;
    Ok(new)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(s: &str) -> NaiveDate {
        s.parse().unwrap()
    }

    #[test]
    fn nickname_trimmed_and_required() {
        assert_eq!(check_nickname("  Uli ").unwrap(), "Uli");
        assert!(matches!(check_nickname("   "), Err(Error::Validation { code: "required", .. })));
        assert!(check_nickname(&"x".repeat(NICKNAME_MAX + 1)).is_err());
    }

    #[test]
    fn birthday_range() {
        let today = day("2026-11-16");
        assert_eq!(check_birthday(&Some("1970-05-01".into()), today).unwrap(), Some(day("1970-05-01")));
        assert_eq!(check_birthday(&Some("  ".into()), today).unwrap(), None);
        assert!(check_birthday(&Some("2026-11-17".into()), today).is_err());
        assert!(check_birthday(&Some("1899-12-31".into()), today).is_err());
        assert!(check_birthday(&Some("01.05.1970".into()), today).is_err());
    }

    #[test]
    fn colors_from_palette() {
        assert_eq!(check_color(&Some("teal".into())).unwrap(), Some("teal".into()));
        assert!(check_color(&Some("#fff".into())).is_err());
        assert_eq!(check_color(&None).unwrap(), None);
    }
}
