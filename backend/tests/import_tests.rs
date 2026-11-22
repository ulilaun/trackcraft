//! Import ohne Datenbank (RFC-0008, RFC-0011, RFC-0014 Fälle 18 bis 20, 35, 36): Formaterkennung, GPX-Parser,
//! Outlier-Filter, Höhenhysterese, Fahrzeugerkennung, Kennzahlen und Titel.

mod common;

use trackcraft::config;
use trackcraft::domain::track::Point;
use trackcraft::import::process::{self, Lock, StartOrigin, TimeOrigin, ValueOrigin, VehicleCriterion};
use trackcraft::import::{self, Format, ImportError, ParsedSession, RawPoint, gpx};

fn cfg() -> config::Config {
    let dir = common::temp();
    let extra = r#"
[processing.outlier_filter.max_speed_kmh]
running = 35.0
cycling = 110.0

[processing.vehicle_detection.peak_speed_kmh]
running = 35.0
cycling = 90.0

[processing.vehicle_detection.avg_speed_kmh]
running = 20.0
cycling = 70.0
"#;
    let text = common::config_text(dir.path(), "", extra);
    config::parse(&dir.path().join("t.toml"), &text).unwrap().raw
}

fn tz() -> chrono_tz::Tz {
    chrono_tz::Europe::Berlin
}

/// Punkt in Nord-Richtung: `m` Meter vom Start (1 m ≈ 89,9 Einheiten 1e-7 Grad Breite).
fn pt(t_s: i64, north_m: f64) -> Point {
    let lat = 478_520_000 + (north_m / 111_195.0 * 1e7).round() as i32;
    Point { t_ms: Some(1_700_000_000_000 + t_s * 1000), pos: Some((lat, 117_350_000)), ele_dm: None, hr: None }
}

// ---------- Formaterkennung und GPX ----------

#[test]
fn detects_formats_by_content() {
    let mut fit = vec![14u8, 0x20, 0, 0, 0, 0, 0, 0];
    fit.extend_from_slice(b".FIT");
    assert_eq!(import::detect(&fit).unwrap(), Format::Fit);
    assert_eq!(import::detect(b"\xEF\xBB\xBF  <?xml version=\"1.0\"?><gpx></gpx>").unwrap(), Format::Gpx);
    assert_eq!(import::detect(b"PK\x03\x04zipdata"), Err(ImportError::UnsupportedFormat));
    assert_eq!(import::detect(b"<?xml version=\"1.0\"?><tcx/>"), Err(ImportError::UnsupportedFormat));
    assert_eq!(import::detect(b""), Err(ImportError::UnsupportedFormat));
}

const GPX: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<gpx version="1.1" creator="Test" xmlns="http://www.topografix.com/GPX/1/1"
     xmlns:ns3="http://www.garmin.com/xmlschemas/TrackPointExtension/v2">
  <metadata><name>Metaname</name></metadata>
  <wpt lat="1" lon="1"><time>2026-01-01T00:00:00Z</time></wpt>
  <rte><rtept lat="1" lon="1"/></rte>
  <trk>
    <name>  Runde &amp; Co  </name>
    <type>Road Biking</type>
    <trkseg>
      <trkpt lat="47.852" lon="11.735"><ele>580.04</ele><time>2026-10-04T05:20:42Z</time>
        <extensions><ns3:TrackPointExtension><ns3:hr>120</ns3:hr></ns3:TrackPointExtension></extensions></trkpt>
      <trkpt lat="47.85205" lon="11.73503"><ele>580.05</ele><time>2026-10-04T05:20:43.500Z</time></trkpt>
    </trkseg>
    <trkseg/>
    <trkseg>
      <trkpt lat="47.86" lon="11.74"><time>2026-10-04T06:00:00+02:00</time></trkpt>
      <trkpt lat="47.86" lon="11.74"></trkpt>
    </trkseg>
  </trk>
</gpx>"#;

#[test]
fn gpx_reads_tracks_sections_and_extensions() {
    let s = gpx::parse(GPX.as_bytes()).unwrap();
    assert_eq!(s.title.as_deref(), Some("Runde & Co"));
    assert_eq!(s.source_sport.as_deref(), Some("road_biking"));
    assert_eq!(s.sections.len(), 2, "leere trkseg entfällt, rte/wpt ignoriert");
    let a = &s.sections[0][0].point;
    assert_eq!(a.pos, Some((478_520_000, 117_350_000)));
    assert_eq!(a.ele_dm, Some(5800));
    assert_eq!(a.hr, Some(120));
    let ms = |s: &str| chrono::DateTime::parse_from_rfc3339(s).unwrap().timestamp_millis();
    assert_eq!(a.t_ms, Some(ms("2026-10-04T05:20:42Z")));
    assert_eq!(s.sections[0][1].point.ele_dm, Some(5801), "580,05 m rundet weg von null");
    assert_eq!(s.sections[0][1].point.t_ms, Some(ms("2026-10-04T05:20:43.5Z")));
    assert_eq!(s.sections[1][0].point.t_ms, Some(ms("2026-10-04T04:00:00Z")), "Zeitzone +02:00");
    assert_eq!(s.sections[1][1].point.t_ms, None, "Punkt ohne Zeit bleibt gespeichert");
}

#[test]
fn gpx_title_falls_back_to_metadata_name() {
    let g = GPX.replace("<name>  Runde &amp; Co  </name>", "");
    assert_eq!(gpx::parse(g.as_bytes()).unwrap().title.as_deref(), Some("Metaname"));
}

#[test]
fn gpx_without_any_time_is_a_route() {
    let g = r#"<gpx><trk><trkseg><trkpt lat="1" lon="1"><ele>1</ele></trkpt></trkseg></trk></gpx>"#;
    assert_eq!(gpx::parse(g.as_bytes()).unwrap_err(), ImportError::GpxWithoutTime);
}

#[test]
fn gpx_security_rules() {
    let doctype = r#"<?xml version="1.0"?><!DOCTYPE gpx [<!ENTITY x "y">]><gpx></gpx>"#;
    assert!(matches!(gpx::parse(doctype.as_bytes()), Err(ImportError::InvalidFile(_))));
    let entity = GPX.replace("Runde &amp; Co", "Runde &xxe;");
    assert!(matches!(gpx::parse(entity.as_bytes()), Err(ImportError::InvalidFile(_))));
    let broken = GPX.replace("</trkseg>", "");
    assert!(matches!(gpx::parse(broken.as_bytes()), Err(ImportError::InvalidFile(_))));
    let charref = GPX.replace("Runde &amp; Co", "Runde &#228;");
    assert_eq!(gpx::parse(charref.as_bytes()).unwrap().title.as_deref(), Some("Runde ä"));
}

#[test]
fn gpx_type_normalization() {
    assert_eq!(gpx::normalize_type("Road Biking").as_deref(), Some("road_biking"));
    assert_eq!(gpx::normalize_type("  --Mountain  Bike!! ").as_deref(), Some("mountain_bike"));
    assert_eq!(gpx::normalize_type("9").as_deref(), Some("9"));
    assert_eq!(gpx::normalize_type(" - "), None);
}

// ---------- Höhenhysterese (Beispiele aus RFC-0008) ----------

fn ele(seq: &[Option<f64>]) -> Vec<Point> {
    seq.iter()
        .enumerate()
        .map(|(i, h)| Point { t_ms: Some(i as i64), ele_dm: h.map(|m| (m * 10.0).round() as i32), ..Default::default() })
        .collect()
}

type Case = (Vec<Vec<Point>>, Option<(f64, f64)>);

#[test]
fn hysteresis_examples() {
    let cases: Vec<Case> = vec![
        (vec![ele(&[Some(100.), Some(101.), Some(102.), Some(103.), Some(104.), Some(106.)])], Some((6., 0.))),
        (vec![ele(&[Some(100.), Some(102.), Some(102.9), Some(101.)])], Some((0., 0.))),
        (vec![ele(&[Some(100.), Some(103.)])], Some((3., 0.))),
        (vec![ele(&[Some(100.), Some(103.), Some(101.), Some(104.)])], Some((3., 0.))),
        (vec![ele(&[Some(100.), Some(96.), Some(100.)])], Some((4., 4.))),
        (vec![ele(&[Some(100.), None, Some(104.)])], Some((4., 0.))),
        (vec![ele(&[Some(100.), Some(105.), Some(100.), Some(105.)])], Some((10., 5.))),
        (vec![ele(&[Some(100.)]), ele(&[Some(102.), Some(105.)])], Some((3., 0.))),
        (vec![ele(&[Some(100.)])], None),
    ];
    for (i, (sections, expected)) in cases.into_iter().enumerate() {
        let got = process::hysteresis(&sections, 3.0).map(|e| (e.gain_m, e.loss_m));
        match (got, expected) {
            (Some((g, l)), Some((eg, el))) => assert!((g - eg).abs() < 1e-9 && (l - el).abs() < 1e-9, "Fall {i}: {g}/{l}"),
            (got, exp) => assert_eq!(got, exp, "Fall {i}"),
        }
    }
}

// ---------- Outlier-Filter ----------

#[test]
fn single_jump_removes_only_the_jump_point() {
    let mut pts: Vec<Point> = (0..20).map(|i| pt(i, i as f64 * 5.0)).collect();
    pts[10] = Point { ele_dm: Some(4000), hr: Some(130), ..pt(10, 900.0) }; // 900 m in 1 s
    let (out, dropped) = process::outlier_filter(&pts, 110.0);
    assert_eq!(dropped, 1);
    assert_eq!(out.len(), 20, "verworfen heißt Position verworfen");
    assert_eq!(out[10].pos, None);
    assert_eq!((out[10].t_ms, out[10].ele_dm, out[10].hr), (pts[10].t_ms, Some(4000), Some(130)));
    assert!(out.iter().enumerate().all(|(i, p)| i == 10 || p.pos.is_some()));
}

/// Mehrheitsregel für den ersten Bezugspunkt (RFC-0008 Abschnitt 5): Ein Sprung unter den ersten fünf Punkten
/// reißt die guten Punkte davor nicht mit, nur der Sprungpunkt verliert seine Position.
#[test]
fn jump_within_first_five_points_drops_only_the_jump() {
    let mut pts: Vec<Point> = (0..20).map(|i| pt(i, i as f64 * 5.0)).collect();
    pts[3] = pt(3, 900.0);
    let (out, dropped) = process::outlier_filter(&pts, 110.0);
    assert_eq!(dropped, 1);
    assert_eq!(out.len(), 20);
    assert!(out.iter().enumerate().all(|(i, p)| (i == 3) == p.pos.is_none()));
}

/// Mehrere schlechte Kaltstart-Punkte dicht beieinander: Jeder passt nur zu den anderen schlechten, nicht zur
/// Mehrheit der nächsten fünf, und fällt durch.
#[test]
fn cold_start_cluster_is_dropped() {
    let mut pts: Vec<Point> = (0..20).map(|i| pt(i, i as f64 * 5.0)).collect();
    for (i, p) in pts.iter_mut().enumerate().take(3) {
        *p = pt(i as i64, -2000.0 + i as f64);
    }
    let (out, dropped) = process::outlier_filter(&pts, 110.0);
    assert_eq!(dropped, 3);
    assert!(out[..3].iter().all(|p| p.pos.is_none()));
    assert!(out[3..].iter().all(|p| p.pos.is_some()));
}

/// Kurzer Abschnitt mit weniger als fünf Folgepunkten: Mehrheit der vorhandenen.
#[test]
fn short_section_uses_majority_of_available_points() {
    // P0 passt zu P1 und P2, nicht zu P3 (Sprung): 2 von 3, also Bezugspunkt; P3 wird verworfen.
    let pts = vec![pt(0, 0.0), pt(1, 5.0), pt(2, 10.0), pt(3, 900.0)];
    let (out, dropped) = process::outlier_filter(&pts, 110.0);
    assert_eq!(dropped, 1);
    assert_eq!(out[3].pos, None);
    // ein einzelner Punkt ist immer Bezugspunkt
    assert_eq!(process::outlier_filter(&[pt(0, 0.0)], 110.0).1, 0);
}

#[test]
fn bad_cold_start_does_not_take_good_points_with_it() {
    let mut pts: Vec<Point> = (0..10).map(|i| pt(i, i as f64 * 5.0)).collect();
    pts[0] = pt(0, -2000.0); // Kaltstart 2 km daneben
    let (out, dropped) = process::outlier_filter(&pts, 110.0);
    assert_eq!(dropped, 1);
    assert_eq!(out[0].pos, None);
    assert!(out[1..].iter().all(|p| p.pos.is_some()));
}

#[test]
fn duplicate_timestamp_drops_whole_point_and_unchecked_points_stay() {
    let mut pts = vec![pt(0, 0.0), pt(1, 5.0), pt(1, 6.0), pt(2, 10.0)];
    pts.push(Point { t_ms: None, ..pt(3, 9999.0) }); // ohne Zeit: nicht geprüft
    pts.push(Point { pos: None, hr: Some(100), ..pt(4, 0.0) }); // ohne Position: nicht geprüft
    let (out, dropped) = process::outlier_filter(&pts, 110.0);
    assert_eq!(dropped, 1);
    assert_eq!(out.len(), 5);
    assert_eq!(out[2].t_ms, pts[3].t_ms);
    assert!(out[3].pos.is_some(), "Punkt ohne Zeit wird nicht geprüft");
}

// ---------- Aufbereitung je Session ----------

fn session(source: &str, pts: Vec<RawPoint>) -> ParsedSession {
    ParsedSession { session_index: 0, source_sport: Some(source.into()), sections: vec![pts], ..Default::default() }
}

fn raw(p: Point) -> RawPoint {
    RawPoint { point: p, speed_mps: None, distance_m: None }
}

#[test]
fn metrics_from_points_without_file_values() {
    // 101 Punkte, 1 Hz, 5 m/s, Höhe steigt um 0,1 m je Punkt, HF 140/141 abwechselnd
    let pts: Vec<RawPoint> =
        (0..=100).map(|i| raw(Point { ele_dm: Some(5000 + i as i32), hr: Some(140 + (i % 2) as u16), ..pt(i, i as f64 * 5.0) })).collect();
    let p = process::prepare(&session("cycling-road", pts), &cfg(), &trackcraft::sports::SportMapping::defaults(), tz());
    assert_eq!(p.sport_class, "cycling");
    assert_eq!(p.sub_sport.as_deref(), Some("road"), "exakter Schlüssel der Zuordnung");
    assert_eq!(p.start.unwrap().1, StartOrigin::FirstPoint);
    assert_eq!(p.total_time, Some((100_000, TimeOrigin::GpsPoints)));
    let (d, origin) = p.distance.unwrap();
    assert_eq!(origin, ValueOrigin::Points);
    assert!((d - 500.0).abs() < 1.0, "{d}");
    assert!((p.avg_speed_kmh.unwrap() - 18.0).abs() < 0.05);
    assert_eq!(p.avg_hr, Some((140, ValueOrigin::Points)), "51× 140 und 50× 141 ergeben 140,495");
    let e = p.elevation.unwrap();
    assert!((e.gain_m - 9.0).abs() < 1e-9 && e.loss_m == 0.0, "{e:?}");
    assert_eq!(p.max_speed_threshold_kmh, Some(110.0));
    assert!(p.lock.is_none() && !p.needs_time_input);
    // Ersatztitel mit Untertyp und lokalem Start (14.11.2023 22:13 UTC = 23:13 Berlin)
    assert_eq!(p.title.as_deref(), Some("Rennrad 14.11.2023 23:13"));
    // ohne Untertyp-Zuordnung die Sportklasse
    let pts: Vec<RawPoint> = (0..=10).map(|i| raw(pt(i, i as f64 * 5.0))).collect();
    let p = process::prepare(&session("cycling-gravel", pts), &cfg(), &trackcraft::sports::SportMapping::defaults(), tz());
    assert_eq!((p.sub_sport.as_deref(), p.title.as_deref()), (None, Some("Radfahren 14.11.2023 23:13")));
}

#[test]
fn file_values_win_and_unknown_class() {
    let mut s = session("rowing", vec![raw(pt(0, 0.0)), raw(pt(10, 20.0))]);
    s.file =
        import::FileValues { start_ms: Some(1), elapsed_ms: Some(5000), distance_m: Some(42.0), ascent_m: Some(7.0), avg_hr: Some(99) };
    let p = process::prepare(&s, &cfg(), &trackcraft::sports::SportMapping::defaults(), tz());
    assert_eq!(p.sport_class, "unknown");
    assert_eq!(p.start, Some((1, StartOrigin::File)));
    assert_eq!(p.total_time, Some((5000, TimeOrigin::File)));
    assert_eq!(p.distance, Some((42.0, ValueOrigin::File)));
    assert_eq!(p.avg_hr, Some((99, ValueOrigin::File)));
    assert_eq!(p.ascent_file_m, Some(7.0));
    assert_eq!(p.max_speed_threshold_kmh, Some(100.0), "default_max_speed_kmh");
    assert!(p.title.unwrap().starts_with("Aktivität "));
}

#[test]
fn session_without_gps_and_time_needs_input() {
    let s = session("cycling", vec![raw(Point { hr: Some(120), ..Default::default() })]);
    let p = process::prepare(&s, &cfg(), &trackcraft::sports::SportMapping::defaults(), tz());
    assert!(p.needs_time_input);
    assert_eq!(p.title, None);
    assert_eq!(p.distance, None, "fehlend bleibt unbekannt, nie 0");
}

#[test]
fn point_limit_locks_session() {
    let pts: Vec<RawPoint> = (0..100_001).map(|i| raw(pt(i, i as f64))).collect();
    let p = process::prepare(&session("cycling", pts), &cfg(), &trackcraft::sports::SportMapping::defaults(), tz());
    assert!(matches!(p.lock, Some(Lock::PointLimit { count: 100_001, max: 100_000 })));
}

#[test]
fn vehicle_peak_with_gps_speeds_locks_running() {
    // Laufen 3 m/s, dann 60 s mit 20 m/s (72 km/h), dann wieder Laufen
    let mut d = 0.0;
    let pts: Vec<RawPoint> = (0..400)
        .map(|i| {
            d += if (100..160).contains(&i) { 20.0 } else { 3.0 };
            raw(pt(i, d))
        })
        .collect();
    let p = process::prepare(&session("running-trail", pts), &cfg(), &trackcraft::sports::SportMapping::defaults(), tz());
    let Some(Lock::Vehicle(f)) = &p.lock else { panic!("erwartet Fahrzeug: {:?}", p.lock) };
    assert_eq!(f.criterion, VehicleCriterion::Peak);
    assert_eq!(f.limit_kmh, 35.0, "running-trail erbt von running");
    assert!(f.max_kmh > 70.0 && f.to_ms - f.from_ms >= 30_000);
    let msg = process::vehicle_message(f, p.sport_class, tz(), 300);
    assert!(msg.contains("über der Grenze für Laufen (35 km/h)"), "{msg}");
}

#[test]
fn short_gps_jump_is_no_vehicle() {
    let mut d = 0.0;
    let pts: Vec<RawPoint> = (0..400)
        .map(|i| {
            d += if (100..110).contains(&i) { 20.0 } else { 3.0 };
            raw(pt(i, d))
        })
        .collect();
    let p = process::prepare(&session("running", pts), &cfg(), &trackcraft::sports::SportMapping::defaults(), tz());
    assert!(p.lock.is_none(), "{:?}", p.lock);
}

#[test]
fn vehicle_average_with_device_distance() {
    // Rad: Gerätegeschwindigkeit 20 km/h, Distanzzähler aber 400 s lang 25 m/s (90 km/h Schnitt, Spitze bleibt unauffällig)
    let mut dist = 0.0;
    let pts: Vec<RawPoint> = (0..1000)
        .map(|i| {
            dist += if (200..600).contains(&i) { 25.0 } else { 5.5 };
            RawPoint { point: Point { pos: None, ..pt(i, 0.0) }, speed_mps: Some(5.5), distance_m: Some(dist) }
        })
        .collect();
    let p = process::prepare(&session("cycling", pts), &cfg(), &trackcraft::sports::SportMapping::defaults(), tz());
    let Some(Lock::Vehicle(f)) = &p.lock else { panic!("erwartet Fahrzeug: {:?}", p.lock) };
    assert_eq!(f.criterion, VehicleCriterion::Average);
    assert_eq!(f.limit_kmh, 70.0);
}

#[test]
fn window_with_long_pause_does_not_count() {
    // 25 m/s Distanzsprung über eine Pause von 2 min: Fenster mit Pause > 60 s zählen nicht
    let mut pts = Vec::new();
    let mut dist = 0.0;
    for i in 0..1000i64 {
        let t = if i >= 500 { i + 120 } else { i };
        dist += if i == 500 { 8000.0 } else { 5.0 };
        pts.push(RawPoint { point: Point { pos: None, ..pt(t, 0.0) }, speed_mps: Some(5.0), distance_m: Some(dist) });
    }
    let p = process::prepare(&session("cycling", pts), &cfg(), &trackcraft::sports::SportMapping::defaults(), tz());
    assert!(p.lock.is_none(), "{:?}", p.lock);
}

#[test]
fn filter_can_be_disabled() {
    let mut c = cfg();
    c.processing.outlier_filter.enabled = false;
    let mut pts: Vec<RawPoint> = (0..10).map(|i| raw(pt(i, i as f64 * 5.0))).collect();
    pts[5] = raw(pt(5, 900.0));
    let p = process::prepare(&session("cycling", pts), &c, &trackcraft::sports::SportMapping::defaults(), tz());
    assert_eq!((p.filtered_outliers_count, p.max_speed_threshold_kmh), (0, None));
}
