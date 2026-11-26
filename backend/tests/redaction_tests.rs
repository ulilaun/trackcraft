//! Geheimnisse erscheinen nicht im `Debug`-Format (ADR-0013, #3).

use std::path::Path;

use trackcraft::backup::format::StandInfo;
use trackcraft::backup::restore::{Comparison, Current, Preview};
use trackcraft::garmin::store::{Change, Settings};
use trackcraft::import_job::ImportDecision;
use trackcraft::logging::REDACTED;

const SECRET: &str = "geheim-1234567890";

fn assert_hidden(debug: String) {
    assert!(!debug.contains(SECRET), "Geheimnis im Debug-Format: {debug}");
    assert!(debug.contains(REDACTED), "Platzhalter fehlt: {debug}");
}

#[test]
fn garmin_settings_hide_api_key() {
    let settings = Settings { api_key: Some(SECRET.into()), enabled: true, ..Default::default() };
    let debug = format!("{settings:?}");
    assert_hidden(debug.clone());
    assert!(debug.contains("enabled: true"), "andere Felder bleiben sichtbar: {debug}");
    assert!(format!("{:?}", Settings::default()).contains("api_key: None"));
}

#[test]
fn garmin_change_hides_api_key() {
    let change = Change { api_key: Some(Some(SECRET.into())), ..Default::default() };
    assert_hidden(format!("{change:?}"));
    assert_hidden(format!("{change:#?}"));
    let remove = Change { api_key: Some(None), ..Default::default() };
    assert!(format!("{remove:?}").contains("api_key: Some(None)"));
}

#[test]
fn restore_preview_hides_token() {
    let preview = Preview {
        token: SECRET.into(),
        stand: StandInfo { name: "trackcraft-20261126-120000-s23-c1".into(), manifest: None, problem: None },
        current: Current { revision_id: None, activity_count: 0, segment_count: 0, is_empty: true },
        comparison: Comparison::NotComparable,
        requires_acknowledgement: false,
        warnings: vec![],
    };
    let debug = format!("{preview:?}");
    assert_hidden(debug.clone());
    assert!(debug.contains("trackcraft-20261126-120000-s23-c1"));
}

#[test]
fn import_decision_hides_state_token() {
    let decision: ImportDecision = serde_json::from_value(serde_json::json!({ "state_token": SECRET, "sessions": [] })).unwrap();
    assert_hidden(format!("{decision:?}"));
}

/// Neue Strukturen mit Schlüssel- oder Tokenfeld dürfen `Debug` nicht einfach ableiten.
#[test]
fn no_derived_debug_on_secret_fields() {
    let mut found = vec![];
    scan(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut found);
    assert!(found.is_empty(), "derive(Debug) auf Strukturen mit Geheimnis (eigenes Debug mit logging::Hidden schreiben): {found:#?}");
}

#[test]
fn scan_finds_derived_debug_with_secret() {
    let sample = "#[derive(Debug, Clone)]\n#[serde(deny_unknown_fields)]\npub struct Foo {\n    pub id: i64,\n    pub state_token: String,\n}\n\
                  #[derive(Debug)]\nstruct Bar {\n    tokens_used: u32,\n}\n";
    let mut found = vec![];
    check_file(Path::new("beispiel.rs"), sample, &mut found);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("state_token"));
}

const SECRET_FIELDS: [&str; 5] = ["api_key", "token", "password", "secret", "cookie"];

fn scan(dir: &Path, found: &mut Vec<String>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            scan(&path, found);
        } else if path.extension().is_some_and(|e| e == "rs") {
            check_file(&path, &std::fs::read_to_string(&path).unwrap(), found);
        }
    }
}

fn check_file(path: &Path, text: &str, found: &mut Vec<String>) {
    let lines: Vec<&str> = text.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        let t = line.trim();
        if !(t.starts_with("#[derive(") && t.contains("Debug")) {
            continue;
        }
        // Weitere Attribute und Kommentare bis zur Struktur überspringen.
        let mut j = i + 1;
        while j < lines.len() && (lines[j].trim().starts_with("#[") || lines[j].trim().starts_with("//")) {
            j += 1;
        }
        let Some(head) = lines.get(j).map(|l| l.trim()) else { continue };
        if !(head.contains("struct ") && head.ends_with('{')) {
            continue;
        }
        for field in lines[j + 1..].iter().map(|l| l.trim()).take_while(|l| !l.starts_with('}')) {
            let name = field.trim_start_matches("pub(crate) ").trim_start_matches("pub ").split(':').next().unwrap_or("").trim();
            if SECRET_FIELDS.iter().any(|s| name == *s || name.ends_with(&format!("_{s}"))) {
                found.push(format!("{}:{} {head} → {name}", path.display(), j + 1));
            }
        }
    }
}
