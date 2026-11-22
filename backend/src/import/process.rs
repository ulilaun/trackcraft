//! Aufbereitung je Session (RFC-0008 Abschnitte 4 bis 6): Sportklasse, Fahrzeugerkennung, Outlier-Filter,
//! Kennzahlen, Höhenhysterese und Titel. Rein rechnend, ohne Datenbank.

use std::collections::BTreeMap;

use chrono::TimeZone;

use super::{ParsedSession, RawPoint};
use crate::config::Config;
use crate::domain::track::Point;
use crate::geo::distance_m;
use crate::sports::SportMapping;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartOrigin {
    File,
    FirstPoint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeOrigin {
    File,
    GpsPoints,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueOrigin {
    File,
    /// Distanz aus GPS-Punkten bzw. Herzfrequenz aus Messpunkten
    Points,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Elevation {
    pub gain_m: f64,
    pub loss_m: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VehicleCriterion {
    Peak,
    Average,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VehicleFinding {
    pub criterion: VehicleCriterion,
    pub from_ms: i64,
    pub to_ms: i64,
    pub max_kmh: f64,
    pub limit_kmh: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Lock {
    /// Mehr Punkte als `max_points_per_session`
    PointLimit {
        count: usize,
        max: u64,
    },
    Vehicle(VehicleFinding),
}

impl Lock {
    pub fn code(&self) -> &'static str {
        match self {
            Self::PointLimit { .. } => "locked_limit",
            Self::Vehicle(_) => "locked_vehicle",
        }
    }
}

/// Eine aufbereitete Session, bereit zum Anzeigen im Importdialog und zum Speichern.
#[derive(Debug, Clone)]
pub struct Prepared {
    pub session_index: usize,
    pub source_sport: Option<String>,
    pub sport_class: &'static str,
    /// Schlüssel im Untertyp-Katalog
    pub sub_sport: Option<String>,
    /// Bezeichnung des Untertyps für Ersatztitel und Importdialog
    pub sub_sport_label: Option<String>,
    /// Titel aus der Datei oder Ersatztitel; ohne Start erst nach der Zeiteingabe bestimmbar
    pub title: Option<String>,
    pub start: Option<(i64, StartOrigin)>,
    pub total_time: Option<(i64, TimeOrigin)>,
    pub distance: Option<(f64, ValueOrigin)>,
    pub ascent_file_m: Option<f64>,
    /// Hysterese über die gespeicherten Punkte (Berechnungsversion, RFC-0007)
    pub elevation: Option<Elevation>,
    pub avg_speed_kmh: Option<f64>,
    pub avg_hr: Option<(u16, ValueOrigin)>,
    /// Gefilterte Abschnitte, jeder mit mindestens einem Punkt
    pub sections: Vec<Vec<Point>>,
    pub filtered_outliers_count: usize,
    pub max_speed_threshold_kmh: Option<f64>,
    pub lock: Option<Lock>,
    /// Weder GPS-Punkte noch Zeitangaben: Start und Gesamtzeit werden im Dialog abgefragt (RFC-0008 Abschnitt 7)
    pub needs_time_input: bool,
}

impl Prepared {
    pub fn point_count(&self) -> usize {
        self.sections.iter().map(Vec::len).sum()
    }
}

/// Wert je `source_sport`: erst der volle Schlüssel, dann der Teil vor dem ersten `-` (RFC-0008 Abschnitt 4).
fn lookup<'a, V>(map: &'a BTreeMap<String, V>, source: Option<&str>) -> Option<&'a V> {
    let s = source?;
    map.get(s).or_else(|| s.split_once('-').and_then(|(sport, _)| map.get(sport)))
}

pub fn class_label(class: &str) -> &'static str {
    match class {
        "cycling" => "Radfahren",
        "ebike" => "E-Bike",
        "running" => "Laufen",
        "skiing" => "Skifahren",
        "swimming" => "Schwimmen",
        "studio" => "Studio",
        _ => "Aktivität",
    }
}

/// Ersatztitel „Bezeichnung TT.MM.JJJJ HH:MM“ in der Konfigurationszone (RFC-0008 Abschnitt 6).
pub fn fallback_title(sub_sport_label: Option<&str>, class: &str, start_ms: i64, tz: chrono_tz::Tz) -> String {
    let label = sub_sport_label.map(str::to_string).unwrap_or_else(|| class_label(class).to_string());
    let local = tz.timestamp_millis_opt(start_ms).single().map(|d| d.format("%d.%m.%Y %H:%M").to_string()).unwrap_or_default();
    format!("{label} {local}").trim().to_string()
}

pub fn prepare(session: &ParsedSession, cfg: &Config, mapping: &SportMapping, tz: chrono_tz::Tz) -> Prepared {
    let source = session.source_sport.as_deref();
    // Zuordnung aus der Datenbank; Untertyp nur über den exakten Schlüssel.
    let resolved = mapping.resolve(source);
    let sport_class = resolved.sport_class;
    let sub_sport = resolved.sub_sport;
    let sub_sport_label = sub_sport.as_deref().and_then(|k| mapping.label(sport_class, k)).map(str::to_string);

    // Outlier-Filter je Abschnitt
    let of = &cfg.processing.outlier_filter;
    let threshold = of.enabled.then(|| *lookup(&of.max_speed_kmh, source).unwrap_or(&of.default_max_speed_kmh));
    let mut filtered_outliers_count = 0;
    let sections: Vec<Vec<Point>> = session
        .sections
        .iter()
        .map(|raw| {
            let pts: Vec<Point> = raw.iter().map(|r| r.point).collect();
            match threshold {
                Some(max) => {
                    let (out, dropped) = outlier_filter(&pts, max);
                    filtered_outliers_count += dropped;
                    out
                }
                None => pts,
            }
        })
        .filter(|s| !s.is_empty())
        .collect();

    // Sperren: Punktlimit vor Fahrzeug
    let raw_count = session.point_count();
    let lock = if raw_count as u64 > cfg.import.max_points_per_session {
        Some(Lock::PointLimit { count: raw_count, max: cfg.import.max_points_per_session })
    } else {
        detect_vehicle(session, &sections, cfg).map(Lock::Vehicle)
    };

    // Kennzahlen
    let all: Vec<&Point> = sections.iter().flatten().collect();
    let first_time = all.iter().find_map(|p| p.t_ms);
    let gps: Vec<&&Point> = all.iter().filter(|p| p.pos.is_some() && p.t_ms.is_some()).collect();
    let start = match (session.file.start_ms, first_time) {
        (Some(s), _) => Some((s, StartOrigin::File)),
        (None, Some(t)) => Some((t, StartOrigin::FirstPoint)),
        _ => None,
    };
    let total_time = match session.file.elapsed_ms {
        Some(ms) => Some((ms, TimeOrigin::File)),
        None => match (gps.first().and_then(|p| p.t_ms), gps.last().and_then(|p| p.t_ms)) {
            (Some(a), Some(b)) if b >= a => Some((b - a, TimeOrigin::GpsPoints)),
            _ => None,
        },
    };
    let distance = match session.file.distance_m {
        Some(d) => Some((d, ValueOrigin::File)),
        None => gps_distance(&sections).map(|d| (d, ValueOrigin::Points)),
    };
    let avg_speed_kmh = match (distance, total_time) {
        (Some((d, _)), Some((ms, _))) if ms > 0 => Some(d / (ms as f64 / 1000.0) * 3.6),
        _ => None,
    };
    let avg_hr = match session.file.avg_hr {
        Some(h) => Some((h, ValueOrigin::File)),
        None => {
            let hrs: Vec<f64> = all.iter().filter_map(|p| p.hr.map(f64::from)).collect();
            (!hrs.is_empty())
                .then(|| crate::domain::track::round_half_away(hrs.iter().sum::<f64>() / hrs.len() as f64) as u16)
                .map(|h| (h, ValueOrigin::Points))
        }
    };
    let elevation = hysteresis(&sections, cfg.processing.elevation.hysteresis_m);
    let needs_time_input = start.is_none() || total_time.is_none();
    let title = session.title.clone().or_else(|| start.map(|(s, _)| fallback_title(sub_sport_label.as_deref(), sport_class, s, tz)));

    Prepared {
        session_index: session.session_index,
        source_sport: session.source_sport.clone(),
        sport_class,
        sub_sport,
        sub_sport_label,
        title,
        start,
        total_time,
        distance,
        ascent_file_m: session.file.ascent_m,
        elevation,
        avg_speed_kmh,
        avg_hr,
        sections,
        filtered_outliers_count,
        max_speed_threshold_kmh: threshold,
        lock,
        needs_time_input,
    }
}

/// Geschwindigkeit in km/h zwischen zwei Punkten mit Position und Zeit; bei Zeitdifferenz ≤ 0 unendlich,
/// außer die Position ist gleich.
fn speed_kmh(a: &Point, b: &Point) -> f64 {
    let (Some(pa), Some(pb), Some(ta), Some(tb)) = (a.pos, b.pos, a.t_ms, b.t_ms) else { return 0.0 };
    let d = distance_m(pa, pb);
    let dt = (tb - ta) as f64 / 1000.0;
    if dt <= 0.0 { if d > 0.0 { f64::INFINITY } else { 0.0 } } else { d / dt * 3.6 }
}

/// Outlier-Filter für einen Abschnitt (RFC-0008 Abschnitt 5). Liefert die Punkte und die Zahl der Verworfenen.
pub fn outlier_filter(points: &[Point], max_kmh: f64) -> (Vec<Point>, usize) {
    let checked = |p: &Point| p.pos.is_some() && p.t_ms.is_some();
    let idx: Vec<usize> = (0..points.len()).filter(|&i| checked(&points[i])).collect();
    let mut out: Vec<Option<Point>> = points.iter().copied().map(Some).collect();
    let mut dropped = 0;

    // Erster Bezugspunkt: Geschwindigkeit zu mehr als der Hälfte der nächsten 5 geprüften Punkte höchstens max
    // (3 von 5; bei weniger Folgepunkten die Mehrheit der vorhandenen). Ein schlechter Kaltstart fällt durch,
    // ein einzelner Sprung kurz nach dem Start reißt keine guten Punkte mit.
    let mut k = 0;
    while k < idx.len() {
        let cand = &points[idx[k]];
        let next: Vec<usize> = idx[k + 1..].iter().take(5).copied().collect();
        let fitting = next.iter().filter(|&&j| speed_kmh(cand, &points[j]) <= max_kmh).count();
        if fitting > next.len() / 2 || next.is_empty() {
            break;
        }
        out[idx[k]].as_mut().unwrap().pos = None;
        dropped += 1;
        k += 1;
    }
    let Some(&first) = idx.get(k) else {
        return (out.into_iter().flatten().collect(), dropped);
    };

    let mut last = points[first];
    for &i in &idx[k + 1..] {
        let p = &points[i];
        if p.t_ms == last.t_ms {
            // Gleicher Zeitstempel: der erste zählt, der ganze Punkt entfällt.
            out[i] = None;
            dropped += 1;
        } else if speed_kmh(&last, p) > max_kmh {
            // Nur die Position wird verworfen; Zeit, Höhe und HF bleiben.
            out[i].as_mut().unwrap().pos = None;
            dropped += 1;
        } else {
            last = *p;
        }
    }
    let out: Vec<Point> = out.into_iter().flatten().filter(|p| !p.is_empty()).collect();
    (out, dropped)
}

/// GPS-Distanz je Abschnitt über aufeinanderfolgende Punkte mit Position; `None` ohne zwei Positionen.
pub fn gps_distance(sections: &[Vec<Point>]) -> Option<f64> {
    let mut total = 0.0;
    let mut any = false;
    for s in sections {
        let mut prev: Option<(i32, i32)> = None;
        for p in s {
            if let Some(pos) = p.pos {
                if let Some(q) = prev {
                    total += distance_m(q, pos);
                    any = true;
                }
                prev = Some(pos);
            }
        }
    }
    any.then_some(total)
}

/// Höhenhysterese je Abschnitt, summiert ohne Verbindung über Abschnittsgrenzen (RFC-0008 „Höhenhysterese“).
pub fn hysteresis(sections: &[Vec<Point>], threshold_m: f64) -> Option<Elevation> {
    let mut gain = 0.0;
    let mut loss = 0.0;
    let mut known = false;
    for s in sections {
        let heights: Vec<f64> = s.iter().filter_map(|p| p.ele_dm).map(|dm| dm as f64 / 10.0).collect();
        if heights.len() >= 2 {
            known = true;
        }
        let Some(&first) = heights.first() else { continue };
        let mut r = first;
        for &h in &heights[1..] {
            let d = h - r;
            // Vergleich mit kleiner Toleranz, damit 103,0 − 100,0 = 3 trotz Gleitkomma als erreicht gilt.
            if d >= threshold_m - 1e-9 {
                gain += d;
                r = h;
            } else if d <= -threshold_m + 1e-9 {
                loss += -d;
                r = h;
            }
        }
    }
    known.then_some(Elevation { gain_m: gain, loss_m: loss })
}

/// Fahrzeugerkennung je Session (RFC-0008 Abschnitt 5.1). Liefert den ersten erkannten Abschnitt.
pub fn detect_vehicle(session: &ParsedSession, filtered: &[Vec<Point>], cfg: &Config) -> Option<VehicleFinding> {
    let vd = &cfg.processing.vehicle_detection;
    if !vd.enabled {
        return None;
    }
    let source = session.source_sport.as_deref();
    let raw: Vec<&RawPoint> = session.sections.iter().flatten().collect();
    let mut findings = Vec::new();

    if let Some(&limit) = lookup(&vd.peak_speed_kmh, source) {
        // Gerätegeschwindigkeit der Records, sonst Geschwindigkeit zwischen Punkten vor dem Filter.
        let samples: Vec<(i64, f64)> = if raw.iter().any(|p| p.speed_mps.is_some()) {
            raw.iter().filter_map(|p| Some((p.point.t_ms?, p.speed_mps? * 3.6))).collect()
        } else {
            session
                .sections
                .iter()
                .flat_map(|s| {
                    let gps: Vec<&RawPoint> = s.iter().filter(|p| p.point.pos.is_some() && p.point.t_ms.is_some()).collect();
                    gps.windows(2)
                        .filter(|w| w[1].point.t_ms > w[0].point.t_ms)
                        .map(|w| (w[1].point.t_ms.unwrap(), speed_kmh(&w[0].point, &w[1].point)))
                        .collect::<Vec<_>>()
                })
                .collect()
        };
        if let Some(f) = peak_run(&samples, limit, vd.min_duration_s as i64 * 1000, vd.max_pause_s as i64 * 1000) {
            findings.push(f);
        }
    }

    if let Some(&limit) = lookup(&vd.avg_speed_kmh, source) {
        // Distanz der Records, sonst GPS-Distanz über die gefilterten Punkte.
        let series: Vec<(i64, f64)> = if raw.iter().any(|p| p.distance_m.is_some()) {
            raw.iter().filter_map(|p| Some((p.point.t_ms?, p.distance_m?))).collect()
        } else {
            let mut acc = 0.0;
            let mut out = Vec::new();
            for s in filtered {
                let mut prev: Option<(i32, i32)> = None;
                for p in s {
                    if let (Some(pos), Some(t)) = (p.pos, p.t_ms) {
                        if let Some(q) = prev {
                            acc += distance_m(q, pos);
                        }
                        prev = Some(pos);
                        out.push((t, acc));
                    }
                }
            }
            out
        };
        if let Some(f) = average_window(&series, limit, vd.window_s as i64 * 1000, vd.max_pause_s as i64 * 1000) {
            findings.push(f);
        }
    }
    findings.into_iter().min_by_key(|f| f.from_ms)
}

/// Erste ununterbrochene Folge über `limit` mit mindestens `min_ms` Dauer.
fn peak_run(samples: &[(i64, f64)], limit: f64, min_ms: i64, max_pause_ms: i64) -> Option<VehicleFinding> {
    let mut run: Option<(i64, i64, f64)> = None; // (von, bis, max)
    let mut prev_t: Option<i64> = None;
    for &(t, v) in samples {
        let gap = prev_t.is_some_and(|p| t - p > max_pause_ms || t < p);
        prev_t = Some(t);
        if v > limit && !gap {
            run = Some(match run {
                Some((a, _, m)) => (a, t, m.max(v)),
                None => (t, t, v),
            });
        } else {
            if let Some((a, b, m)) = run.take()
                && b - a >= min_ms
            {
                return Some(VehicleFinding { criterion: VehicleCriterion::Peak, from_ms: a, to_ms: b, max_kmh: m, limit_kmh: limit });
            }
            run = (v > limit).then_some((t, t, v));
        }
    }
    match run {
        Some((a, b, m)) if b - a >= min_ms => {
            Some(VehicleFinding { criterion: VehicleCriterion::Peak, from_ms: a, to_ms: b, max_kmh: m, limit_kmh: limit })
        }
        _ => None,
    }
}

/// Erstes Fenster von `window_ms`, dessen Schnitt über `limit` liegt; Fenster mit Pause über `max_pause_ms` zählen nicht.
/// Der gemeldete Abschnitt umfasst alle direkt anschließenden Fenster über der Grenze.
fn average_window(series: &[(i64, f64)], limit: f64, window_ms: i64, max_pause_ms: i64) -> Option<VehicleFinding> {
    if series.len() < 2 {
        return None;
    }
    // Für jeden Endpunkt j der früheste Start i mit t_j − t_i ≥ Fenster; Pausen im Fenster machen es ungültig.
    let mut found: Option<VehicleFinding> = None;
    let mut i = 0;
    let mut last_gap_end = 0usize; // Index nach der letzten zu großen Pause
    for j in 1..series.len() {
        if series[j].0 - series[j - 1].0 > max_pause_ms || series[j].0 < series[j - 1].0 {
            last_gap_end = j;
        }
        while i + 1 < j && series[j].0 - series[i + 1].0 >= window_ms {
            i += 1;
        }
        let span = series[j].0 - series[i].0;
        let valid = span >= window_ms && i >= last_gap_end;
        let avg = if valid { (series[j].1 - series[i].1) / (span as f64 / 1000.0) * 3.6 } else { 0.0 };
        match (&mut found, valid && avg > limit) {
            (None, true) => {
                found = Some(VehicleFinding {
                    criterion: VehicleCriterion::Average,
                    from_ms: series[i].0,
                    to_ms: series[j].0,
                    max_kmh: avg,
                    limit_kmh: limit,
                })
            }
            (Some(f), true) => {
                f.to_ms = series[j].0;
                f.max_kmh = f.max_kmh.max(avg);
            }
            (Some(_), false) => return found,
            (None, false) => {}
        }
    }
    found
}

/// Meldung für `lock_reason` (RFC-0008 Abschnitt 5.1), Zeiten lokal.
pub fn vehicle_message(f: &VehicleFinding, sport_class: &str, tz: chrono_tz::Tz, window_s: u64) -> String {
    let hm = |ms: i64| tz.timestamp_millis_opt(ms).single().map(|d| d.format("%H:%M").to_string()).unwrap_or_default();
    let what = match f.criterion {
        VehicleCriterion::Peak => format!("bis zu {:.0} km/h", f.max_kmh),
        VehicleCriterion::Average if window_s.is_multiple_of(60) => {
            format!("im {}-Minuten-Schnitt bis zu {:.0} km/h", window_s / 60, f.max_kmh)
        }
        VehicleCriterion::Average => format!("im {window_s}-Sekunden-Schnitt bis zu {:.0} km/h", f.max_kmh),
    };
    format!(
        "Zwischen {} und {} {what}, über der Grenze für {} ({:.0} km/h). Vermutlich Fahrzeug, bitte Aktivität vor dem Import kürzen.",
        hm(f.from_ms),
        hm(f.to_ms),
        class_label(sport_class),
        f.limit_kmh
    )
}
