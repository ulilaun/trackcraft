//! FIT-Adapter über rustyfit (ADR-0002, RFC-0008 „FIT-Zuordnung“ und „FIT-Parser“).
//! Nur dieses Modul kennt rustyfit; ein Wechsel des Parsers betrifft nur diese Datei.

use core::convert::Infallible;

use rustyfit::profile::mesgdef;
use rustyfit::profile::typedef::{self, DateTime};
use rustyfit::{Decoder, DecoderBuilder, DecoderError};

use super::{FileValues, ImportError, ParsedSession, RawPoint};
use crate::domain::track::{HR_MAX, HR_MIN, LAT_MAX, LON_MAX, Point, deg_to_e7};

/// Zeitpunkte unter diesem Wert sind Gerätezeit seit dem Einschalten, keine Uhrzeit.
const DATETIME_MIN: u32 = 0x1000_0000;

fn time_ms(t: DateTime) -> Option<i64> {
    if t.0 < DATETIME_MIN {
        return None;
    }
    t.unix_timestamp().map(|s| s * 1000)
}

fn sport_name(sport: typedef::Sport, sub: typedef::SubSport) -> Option<String> {
    if sport.0 == 0xFF {
        return None;
    }
    let name = |s: String| if s.contains('(') { None } else { Some(s) };
    let sport = name(sport.to_string())?;
    match name(sub.to_string()) {
        Some(sub) if sub != "generic" && sub_valid(&sub) => Some(format!("{sport}-{sub}")),
        _ => Some(sport),
    }
}

fn sub_valid(s: &str) -> bool {
    s != "invalid"
}

struct RawSession {
    start_ms: Option<i64>,
    values: FileValues,
    source_sport: Option<String>,
}

fn session_values(s: &mesgdef::Session) -> RawSession {
    let start_ms = time_ms(s.start_time);
    RawSession {
        start_ms,
        values: FileValues {
            start_ms,
            elapsed_ms: (s.total_elapsed_time != u32::MAX).then_some(s.total_elapsed_time as i64),
            distance_m: (s.total_distance != u32::MAX && s.total_distance > 0).then(|| s.total_distance as f64 / 100.0),
            ascent_m: (s.total_ascent != u16::MAX && s.total_ascent > 0).then_some(s.total_ascent as f64),
            avg_hr: (s.avg_heart_rate != u8::MAX && s.avg_heart_rate > 0).then_some(s.avg_heart_rate as u16),
        },
        source_sport: sport_name(s.sport, s.sub_sport),
    }
}

fn semicircles_to_e7(v: i32) -> i32 {
    deg_to_e7(v as f64 * 180.0 / 2_147_483_648.0)
}

fn record_point(r: &mesgdef::Record) -> RawPoint {
    let pos = (r.position_lat != i32::MAX && r.position_long != i32::MAX)
        .then(|| (semicircles_to_e7(r.position_lat), semicircles_to_e7(r.position_long)))
        .filter(|&(la, lo)| la.unsigned_abs() <= LAT_MAX as u32 && lo.unsigned_abs() <= LON_MAX as u32);
    // Höhe: Rohwert / 5 − 500 m, in Dezimetern also Rohwert · 2 − 5000 (exakt ganzzahlig).
    let ele_dm = if r.enhanced_altitude != u32::MAX {
        Some((r.enhanced_altitude as i64 * 2 - 5000) as i32)
    } else if r.altitude != u16::MAX {
        Some(r.altitude as i32 * 2 - 5000)
    } else {
        None
    };
    let hr = (r.heart_rate != u8::MAX).then_some(r.heart_rate as u16).filter(|h| (HR_MIN..=HR_MAX).contains(h));
    let speed_mps = if r.enhanced_speed != u32::MAX {
        Some(r.enhanced_speed as f64 / 1000.0)
    } else if r.speed != u16::MAX {
        Some(r.speed as f64 / 1000.0)
    } else {
        None
    };
    RawPoint {
        point: Point { t_ms: time_ms(r.timestamp), pos, ele_dm, hr },
        speed_mps,
        distance_m: (r.distance != u32::MAX).then(|| r.distance as f64 / 100.0),
    }
}

/// CRC-16 des FIT-Protokolls (Polynom 0xA001, Nibble-Tabelle wie im FIT SDK).
fn crc16(data: &[u8]) -> u16 {
    const T: [u16; 16] =
        [0x0000, 0xCC01, 0xD801, 0x1400, 0xF001, 0x3C00, 0x2800, 0xE401, 0xA001, 0x6C00, 0x7800, 0xB401, 0x5000, 0x9C01, 0x8801, 0x4400];
    let mut crc: u16 = 0;
    for &b in data {
        for nibble in [b & 0x0F, b >> 4] {
            let tmp = T[(crc & 0x0F) as usize];
            crc = ((crc >> 4) & 0x0FFF) ^ tmp ^ T[nibble as usize];
        }
    }
    crc
}

/// Ergebnis der eigenen Prüfsummenprüfung.
#[derive(Debug, PartialEq, Eq)]
enum Integrity {
    Intact,
    /// Kopf in Ordnung, aber die Datei-Prüfsumme einer Teildatei passt nicht.
    FileCrcWrong,
    /// Die Kopf-Prüfsumme einer Teildatei passt nicht.
    HeaderCrcWrong,
}

/// Prüft Kopf- und Datei-Prüfsumme jeder Teildatei nach FIT-Protokoll.
///
/// Eigene Prüfung statt der von rustyfit, weil eine falsche Datei-Prüfsumme die Datei nicht abweisen soll, sondern
/// nur gemeldet wird; rustyfit kennt nur „prüfen und abweisen“ oder „nicht prüfen“. Ursprünglicher
/// Anlass war ein Fehler in rustyfit 0.11.1 (Kopf mit Prüfsumme 0 fehlte in der Datei-Prüfsumme, behoben in 0.11.2,
/// muktihari/rustyfit#179). Eine abgeschnittene Datei bleibt hier unbewertet; sie scheitert beim Dekodieren.
fn integrity(bytes: &[u8]) -> Integrity {
    let mut result = Integrity::Intact;
    let mut o = 0usize;
    while let Some(head) = bytes.get(o..o + 12) {
        let size = head[0] as usize;
        if size == 14 {
            let Some(crc) = bytes.get(o + 12..o + 14) else { break };
            let crc = u16::from_le_bytes([crc[0], crc[1]]);
            if crc != 0 && crc != crc16(head) {
                return Integrity::HeaderCrcWrong;
            }
        }
        let end = o.saturating_add(size).saturating_add(u32::from_le_bytes([head[4], head[5], head[6], head[7]]) as usize);
        let (Some(part), Some(found)) = (bytes.get(o..end), bytes.get(end..end + 2)) else { break };
        if crc16(part) != u16::from_le_bytes([found[0], found[1]]) {
            result = Integrity::FileCrcWrong;
        }
        o = end + 2;
    }
    result
}

type Decoded = (Vec<RawSession>, Vec<RawPoint>);

/// Dekodiert alle Teildateien und sammelt Sessions und Records.
fn decode(bytes: &[u8], mut decoder: Decoder) -> Result<Option<Decoded>, DecoderError<Infallible>> {
    let mut reader = bytes;
    let mut sessions: Vec<RawSession> = Vec::new();
    let mut records: Vec<RawPoint> = Vec::new();
    let mut parts = 0;
    loop {
        match decoder.decode(&mut reader) {
            Ok(Some(fit)) => {
                parts += 1;
                for mesg in &fit.messages {
                    match mesg.num {
                        typedef::MesgNum::SESSION => sessions.push(session_values(&mesgdef::Session::from(mesg))),
                        typedef::MesgNum::RECORD => records.push(record_point(&mesgdef::Record::from(mesg))),
                        _ => {}
                    }
                }
            }
            Ok(None) => break,
            Err(e) => return Err(e),
        }
    }
    Ok((parts > 0).then_some((sessions, records)))
}

/// Liest alle Teildateien einer (ggf. verketteten) FIT-Datei und ordnet Records den Sessions zu.
///
/// Prüfsummen prüft [`integrity`]. Passt nur eine Datei-Prüfsumme nicht, wird die Datei trotzdem angenommen, wenn
/// sie vollständig dekodiert; der zweite Wert sagt, ob das geschah. Eine falsche Kopf-Prüfsumme und
/// Strukturfehler weisen die Datei ab.
pub fn parse(bytes: &[u8]) -> Result<(Vec<ParsedSession>, bool), ImportError> {
    let checksum_ignored = match integrity(bytes) {
        Integrity::HeaderCrcWrong => return Err(ImportError::InvalidFile("FIT nicht lesbar: Kopf-Prüfsumme falsch".into())),
        Integrity::FileCrcWrong => true,
        Integrity::Intact => false,
    };
    let decoded = decode(bytes, DecoderBuilder::new().checksum(false).build())
        .map_err(|e| ImportError::InvalidFile(format!("FIT nicht lesbar: {e:?}")))?;
    let Some((mut sessions, records)) = decoded else {
        return Err(ImportError::InvalidFile("FIT ohne Inhalt".into()));
    };

    // Ohne Session-Message: eine Session ohne Dateiwerte.
    if sessions.is_empty() {
        sessions.push(RawSession { start_ms: None, values: FileValues::default(), source_sport: None });
    }

    let mut buckets: Vec<Vec<RawPoint>> = vec![Vec::new(); sessions.len()];
    if sessions.len() == 1 {
        buckets[0] = records;
    } else {
        for r in records {
            // Bei mehreren Sessions werden Records ohne Zeitstempel verworfen.
            let Some(t) = r.point.t_ms else { continue };
            // Zur letzten Session, deren Start nicht nach dem Record liegt; davor zur ersten.
            let idx = sessions.iter().rposition(|s| s.start_ms.is_some_and(|st| st <= t)).unwrap_or(0);
            buckets[idx].push(r);
        }
    }

    let sessions = sessions
        .into_iter()
        .zip(buckets)
        .enumerate()
        .map(|(i, (s, pts))| {
            // Punkte ganz ohne Wert werden nicht gespeichert.
            let pts: Vec<RawPoint> = pts.into_iter().filter(|p| !p.point.is_empty()).collect();
            ParsedSession {
                session_index: i,
                source_sport: s.source_sport,
                file: s.values,
                sections: if pts.is_empty() { vec![] } else { vec![pts] },
                title: None,
            }
        })
        .collect();
    Ok((sessions, checksum_ignored))
}
