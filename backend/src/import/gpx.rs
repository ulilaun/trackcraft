//! GPX-Parser (RFC-0008 Abschnitt 3, RFC-0011 „XML“): nur `<trk>/<trkseg>/<trkpt>`, Zeit, Höhe und die
//! Garmin-TrackPointExtension `hr`. Eine Datei mit `<!DOCTYPE` wird abgewiesen, Entitäten werden nie aufgelöst
//! außer den fünf vordefinierten und Zeichenreferenzen. Routen (`<rte>`) und Wegpunkte (`<wpt>`) werden ignoriert.

use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

use super::{FileValues, ImportError, ParsedSession, RawPoint};
use crate::domain::track::{HR_MAX, HR_MIN, LAT_MAX, LON_MAX, Point, deg_to_e7, round_half_away};

fn invalid(msg: impl Into<String>) -> ImportError {
    ImportError::InvalidFile(msg.into())
}

/// Lokaler Name ohne Präfix, z. B. `gpxtpx:hr` → `hr`.
fn local(name: &str) -> String {
    match name.rsplit_once(':') {
        Some((_, l)) => l.to_string(),
        None => name.to_string(),
    }
}

fn attr(e: &BytesStart, key: &str) -> Result<Option<String>, ImportError> {
    for a in e.attributes() {
        let a = a.map_err(|e| invalid(format!("Attribut: {e}")))?;
        if local(a.key.as_ref()) == key {
            return Ok(Some(
                a.normalized_value(quick_xml::XmlVersion::Implicit1_0).map_err(|e| invalid(format!("Attribut: {e}")))?.to_string(),
            ));
        }
    }
    Ok(None)
}

fn predefined_entity(name: &str) -> Option<char> {
    match name {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        _ => None,
    }
}

/// Normalisiert `<type>`: Kleinbuchstaben, jede Folge anderer Zeichen als Buchstaben/Ziffern zu `_`,
/// außen ohne `_` (RFC-0008 Abschnitt 4). `Road Biking` wird `road_biking`.
pub fn normalize_type(s: &str) -> Option<String> {
    let mut out = String::new();
    let mut last_us = false;
    for c in s.trim().to_lowercase().chars() {
        if c.is_alphanumeric() {
            out.push(c);
            last_us = false;
        } else if !last_us {
            out.push('_');
            last_us = true;
        }
    }
    let out = out.trim_matches('_').to_string();
    (!out.is_empty()).then_some(out)
}

fn parse_time(s: &str) -> Option<i64> {
    let s = s.trim();
    chrono::DateTime::parse_from_rfc3339(s).ok().map(|d| d.timestamp_millis()).or_else(|| {
        // Manche Exporte lassen die Zone weg; dann gilt UTC.
        chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f").ok().map(|d| d.and_utc().timestamp_millis())
    })
}

#[derive(Default)]
struct Pending {
    pos: Option<(i32, i32)>,
    ele: Option<f64>,
    time: Option<i64>,
    hr: Option<f64>,
}

#[derive(Default)]
struct State {
    stack: Vec<String>,
    text: String,
    sections: Vec<Vec<RawPoint>>,
    segment: Option<Vec<RawPoint>>,
    point: Option<Pending>,
    trk_count: usize,
    trk_name: Option<String>,
    trk_type: Option<String>,
    meta_name: Option<String>,
    saw_gpx: bool,
}

impl State {
    fn parent(&self) -> &str {
        self.stack.last().map(String::as_str).unwrap_or("")
    }

    fn open(&mut self, e: &BytesStart) -> Result<(), ImportError> {
        let name = local(e.name().as_ref());
        match (name.as_str(), self.parent()) {
            ("gpx", "") => self.saw_gpx = true,
            ("trk", "gpx") => self.trk_count += 1,
            ("trkseg", "trk") => self.segment = Some(Vec::new()),
            ("trkpt", "trkseg") => {
                let lat = attr(e, "lat")?.and_then(|v| v.trim().parse::<f64>().ok());
                let lon = attr(e, "lon")?.and_then(|v| v.trim().parse::<f64>().ok());
                let pos = match (lat, lon) {
                    (Some(la), Some(lo)) => Some((deg_to_e7(la), deg_to_e7(lo)))
                        .filter(|&(a, b)| a.unsigned_abs() <= LAT_MAX as u32 && b.unsigned_abs() <= LON_MAX as u32),
                    _ => None,
                };
                self.point = Some(Pending { pos, ..Default::default() });
            }
            _ => {}
        }
        self.stack.push(name);
        self.text.clear();
        Ok(())
    }

    fn close(&mut self) {
        let name = self.stack.pop().unwrap_or_default();
        let text = std::mem::take(&mut self.text);
        let value = text.trim();
        let in_point = self.point.is_some();
        match (name.as_str(), self.parent()) {
            ("ele", "trkpt") if in_point => self.point.as_mut().unwrap().ele = value.parse::<f64>().ok(),
            ("time", "trkpt") if in_point => self.point.as_mut().unwrap().time = parse_time(value),
            // Garmin TrackPointExtension v1/v2: …/extensions/TrackPointExtension/hr
            ("hr", "TrackPointExtension") if in_point => self.point.as_mut().unwrap().hr = value.parse::<f64>().ok(),
            ("trkpt", "trkseg") => {
                if let (Some(p), Some(seg)) = (self.point.take(), self.segment.as_mut()) {
                    let point = Point {
                        t_ms: p.time,
                        pos: p.pos,
                        ele_dm: p.ele.map(|m| round_half_away(m * 10.0) as i32),
                        hr: p.hr.map(round_half_away).filter(|h| (HR_MIN as i64..=HR_MAX as i64).contains(h)).map(|h| h as u16),
                    };
                    if !point.is_empty() {
                        seg.push(RawPoint { point, speed_mps: None, distance_m: None });
                    }
                }
            }
            ("trkseg", "trk") => {
                if let Some(seg) = self.segment.take().filter(|s| !s.is_empty()) {
                    self.sections.push(seg);
                }
            }
            ("name", "trk") if self.trk_count == 1 && self.trk_name.is_none() && !value.is_empty() => {
                self.trk_name = Some(value.to_string())
            }
            ("type", "trk") if self.trk_count == 1 && self.trk_type.is_none() => self.trk_type = Some(value.to_string()),
            ("name", "metadata") if !value.is_empty() => self.meta_name = Some(value.to_string()),
            _ => {}
        }
    }
}

pub fn parse(bytes: &[u8]) -> Result<ParsedSession, ImportError> {
    let text = std::str::from_utf8(bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes)).map_err(|_| invalid("GPX ist kein UTF-8"))?;
    let mut reader = Reader::from_str(text);
    reader.config_mut().check_end_names = true;
    let mut st = State::default();

    loop {
        match reader.read_event().map_err(|e| invalid(format!("XML: {e}")))? {
            Event::DocType(_) => return Err(invalid("GPX mit DOCTYPE wird abgewiesen")),
            Event::Start(e) => st.open(&e)?,
            Event::Empty(e) => {
                st.open(&e)?;
                st.close();
            }
            Event::End(_) => st.close(),
            Event::Text(t) => st.text.push_str(&t.xml_content(quick_xml::XmlVersion::Implicit1_0)),
            Event::CData(t) => st.text.push_str(&t.into_inner()),
            Event::GeneralRef(r) => {
                if r.is_char_ref() {
                    match r.resolve_char_ref() {
                        Ok(Some(c)) => st.text.push(c),
                        _ => return Err(invalid("ungültige Zeichenreferenz")),
                    }
                } else {
                    let name = r.into_inner();
                    match predefined_entity(&name) {
                        Some(c) => st.text.push(c),
                        None => return Err(invalid(format!("Entität &{name}; wird nicht aufgelöst"))),
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if !st.saw_gpx {
        return Err(invalid("kein <gpx>-Element"));
    }
    if !st.sections.iter().flatten().any(|p| p.point.t_ms.is_some()) {
        return Err(ImportError::GpxWithoutTime);
    }
    Ok(ParsedSession {
        session_index: 0,
        source_sport: st.trk_type.as_deref().and_then(normalize_type),
        file: FileValues::default(),
        sections: st.sections,
        title: st.trk_name.or(st.meta_name).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
    })
}
