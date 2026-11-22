//! Golden-Segmente (RFC-0014 „Golden-Segmente“): echte Fahrten gegen zehn feste Segmente.
//!
//! - Geometrien mit Koordinaten nur im privaten Korpus unter `golden/segments/<id>.gpx`
//! - Erwartungen ohne Koordinaten in `tests/corpus/golden.toml`
//! - Prüfen: `TRACKCRAFT_TEST_CORPUS=<ordner> cargo test --release --test golden_tests -- --nocapture`
//!
//! Ausgewertet werden die Geräteaufzeichnungen (`edge/`, `uhr/`), aus denen die Erwartungen stammen; Exporte
//! derselben Fahrten aus Strava und SportTracks würden dieselben Versuche ein zweites Mal liefern.
//! Ohne `TRACKCRAFT_TEST_CORPUS` wird der Test übersprungen und meldet das.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use trackcraft::config;
use trackcraft::domain::track::Point;
use trackcraft::import::{self, process};
use trackcraft::segments::matching::{self, Attempt, Reference};

#[derive(Debug, Deserialize)]
struct Golden {
    golden: Header,
    segment: Vec<Segment>,
}

#[derive(Debug, Deserialize)]
struct Header {
    duration_tolerance_ms: i64,
}

#[derive(Debug, Deserialize)]
struct Segment {
    id: String,
    name: String,
    sport_class: String,
    expected_valid: usize,
    #[serde(default)]
    attempt: Vec<Expected>,
}

#[derive(Debug, Deserialize)]
struct Expected {
    file: String,
    result: String,
    duration_ms: Option<i64>,
}

fn cfg() -> config::Config {
    let name = if cfg!(windows) { "trackcraft.example.toml" } else { "trackcraft.container.example.toml" };
    config::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("config").join(name)).unwrap().raw
}

fn golden() -> Golden {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("corpus").join("golden.toml");
    toml::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// Positionen (Breite, Länge) aus einer Golden-GPX ohne Zeitangaben.
fn gpx_points(path: &Path) -> Vec<(f64, f64)> {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let attr = |tag: &str, name: &str| -> f64 {
        let key = format!("{name}=\"");
        let start = tag.find(&key).unwrap() + key.len();
        let end = start + tag[start..].find('"').unwrap();
        tag[start..end].parse().unwrap()
    };
    text.split("<trkpt").skip(1).map(|t| (attr(t, "lat"), attr(t, "lon"))).collect()
}

/// Gefilterte Abschnitte je Sportklasse für alle Geräteaufzeichnungen des Korpus.
fn activities(root: &Path, cfg: &config::Config) -> Vec<(String, &'static str, Vec<Vec<Point>>)> {
    let mut out = vec![];
    for source in ["edge", "uhr"] {
        let Ok(entries) = std::fs::read_dir(root.join(source)) else { continue };
        let mut names: Vec<String> = entries.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().to_string()).collect();
        names.sort();
        for name in names {
            let rel = format!("{source}/{name}");
            let Ok(bytes) = std::fs::read(root.join(source).join(&name)) else { continue };
            let Ok(parsed) = import::parse(&bytes) else { continue };
            for s in &parsed.sessions {
                let p = process::prepare(s, cfg, &trackcraft::sports::SportMapping::defaults(), chrono_tz::Europe::Berlin);
                if p.lock.is_none() {
                    out.push((rel.clone(), p.sport_class, p.sections));
                }
            }
        }
    }
    out
}

#[test]
fn golden_segments_match_expectations() {
    let Some(root) = std::env::var_os("TRACKCRAFT_TEST_CORPUS").map(PathBuf::from) else {
        eprintln!("TRACKCRAFT_TEST_CORPUS nicht gesetzt: Golden-Segmente übersprungen");
        return;
    };
    let cfg = cfg();
    let params = &with_overrides(cfg.segments.matching.clone());
    let golden = golden();
    let tol = golden.golden.duration_tolerance_ms;
    let all = activities(&root, &cfg);
    let mut problems: Vec<String> = vec![];

    for seg in &golden.segment {
        let geometry = gpx_points(&root.join("golden").join("segments").join(format!("{}.gpx", seg.id)));
        let reference = Reference::new(&geometry, params).unwrap();
        // Ergebnisse je Datei
        let mut found: BTreeMap<&str, Vec<Attempt>> = BTreeMap::new();
        for (file, class, sections) in &all {
            if *class != seg.sport_class {
                continue;
            }
            for section in sections {
                let attempts = matching::match_section(&reference, section, params);
                if !attempts.is_empty() {
                    found.entry(file.as_str()).or_default().extend(attempts);
                }
            }
        }
        let mut expected: BTreeMap<&str, Vec<&Expected>> = BTreeMap::new();
        for e in &seg.attempt {
            expected.entry(e.file.as_str()).or_default().push(e);
        }

        let mut valid = 0;
        let mut worst_ms = 0;
        let files: std::collections::BTreeSet<&str> = found.keys().chain(expected.keys()).copied().collect();
        for file in files {
            let got = found.get(file).map(Vec::as_slice).unwrap_or_default();
            let mut got_valid: Vec<i64> = got.iter().filter(|a| a.is_valid()).filter_map(|a| a.duration_ms).collect();
            let exp = expected.get(file).cloned().unwrap_or_default();
            let mut exp_valid: Vec<i64> = exp.iter().filter(|e| e.result == "valid").filter_map(|e| e.duration_ms).collect();
            got_valid.sort();
            exp_valid.sort();
            valid += got_valid.len();
            for nr in got.iter().filter(|a| !a.is_valid()) {
                problems.push(format!(
                    "{} {}: nicht gewertet ({:?}), erwartet kein solcher Versuch",
                    seg.id,
                    file,
                    nr.not_ranked.map(|r| r.code())
                ));
            }
            if got_valid.len() != exp_valid.len() {
                problems.push(format!(
                    "{} {}: {} gewertet, erwartet {} ({:?} / {:?})",
                    seg.id,
                    file,
                    got_valid.len(),
                    exp_valid.len(),
                    got_valid,
                    exp_valid
                ));
                continue;
            }
            for (g, e) in got_valid.iter().zip(&exp_valid) {
                let diff = (g - e).abs();
                worst_ms = worst_ms.max(diff);
                if diff > tol {
                    problems.push(format!("{} {}: Dauer {g} ms, erwartet {e} ms (Abweichung {diff} ms)", seg.id, file));
                }
            }
        }
        eprintln!(
            "{:3} {:24} {:>8.0} m  gewertet {:3} (erwartet {:3})  größte Abweichung {:5} ms",
            seg.id,
            seg.name,
            reference.length_m(),
            valid,
            seg.expected_valid,
            worst_ms
        );
        if valid != seg.expected_valid {
            problems.push(format!("{}: {} gewertet, erwartet {}", seg.id, valid, seg.expected_valid));
        }
    }

    for p in &problems {
        eprintln!("  {p}");
    }
    assert!(problems.is_empty(), "{} Abweichungen von golden.toml", problems.len());
}

/// Fehlersuche: `TRACKCRAFT_GOLDEN_DEBUG=R3:edge/2024-01-28-12-05-29.fit` zeigt jeden Kandidaten mit Grund.
///
/// Beliebige Referenz statt Golden-ID: `TRACKCRAFT_GOLDEN_REF=<pfad.gpx>` und `TRACKCRAFT_GOLDEN_DEBUG=-:edge/a.fit,edge/b.fit`;
/// `TRACKCRAFT_GOLDEN_ROUTE=1` wertet die Referenz als Strecke (Aufzeichnungsrand zählt als Passage).
#[test]
fn golden_debug() {
    let (Some(root), Ok(what)) = (std::env::var_os("TRACKCRAFT_TEST_CORPUS").map(PathBuf::from), std::env::var("TRACKCRAFT_GOLDEN_DEBUG"))
    else {
        return;
    };
    let (id, files) = what.split_once(':').expect("ID:datei");
    let cfg = cfg();
    let route = std::env::var("TRACKCRAFT_GOLDEN_ROUTE").is_ok_and(|v| v == "1");
    let params = with_overrides(cfg.segments.params_for(if route { "route" } else { "segment" }));
    let ref_path = std::env::var_os("TRACKCRAFT_GOLDEN_REF")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("golden").join("segments").join(format!("{id}.gpx")));
    let geometry = gpx_points(&ref_path);
    let reference = Reference::new(&geometry, &params).unwrap().with_open_ends(route);
    for file in files.split(',') {
        eprintln!("== {file}");
        let started = std::time::Instant::now();
        golden_debug_file(&root, file, &cfg, &params, &reference, &geometry);
        eprintln!("   {} ms", started.elapsed().as_millis());
    }
}

/// Parameter versuchsweise ändern: `TRACKCRAFT_GOLDEN_SET=tolerance_m=20,max_inner_gap_m=104`.
fn with_overrides(mut p: config::MatchingSection) -> config::MatchingSection {
    let Ok(set) = std::env::var("TRACKCRAFT_GOLDEN_SET") else { return p };
    for kv in set.split(',').filter(|s| !s.is_empty()) {
        let (k, v) = kv.split_once('=').expect("name=wert");
        let v: f64 = v.parse().expect("Zahl");
        match k {
            "tolerance_m" => p.tolerance_m = v,
            "max_inner_gap_m" => p.max_inner_gap_m = v,
            "matching_radius_meters" => p.matching_radius_meters = v,
            "max_offset_m" => p.max_offset_m = v,
            "min_coverage_ratio" => p.min_coverage_ratio = v,
            _ => panic!("unbekannt: {k}"),
        }
    }
    p
}

/// Durchsucht alle Radfahrten des Korpus nach Versuchen auf beliebigen Referenzen (Fehlersuche, Parameterwahl):
/// `TRACKCRAFT_GOLDEN_SCAN=<a.gpx>,<b.gpx>`, optional `TRACKCRAFT_GOLDEN_SET=…` und `TRACKCRAFT_GOLDEN_ROUTE=1`.
#[test]
fn golden_scan() {
    let (Some(root), Ok(refs)) = (std::env::var_os("TRACKCRAFT_TEST_CORPUS").map(PathBuf::from), std::env::var("TRACKCRAFT_GOLDEN_SCAN"))
    else {
        return;
    };
    let cfg = cfg();
    let route = std::env::var("TRACKCRAFT_GOLDEN_ROUTE").is_ok_and(|v| v == "1");
    let params = with_overrides(cfg.segments.params_for(if route { "route" } else { "segment" }));
    let all = activities(&root, &cfg);
    for r in refs.split(',') {
        let reference = Reference::new(&gpx_points(Path::new(r)), &params).unwrap().with_open_ends(route);
        let mut valid = vec![];
        for (file, class, sections) in &all {
            if *class != "cycling" {
                continue;
            }
            for section in sections {
                for a in matching::match_section(&reference, section, &params).iter().filter(|a| a.is_valid()) {
                    valid.push(format!("{file} {} ms", a.duration_ms.unwrap_or(0)));
                }
            }
        }
        eprintln!("{r}: {} gewertet", valid.len());
        for v in valid {
            eprintln!("  {v}");
        }
    }
}

fn golden_debug_file(
    root: &Path,
    file: &str,
    cfg: &config::Config,
    params: &config::MatchingSection,
    reference: &Reference,
    geometry: &[(f64, f64)],
) {
    let parsed = import::parse(&std::fs::read(root.join(file)).unwrap()).unwrap();
    for s in &parsed.sessions {
        let p = process::prepare(s, cfg, &trackcraft::sports::SportMapping::defaults(), chrono_tz::Europe::Berlin);
        for (k, section) in p.sections.iter().enumerate() {
            // Nur lokal: TRACKCRAFT_GOLDEN_DUMP=<datei.json> schreibt Referenz und Track für eine Kartenansicht
            if let Some(out) = std::env::var_os("TRACKCRAFT_GOLDEN_DUMP") {
                let deg = |v: i32| v as f64 / 1e7;
                let track: Vec<serde_json::Value> = section
                    .iter()
                    .enumerate()
                    .filter_map(|(i, pt)| pt.pos.map(|(la, lo)| serde_json::json!([deg(la), deg(lo), pt.t_ms.map(|t| t / 1000), i])))
                    .collect();
                std::fs::write(out, serde_json::json!({ "reference": geometry, "track": track }).to_string()).unwrap();
            }
            let (_, log) = matching::diagnose(reference, section, params);
            for line in log {
                eprintln!("Session {} Abschnitt {k}: {line}", s.session_index);
            }
        }
    }
}
