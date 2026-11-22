//! Privater Testkorpus (RFC-0014 „Testkorpus“): echte Dateien nur lokal über `TRACKCRAFT_TEST_CORPUS`, erwartete
//! Kennzahlen ohne Koordinaten in `tests/corpus/manifest.toml`.
//!
//! - Prüfen: `TRACKCRAFT_TEST_CORPUS=<ordner> cargo test --test corpus_tests -- --nocapture`
//! - Manifest neu schreiben (bewusster Schritt, Änderungen im Diff prüfen):
//!   zusätzlich `TRACKCRAFT_UPDATE_MANIFEST=1`
//!
//! Ohne `TRACKCRAFT_TEST_CORPUS` wird der Test übersprungen und meldet das.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use trackcraft::config;
use trackcraft::import::{self, process};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Manifest {
    #[serde(rename = "file")]
    files: Vec<FileEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct FileEntry {
    path: String,
    sha256: String,
    source: String,
    /// `ok` oder der Fehlercode der Abweisung
    result: String,
    #[serde(default, rename = "session", skip_serializing_if = "Vec::is_empty")]
    sessions: Vec<SessionEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct SessionEntry {
    index: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_sport: Option<String>,
    sport_class: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    sub_sport: Option<String>,
    /// Start in UTC, RFC 3339
    #[serde(skip_serializing_if = "Option::is_none")]
    start: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    total_time_ms: Option<i64>,
    /// Distanz in Metern, eine Nachkommastelle
    #[serde(skip_serializing_if = "Option::is_none")]
    distance_m: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    distance_origin: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ascent_file_m: Option<f64>,
    /// Aufstieg über die Höhenhysterese, eine Nachkommastelle
    #[serde(skip_serializing_if = "Option::is_none")]
    ascent_hysteresis_m: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    avg_hr: Option<u16>,
    raw_points: usize,
    points: usize,
    dropped: usize,
    sections: usize,
    /// Sperre: `locked_limit`, `locked_vehicle`
    #[serde(skip_serializing_if = "Option::is_none")]
    lock: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    needs_time_input: Option<bool>,
}

fn manifest_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("corpus").join("manifest.toml")
}

fn cfg() -> config::Config {
    let name = if cfg!(windows) { "trackcraft.example.toml" } else { "trackcraft.container.example.toml" };
    config::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("config").join(name)).unwrap().raw
}

fn one_decimal(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

fn evaluate(rel: &str, bytes: &[u8], cfg: &config::Config) -> FileEntry {
    let sha256 = Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect();
    let source = rel.split('/').next().unwrap_or("").to_string();
    let base = FileEntry { path: rel.to_string(), sha256, source, result: "ok".into(), sessions: vec![] };
    let parsed = match import::parse(bytes) {
        Ok(p) => p,
        Err(e) => return FileEntry { result: e.code().into(), ..base },
    };
    let tz = chrono_tz::Europe::Berlin;
    let sessions = parsed
        .sessions
        .iter()
        .map(|s| {
            let p = process::prepare(s, cfg, &trackcraft::sports::SportMapping::defaults(), tz);
            SessionEntry {
                index: s.session_index,
                source_sport: p.source_sport.clone(),
                sport_class: p.sport_class.into(),
                sub_sport: p.sub_sport.clone(),
                start: p.start.and_then(|(ms, _)| chrono::DateTime::from_timestamp_millis(ms)).map(|d| d.to_rfc3339()),
                total_time_ms: p.total_time.map(|(ms, _)| ms),
                distance_m: p.distance.map(|(d, _)| one_decimal(d)),
                distance_origin: p.distance.map(|(_, o)| format!("{o:?}").to_lowercase()),
                ascent_file_m: p.ascent_file_m,
                ascent_hysteresis_m: p.elevation.map(|e| one_decimal(e.gain_m)),
                avg_hr: p.avg_hr.map(|(h, _)| h),
                raw_points: s.point_count(),
                points: p.point_count(),
                dropped: p.filtered_outliers_count,
                sections: p.sections.len(),
                lock: p.lock.as_ref().map(|l| l.code().to_string()),
                needs_time_input: p.needs_time_input.then_some(true),
            }
        })
        .collect();
    FileEntry { sessions, ..base }
}

fn corpus_files(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    for source in ["edge", "uhr", "strava", "sporttracks", "komoot"] {
        let dir = root.join(source);
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        let mut names: Vec<String> = entries
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_file())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.to_lowercase().ends_with(".fit") || n.to_lowercase().ends_with(".gpx"))
            .collect();
        names.sort();
        out.extend(names.into_iter().map(|n| format!("{source}/{n}")));
    }
    out
}

fn summary(entries: &[FileEntry]) {
    let mut by_class: BTreeMap<String, (usize, usize, usize, usize, usize)> = BTreeMap::new(); // Dateien, Punkte, verworfen, Dateien mit Verworfenen, gesperrt
    for e in entries {
        for s in &e.sessions {
            let v = by_class.entry(format!("{}/{}", e.source, s.sport_class)).or_default();
            v.0 += 1;
            v.1 += s.raw_points;
            v.2 += s.dropped;
            v.3 += usize::from(s.dropped > 0);
            v.4 += usize::from(s.lock.is_some());
        }
    }
    eprintln!("Quelle/Klasse: Sessions, Punkte, verworfen (in Sessions), gesperrt");
    for (k, (n, pts, drop, with, locked)) in by_class {
        eprintln!("  {k:<22} {n:>4} {pts:>9} {drop:>6} ({with}) {locked}");
    }
    for e in entries {
        if e.result != "ok" {
            eprintln!("  abgewiesen: {} ({})", e.path, e.result);
        }
        for s in &e.sessions {
            if let Some(l) = &s.lock {
                eprintln!("  gesperrt: {} Session {} ({l})", e.path, s.index);
            }
        }
    }
}

#[test]
fn private_corpus_matches_manifest() {
    let Some(root) = std::env::var_os("TRACKCRAFT_TEST_CORPUS").map(PathBuf::from) else {
        eprintln!("übersprungen: TRACKCRAFT_TEST_CORPUS ist nicht gesetzt (privater Korpus nur lokal, RFC-0014)");
        return;
    };
    let cfg = cfg();
    let update = std::env::var_os("TRACKCRAFT_UPDATE_MANIFEST").is_some();

    if update {
        let files: Vec<FileEntry> =
            corpus_files(&root).iter().map(|rel| evaluate(rel, &std::fs::read(root.join(rel)).unwrap(), &cfg)).collect();
        assert!(!files.is_empty(), "keine Dateien unter {}", root.display());
        summary(&files);
        let text = format!(
            "# Testkorpus-Manifest (RFC-0014): erwartete Kennzahlen je Datei, ohne Koordinaten.\n\
             # Erzeugt mit TRACKCRAFT_UPDATE_MANIFEST=1, Änderungen nur bewusst und im Diff geprüft.\n\
             # Werte nach RFC-0008 mit der Beispielkonfiguration; Distanz und Hysterese auf 0,1 m.\n\n{}",
            toml::to_string(&Manifest { files }).unwrap()
        );
        std::fs::write(manifest_path(), text).unwrap();
        eprintln!("Manifest geschrieben: {}", manifest_path().display());
        return;
    }

    let manifest: Manifest = toml::from_str(&std::fs::read_to_string(manifest_path()).expect("Manifest fehlt")).unwrap();
    let mut failures = Vec::new();
    let mut checked = Vec::new();
    for expected in &manifest.files {
        let path = root.join(&expected.path);
        let Ok(bytes) = std::fs::read(&path) else {
            failures.push(format!("{}: Datei fehlt im Korpus", expected.path));
            continue;
        };
        let actual = evaluate(&expected.path, &bytes, &cfg);
        if actual != *expected {
            failures.push(format!("{}:\n  erwartet {:?}\n  erhalten {:?}", expected.path, expected, actual));
        }
        checked.push(actual);
    }
    let known: Vec<&str> = manifest.files.iter().map(|f| f.path.as_str()).collect();
    for rel in corpus_files(&root) {
        if !known.contains(&rel.as_str()) {
            failures.push(format!("{rel}: neue Datei, nicht im Manifest"));
        }
    }
    summary(&checked);
    assert!(failures.is_empty(), "{} Abweichungen:\n{}", failures.len(), failures.join("\n"));
}

/// Gleiche Datei zweimal ergibt denselben Hash (Erkennung identischer Dateien, RFC-0008 Abschnitt 3).
#[test]
fn manifest_has_no_coordinates() {
    let Ok(text) = std::fs::read_to_string(manifest_path()) else { return };
    for word in ["lat", "lon", "position"] {
        assert!(!text.contains(word), "Manifest darf keine Koordinaten enthalten ({word})");
    }
}
