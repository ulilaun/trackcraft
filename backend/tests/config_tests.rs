//! Startprüfungen der Konfiguration (RFC-0021 Abschnitt 3, RFC-0014 Fall 30).

mod common;

use std::path::Path;

use trackcraft::config::{self, ConfigError, parse_origin, sport_class_for};

fn problems(text: &str) -> Vec<String> {
    match config::parse(Path::new("test.toml"), text) {
        Err(ConfigError::Invalid { problems, .. }) => problems,
        Err(e) => vec![e.to_string()],
        Ok(_) => vec![],
    }
}

#[test]
fn minimal_config_is_valid_and_defaults_apply() {
    let dir = common::temp();
    let cfg = common::validated(dir.path());
    assert_eq!(cfg.raw.app.timezone, "Europe/Berlin");
    assert_eq!(cfg.raw.backup.daily_time, "03:00");
    assert_eq!(cfg.raw.segments.matching.tolerance_m, 20.0);
    assert_eq!(cfg.raw.segments.matching.max_inner_gap_m, 30.0);
    assert_eq!(cfg.raw.import.max_points_per_session, 100_000);
    assert_eq!(cfg.raw.maps.track_max_points, 2000);
    assert_eq!(cfg.origins.len(), 2);
    assert_eq!(cfg.origins[1].port, 8443);
    assert!(cfg.staging_dir().ends_with("staging"));
}

#[test]
fn unknown_key_is_rejected() {
    let dir = common::temp();
    let text = common::config_text(dir.path(), "", "[logging]\nlevel = \"info\"\nfarbe = \"blau\"");
    let err = config::parse(Path::new("t.toml"), &text).unwrap_err();
    assert!(matches!(err, ConfigError::Parse { .. }), "{err}");
}

#[test]
fn missing_required_value_is_rejected() {
    let err = config::parse(Path::new("t.toml"), "[server]\nallowed_origins = []\n").unwrap_err();
    assert!(matches!(err, ConfigError::Parse { .. }));
}

#[test]
fn static_dir_requires_tls() {
    let dir = common::temp();
    let web = common::lit(&dir.path().join("web"));
    let p = problems(&common::config_text(dir.path(), &format!("static_dir = {web}"), ""));
    assert!(p.iter().any(|m| m.contains("[tls] Pflicht")), "{p:?}");
}

#[test]
fn invalid_values_are_all_reported() {
    let dir = common::temp();
    let extra = r#"
[app]
timezone = "Europe/Bielefeld"
[logging]
level = "laut"
[segments.matching]
min_coverage_ratio = 1.5
max_vector_angle_deg = 120.0
[processing.vehicle_detection]
min_duration_s = 300
window_s = 300
[maps]
track_max_points = 1
"#;
    let text = common::config_text(dir.path(), "", extra).replace("[user]\ndisplay_name = \"Testperson\"", "[user]\ndisplay_name = \"  \"");
    let p = problems(&text);
    for needle in ["IANA", "display_name", "level", "min_coverage_ratio", "max_vector_angle_deg", "window_s", "track_max_points"] {
        assert!(p.iter().any(|m| m.contains(needle)), "fehlt {needle}: {p:?}");
    }
}

#[test]
fn target_dir_must_differ_from_data_dirs() {
    let dir = common::temp();
    let text = common::config_text(dir.path(), "", "").replace(
        &format!("target_dir    = {}", common::lit(&dir.path().join("ziel"))),
        &format!("target_dir    = {}", common::lit(&dir.path().join("data/backups"))),
    );
    let p = problems(&text);
    assert!(p.iter().any(|m| m.contains("target_dir muss sich")), "{p:?}");
}

#[test]
fn relative_paths_are_rejected() {
    let dir = common::temp();
    let text = common::config_text(dir.path(), "", "").replace(&common::lit(&dir.path().join("data/logs")), "'logs'");
    let p = problems(&text);
    assert!(p.iter().any(|m| m.contains("logs_dir") && m.contains("absolut")), "{p:?}");
}

/// Die alten Sportart-Abschnitte sind seit Schemaversion 7 veraltet: Die Konfiguration nimmt sie ohne Prüfung an,
/// sports::ensure übernimmt sie einmalig und überspringt ungültige Einträge.
#[test]
fn legacy_sport_sections_are_accepted_unchecked() {
    let dir = common::temp();
    let text = common::config_text(
        dir.path(),
        "",
        "[import.sport_class_mapping]
swimming = \"Becken\"
[import.sub_sport_mapping]
cycling-road = \"Trail\"
",
    );
    let p = problems(&text);
    assert!(p.is_empty(), "{p:?}");
}

#[test]
fn sport_class_inherits_from_sport() {
    let mut m = std::collections::BTreeMap::new();
    m.insert("cycling".to_string(), "cycling".to_string());
    assert_eq!(sport_class_for(&m, "cycling-road"), Some("cycling"));
    assert_eq!(sport_class_for(&m, "cycling"), Some("cycling"));
    assert_eq!(sport_class_for(&m, "running-trail"), None);
}

#[test]
fn origins_follow_the_rules() {
    assert_eq!(parse_origin("https://trackcraft.home.arpa").unwrap().port, 443);
    assert_eq!(parse_origin("https://pc:8443").unwrap().host, "pc");
    for bad in ["http://pc", "https://PC", "https://pc/", "https://pc/app", "https://pc?x=1", "https://pc:0", "https://pc:abc", "https://"]
    {
        assert!(parse_origin(bad).is_err(), "{bad} sollte ungültig sein");
    }
}

#[test]
fn duplicate_origin_is_rejected() {
    let dir = common::temp();
    let text = common::config_text(dir.path(), "", "")
        .replace(r#"["https://trackcraft.test", "https://pc.home.arpa:8443"]"#, r#"["https://a.test", "https://a.test"]"#);
    let p = problems(&text);
    assert!(p.iter().any(|m| m.contains("mehrfach")), "{p:?}");
}

#[test]
fn daily_time_format() {
    let dir = common::temp();
    for (value, ok) in [("03:00", true), ("23:59", true), ("24:00", false), ("3:00", false), ("03:60", false), ("ab:cd", false)] {
        let text = common::config_text(dir.path(), "", "").replace(
            &format!("migration_dir = {}", common::lit(&dir.path().join("data/backups"))),
            &format!("migration_dir = {}\ndaily_time = \"{value}\"", common::lit(&dir.path().join("data/backups"))),
        );
        assert_eq!(problems(&text).is_empty(), ok, "{value}");
    }
}

/// Die ausgelieferten Beispieldateien müssen gültig sein (Windows-Datei unter Windows, Container-Datei unter Linux).
#[test]
fn shipped_example_is_valid() {
    let name = if cfg!(windows) { "trackcraft.example.toml" } else { "trackcraft.container.example.toml" };
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("config").join(name);
    let cfg = config::load(&path).unwrap_or_else(|e| panic!("{e}"));
    assert!(cfg.raw.import.sport_class_mapping.is_empty() && cfg.raw.import.sub_sport_mapping.is_empty(), "Abschnitte veraltet");

    // Weitere Sportklassen: FIT-Sport → Klasse und Untertyp, jetzt aus den eingebauten Vorgaben.
    let tz = chrono_tz::Europe::Berlin;
    for (source, class, sub) in [
        ("walking", "running", "walking"),
        ("hiking", "running", "walking"),
        ("cross_country_skiing", "skiing", "nordic"),
        ("swimming-lap_swimming", "swimming", "indoor"),
        ("fitness_equipment", "studio", "strength"),
        ("training-strength_training", "studio", "strength"),
    ] {
        let s = trackcraft::import::ParsedSession { source_sport: Some(source.into()), ..Default::default() };
        let p = trackcraft::import::process::prepare(&s, &cfg.raw, &trackcraft::sports::SportMapping::defaults(), tz);
        assert_eq!((p.sport_class, p.sub_sport.as_deref()), (class, Some(sub)), "{source}");
    }
}
