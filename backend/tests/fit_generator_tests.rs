//! FIT-Sonderfälle aus einem Generator (RFC-0014 „Pflichtfälle“): erzeugt mit dem Encoder von rustyfit, nicht von Hand.
//! Mehrere Sessions, Records vor der ersten Session, Datei ohne Session-Message, verkettete Datei, Big-Endian,
//! komprimierte Zeitstempel-Header, Ungültig-Werte, falsche Prüfsumme und abgebrochene Datei.

use rustyfit::profile::mesgdef;
use rustyfit::profile::typedef::{self, DateTime};
use rustyfit::proto::{FIT, Message};
use rustyfit::{EncoderBuilder as Builder, Endianness, HeaderOption};
use trackcraft::import::{self, ImportError};

const T0: i64 = 1_700_000_000; // Unix-Sekunden

fn file_id() -> Message {
    let mut f = mesgdef::FileId::new();
    f.r#type = typedef::File::ACTIVITY;
    f.manufacturer = typedef::Manufacturer::GARMIN;
    f.into()
}

fn session(start_s: i64, elapsed_s: u32, sport: typedef::Sport, sub: typedef::SubSport) -> Message {
    let mut s = mesgdef::Session::new();
    s.start_time = DateTime::from_unix_timestamp(start_s);
    s.timestamp = DateTime::from_unix_timestamp(start_s + elapsed_s as i64);
    s.total_elapsed_time = elapsed_s * 1000;
    s.total_distance = 123_456; // Zentimeter: 1234,56 m
    s.total_ascent = 0; // 0 gilt als fehlend
    s.avg_heart_rate = 0xFF; // ungültig
    s.sport = sport;
    s.sub_sport = sub;
    s.into()
}

fn record(t_s: i64, north: i32, hr: Option<u8>, altitude_raw: Option<u16>) -> Message {
    let mut r = mesgdef::Record::new();
    r.timestamp = DateTime::from_unix_timestamp(t_s);
    r.position_lat = 570_000_000 + north; // Semicircles
    r.position_long = 140_000_000;
    if let Some(h) = hr {
        r.heart_rate = h;
    }
    if let Some(a) = altitude_raw {
        r.altitude = a;
    }
    r.speed = 5000;
    r.distance = (t_s - T0).max(0) as u32 * 500;
    r.into()
}

fn encode(messages: Vec<Message>, big_endian: bool, compressed: bool) -> Vec<u8> {
    let mut b = Builder::new();
    if big_endian {
        b = b.endianness(Endianness::BigEndian);
    }
    if compressed {
        b = b.header_option(HeaderOption::Compressed(3));
    }
    let mut enc = b.build();
    let mut fit = FIT { messages, ..Default::default() };
    let mut out = embedded_io_adapters::std::FromStd::new(std::io::Cursor::new(Vec::<u8>::new()));
    enc.encode(&mut out, &mut fit).unwrap();
    out.into_inner().into_inner()
}

/// CRC-16 des FIT-Protokolls, unabhängig vom Code unter Test.
fn crc16(data: &[u8]) -> u16 {
    let mut crc: u16 = 0;
    for &b in data {
        crc ^= b as u16;
        for _ in 0..8 {
            crc = if crc & 1 == 1 { (crc >> 1) ^ 0xA001 } else { crc >> 1 };
        }
    }
    crc
}

fn records(from: i64, n: i64) -> Vec<Message> {
    (0..n).map(|i| record(from + i, i as i32 * 1000, Some(120), Some(3000))).collect()
}

#[test]
fn single_session_values_and_conversions() {
    let mut msgs = vec![file_id()];
    msgs.extend(records(T0, 10));
    msgs.push(record(T0 + 10, 0, Some(0xFF), None)); // HF ungültig, Höhe fehlt
    msgs.push(session(T0, 11, typedef::Sport::CYCLING, typedef::SubSport::ROAD));
    let f = import::parse(&encode(msgs, false, false)).unwrap();
    assert_eq!(f.sessions.len(), 1);
    let s = &f.sessions[0];
    assert_eq!(s.source_sport.as_deref(), Some("cycling-road"));
    assert_eq!(s.file.start_ms, Some(T0 * 1000));
    assert_eq!(s.file.elapsed_ms, Some(11_000));
    assert_eq!(s.file.distance_m, Some(1234.56));
    assert_eq!(s.file.ascent_m, None, "0 gilt als fehlend");
    assert_eq!(s.file.avg_hr, None, "Ungültig-Wert gilt als fehlend");
    let p0 = s.sections[0][0];
    // 570.000.000 Semicircles = 47,7767... Grad; Höhe 3000/5 − 500 = 100 m = 1000 dm
    assert_eq!(p0.point.pos.unwrap().0, trackcraft::domain::track::deg_to_e7(570_000_000.0 * 180.0 / 2_147_483_648.0));
    assert_eq!(p0.point.ele_dm, Some(1000));
    assert_eq!(p0.point.hr, Some(120));
    assert_eq!(p0.speed_mps, Some(5.0));
    let last = s.sections[0].last().unwrap();
    assert_eq!((last.point.hr, last.point.ele_dm), (None, None));
}

#[test]
fn records_follow_session_start_times() {
    let mut msgs = vec![file_id()];
    msgs.extend(records(T0 - 5, 3)); // vor der ersten Session -> erste Session
    msgs.extend(records(T0, 10));
    msgs.push(session(T0, 10, typedef::Sport::RUNNING, typedef::SubSport::GENERIC));
    msgs.extend(records(T0 + 100, 7)); // nach dem Start der zweiten Session
    msgs.push(session(T0 + 100, 7, typedef::Sport::CYCLING, typedef::SubSport::MOUNTAIN));
    let f = import::parse(&encode(msgs, false, false)).unwrap();
    assert_eq!(f.sessions.len(), 2);
    assert_eq!(f.sessions[0].source_sport.as_deref(), Some("running"), "generic ergänzt nichts");
    assert_eq!(f.sessions[0].point_count(), 13);
    assert_eq!(f.sessions[1].source_sport.as_deref(), Some("cycling-mountain"));
    assert_eq!(f.sessions[1].point_count(), 7);
    assert_eq!(f.sessions[1].session_index, 1);
}

#[test]
fn file_without_session_message_is_one_session_without_file_values() {
    let mut msgs = vec![file_id()];
    msgs.extend(records(T0, 5));
    let f = import::parse(&encode(msgs, false, false)).unwrap();
    assert_eq!(f.sessions.len(), 1);
    assert_eq!(f.sessions[0].file, import::FileValues::default());
    assert_eq!(f.sessions[0].source_sport, None);
    assert_eq!(f.sessions[0].point_count(), 5);
}

#[test]
fn chained_big_endian_and_compressed_files_read_the_same() {
    let part = |from: i64, big: bool, comp: bool| {
        let mut m = vec![file_id()];
        m.extend(records(from, 6));
        m.push(session(from, 6, typedef::Sport::CYCLING, typedef::SubSport::GENERIC));
        encode(m, big, comp)
    };
    let plain = import::parse(&part(T0, false, false)).unwrap();
    for (big, comp) in [(true, false), (false, true), (true, true)] {
        let other = import::parse(&part(T0, big, comp)).unwrap();
        assert_eq!(plain.sessions[0].sections, other.sessions[0].sections, "Big-Endian {big}, komprimierte Zeitstempel {comp}");
    }
    // verkettet: Sessions zählen über beide Teildateien
    let mut chained = part(T0, false, false);
    chained.extend(part(T0 + 1000, true, true));
    let f = import::parse(&chained).unwrap();
    assert_eq!(f.sessions.len(), 2);
    assert_eq!(f.sessions[1].session_index, 1);
    assert_eq!(f.sessions[1].file.start_ms, Some((T0 + 1000) * 1000));
}

#[test]
fn damaged_files_are_rejected_but_a_wrong_file_checksum_is_tolerated() {
    let mut msgs = vec![file_id()];
    msgs.extend(records(T0, 5));
    msgs.push(session(T0, 5, typedef::Sport::CYCLING, typedef::SubSport::GENERIC));
    let good = encode(msgs, false, false);
    assert_eq!(good[0], 14, "Generator schreibt 14-Byte-Kopf mit Prüfsumme");
    assert!(!import::parse(&good).unwrap().checksum_ignored);

    let truncated = &good[..good.len() - 20];
    assert!(matches!(import::parse(truncated), Err(ImportError::InvalidFile(_))), "abgebrochene Datei");

    // Falsche Kopf-Prüfsumme bleibt ein harter Fehler.
    let mut bad_header = good.clone();
    bad_header[12] ^= 0xFF;
    assert!(matches!(import::parse(&bad_header), Err(ImportError::InvalidFile(_))), "falsche Kopf-Prüfsumme");

    // Nur die Datei-Prüfsumme ist falsch, der Inhalt ist lesbar: angenommen, mit Kennzeichen.
    let mut bad_crc = good.clone();
    let n = bad_crc.len();
    bad_crc[n - 1] ^= 0xFF;
    let read = import::parse(&bad_crc).expect("falsche Datei-Prüfsumme bei lesbarem Inhalt");
    assert!(read.checksum_ignored);
    assert_eq!(read.sessions[0].sections, import::parse(&good).unwrap().sessions[0].sections);

    // Auch in einer verketteten Datei: die zweite Teildatei mit falscher Datei-Prüfsumme.
    let mut chained = good.clone();
    chained.extend(&bad_crc);
    let read = import::parse(&chained).unwrap();
    assert!(read.checksum_ignored);
    assert_eq!(read.sessions.len(), 2);

    // Kopf-Prüfsumme 0 („nicht gesetzt“): Die Datei-Prüfsumme schließt nach FIT-Protokoll den Kopf ein. Solche
    // Dateien (Garmin Edge 2016–2019) sind intakt und brauchen keinen Hinweis; rustyfit bis 0.11.1 rechnete hier falsch.
    let mut no_header_crc = good.clone();
    no_header_crc[12] = 0;
    no_header_crc[13] = 0;
    let n = no_header_crc.len();
    let crc = crc16(&no_header_crc[..n - 2]).to_le_bytes();
    no_header_crc[n - 2..].copy_from_slice(&crc);
    let read = import::parse(&no_header_crc).expect("Kopf ohne Prüfsumme");
    assert!(!read.checksum_ignored);

    // Strukturfehler (Datensatz ohne Definition) weist die Datei weiter ab, auch wenn die Prüfsumme übergangen wird.
    let mut broken = bad_crc.clone();
    let data_start = 14;
    broken[data_start] = 0x0F; // Datensatz-Kopf mit lokaler Nummer 15, nie definiert
    assert!(matches!(import::parse(&broken), Err(ImportError::InvalidFile(_))), "Strukturfehler");
}
