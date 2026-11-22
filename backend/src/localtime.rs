//! Lokale Eingaben (`LocalTimeInput`, RFC-0003 Abschnitt 3) in der Zeitzone der Konfiguration nach UTC.

use chrono::{LocalResult, NaiveDateTime, TimeZone};
use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Occurrence {
    First,
    Second,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalTimeInput {
    pub local: String,
    #[serde(default)]
    pub occurrence: Option<Occurrence>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalTimeError {
    /// Nicht im Format `JJJJ-MM-TTTHH:MM:SS(.mmm)`
    Invalid,
    /// Doppelte Herbstzeit ohne `occurrence`
    Ambiguous,
}

/// Ergebnis in UTC-Millisekunden; `shifted` bei einer im Frühjahr nicht existierenden Zeit (eine Stunde später).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Converted {
    pub utc_ms: i64,
    pub shifted: bool,
}

fn parse_local(s: &str) -> Option<NaiveDateTime> {
    let (base, frac) = match s.split_once('.') {
        Some((b, f)) => (b, Some(f)),
        None => (s, None),
    };
    let b = base.as_bytes();
    let shape = b.len() == 19
        && b.iter().enumerate().all(|(i, c)| match i {
            4 | 7 => *c == b'-',
            10 => *c == b'T',
            13 | 16 => *c == b':',
            _ => c.is_ascii_digit(),
        });
    if !shape {
        return None;
    }
    let ms = match frac {
        None => 0,
        Some(f) if (1..=3).contains(&f.len()) && f.bytes().all(|c| c.is_ascii_digit()) => {
            f.parse::<i64>().ok()? * 10_i64.pow(3 - f.len() as u32)
        }
        Some(_) => return None,
    };
    let dt = NaiveDateTime::parse_from_str(base, "%Y-%m-%dT%H:%M:%S").ok()?;
    Some(dt + chrono::Duration::milliseconds(ms))
}

pub fn to_utc(input: &LocalTimeInput, tz: chrono_tz::Tz) -> Result<Converted, LocalTimeError> {
    let local = parse_local(&input.local).ok_or(LocalTimeError::Invalid)?;
    match tz.from_local_datetime(&local) {
        LocalResult::Single(d) => Ok(Converted { utc_ms: d.timestamp_millis(), shifted: false }),
        LocalResult::Ambiguous(a, b) => match input.occurrence {
            Some(Occurrence::First) => Ok(Converted { utc_ms: a.timestamp_millis(), shifted: false }),
            Some(Occurrence::Second) => Ok(Converted { utc_ms: b.timestamp_millis(), shifted: false }),
            None => Err(LocalTimeError::Ambiguous),
        },
        // Lücke der Frühjahrsumstellung: eine Stunde später (02:30 wird 03:30).
        LocalResult::None => match tz.from_local_datetime(&(local + chrono::Duration::hours(1))) {
            LocalResult::Single(d) | LocalResult::Ambiguous(d, _) => Ok(Converted { utc_ms: d.timestamp_millis(), shifted: true }),
            LocalResult::None => Err(LocalTimeError::Invalid),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(local: &str, occurrence: Option<Occurrence>) -> LocalTimeInput {
        LocalTimeInput { local: local.into(), occurrence }
    }

    const BERLIN: chrono_tz::Tz = chrono_tz::Europe::Berlin;

    #[test]
    fn normal_summer_time() {
        // 10:00 MESZ = 08:00 UTC
        let c = to_utc(&input("2026-07-01T10:00:00", None), BERLIN).unwrap();
        assert_eq!(c.utc_ms, 1_782_892_800_000);
        assert!(!c.shifted);
        let c = to_utc(&input("2026-07-01T10:00:00.5", None), BERLIN).unwrap();
        assert_eq!(c.utc_ms, 1_782_892_800_500);
    }

    #[test]
    fn spring_gap_shifts_one_hour() {
        // 29.03.2026 02:30 existiert nicht, wird 03:30 MESZ = 01:30 UTC.
        let c = to_utc(&input("2026-03-29T02:30:00", None), BERLIN).unwrap();
        assert!(c.shifted);
        let expected = to_utc(&input("2026-03-29T03:30:00", None), BERLIN).unwrap();
        assert_eq!(c.utc_ms, expected.utc_ms);
    }

    #[test]
    fn autumn_needs_occurrence() {
        let ambiguous = input("2026-10-25T02:30:00", None);
        assert_eq!(to_utc(&ambiguous, BERLIN), Err(LocalTimeError::Ambiguous));
        let first = to_utc(&input("2026-10-25T02:30:00", Some(Occurrence::First)), BERLIN).unwrap();
        let second = to_utc(&input("2026-10-25T02:30:00", Some(Occurrence::Second)), BERLIN).unwrap();
        assert_eq!(second.utc_ms - first.utc_ms, 3_600_000);
    }

    #[test]
    fn rejects_bad_shapes() {
        for s in ["2026-07-01 10:00:00", "2026-07-01T10:00", "2026-07-01T10:00:00Z", "2026-07-01T10:00:00.1234", "2026-13-01T10:00:00"] {
            assert_eq!(to_utc(&input(s, None), BERLIN), Err(LocalTimeError::Invalid), "{s}");
        }
    }
}
