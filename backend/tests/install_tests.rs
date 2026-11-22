//! Erzeugung der ersten Konfiguration durch `setup` (RFC-0005).

use std::path::Path;

use trackcraft::config;
use trackcraft::install::{self, Answers};

fn template() -> String {
    let name = if cfg!(windows) { "trackcraft.example.toml" } else { "trackcraft.container.example.toml" };
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("config").join(name)).unwrap()
}

fn answers(root: &Path, port: u16) -> Answers {
    Answers {
        display_name: "  Uli Lä  ".into(),
        names: vec!["pc.home.arpa".into(), "localhost".into()],
        port,
        static_dir: root.join("Program Files").join("TrackCraft").join("web"),
        cert_file: root.join("config").join("tls").join("server.pem"),
        key_file: root.join("config").join("tls").join("server-key.pem"),
        backup_dir: root.join("sicherung"),
    }
}

#[test]
fn rendered_config_passes_startup_checks() {
    let dir = tempfile::tempdir().unwrap();
    let text = install::render(&template(), &answers(dir.path(), 443)).unwrap();
    let cfg = config::parse(&dir.path().join("t.toml"), &text).unwrap_or_else(|e| panic!("{e}\n{text}"));
    assert_eq!(cfg.raw.user.as_ref().unwrap().display_name, "Uli Lä");
    let origins: Vec<&str> = cfg.origins.iter().map(|o| o.origin.as_str()).collect();
    assert_eq!(origins, ["https://pc.home.arpa", "https://localhost"]);
    assert_eq!(cfg.listen.port(), 443);
    assert_eq!(cfg.raw.backup.target_dir, dir.path().join("sicherung"));
    assert_eq!(install::hosts(&cfg.origins), ["pc.home.arpa", "localhost"]);
    // Vorlage bleibt erkennbar: Kommentare und übrige Werte unverändert
    assert!(text.contains("# nur lokale Migrationssicherung"));
    assert!(text.contains("[segments.matching]"));
}

#[test]
fn non_default_port_appears_in_origins() {
    let dir = tempfile::tempdir().unwrap();
    let text = install::render(&template(), &answers(dir.path(), 8443)).unwrap();
    let cfg = config::parse(&dir.path().join("t.toml"), &text).unwrap();
    assert_eq!(cfg.origins[0].origin, "https://pc.home.arpa:8443");
    assert_eq!(cfg.listen.port(), 8443);
}

#[test]
fn set_origins_changes_only_allowed_origins() {
    let dir = tempfile::tempdir().unwrap();
    let before = install::render(&template(), &answers(dir.path(), 443)).unwrap();
    let after = install::set_origins(&before, &["localhost".into(), "trackcraft.home.arpa".into()], 443).unwrap();
    let cfg = config::parse(&dir.path().join("t.toml"), &after).unwrap();
    let origins: Vec<&str> = cfg.origins.iter().map(|o| o.origin.as_str()).collect();
    assert_eq!(origins, ["https://localhost", "https://trackcraft.home.arpa"]);
    // Alle übrigen Zeilen bleiben gleich.
    let diff: Vec<_> = before.lines().zip(after.lines()).filter(|(a, b)| a != b).collect();
    assert_eq!(diff.len(), 1, "{diff:?}");
    assert!(diff[0].0.starts_with("allowed_origins"));
}

#[test]
fn placeholder_marks_unconfigured_config() {
    // Vorlage (auch ältere Fassungen) gilt als nicht eingerichtet, eine erzeugte Konfiguration nicht.
    assert!(install::is_unconfigured(&template()));
    assert!(install::is_unconfigured("[user]\ndisplay_name = \"Beispielname\"\n[tls]\ncert_file = 'C:\\alt.pem'\n"));
    let dir = tempfile::tempdir().unwrap();
    let text = install::render(&template(), &answers(dir.path(), 443)).unwrap();
    assert!(!install::is_unconfigured(&text));
    assert!(!install::is_unconfigured("kein toml ["));
}

/// Windows-Dienstpfade; die Funktion wird nur unter Windows verwendet.
#[cfg(windows)]
#[test]
fn service_path_is_parsed_with_and_without_quotes() {
    let dir = |s: &str| install::exe_dir_from_service_path(s).map(|p| p.to_string_lossy().replace('/', "\\"));
    let pf = r"C:\Program Files\TrackCraft";
    assert_eq!(dir(r#""C:\Program Files\TrackCraft\trackcraft.exe" service-run"#).as_deref(), Some(pf));
    assert_eq!(dir(r#""C:\Program Files\TrackCraft\trackcraft.exe""#).as_deref(), Some(pf));
    assert_eq!(dir(r"C:\Program Files\TrackCraft\trackcraft.exe service-run").as_deref(), Some(pf));
    assert_eq!(dir(r"D:\TC\trackcraft.exe").as_deref(), Some(r"D:\TC"));
    assert_eq!(dir("trackcraft.exe"), None);
    assert_eq!(dir(r#"""#), None);
}

#[test]
fn origins_helper() {
    assert_eq!(install::origins(&[" PC.Home.Arpa ".into()], 443), ["https://pc.home.arpa"]);
    assert_eq!(install::origins(&["pc".into()], 8443), ["https://pc:8443"]);
}

/// Schlüssel einer TOML als Pfade `tabelle.schlüssel`, ohne Werte.
fn keys(text: &str) -> Vec<String> {
    fn walk(prefix: &str, t: &toml::Table, out: &mut Vec<String>) {
        for (k, v) in t {
            let p = if prefix.is_empty() { k.clone() } else { format!("{prefix}.{k}") };
            match v {
                toml::Value::Table(inner) => walk(&p, inner, out),
                _ => out.push(p),
            }
        }
    }
    let mut out = vec![];
    walk("", &text.parse::<toml::Table>().unwrap(), &mut out);
    out.sort();
    out
}

#[test]
fn linux_template_has_the_same_keys_as_windows() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("config");
    let read = |n: &str| std::fs::read_to_string(dir.join(n)).unwrap();
    assert_eq!(keys(&read("trackcraft.linux.example.toml")), keys(&read("trackcraft.example.toml")));
    assert!(install::is_unconfigured(&read("trackcraft.linux.example.toml")));
}

#[cfg(unix)]
#[test]
fn linux_template_renders_valid_config() {
    let dir = tempfile::tempdir().unwrap();
    let template =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("config").join("trackcraft.linux.example.toml")).unwrap();
    let text = install::render(&template, &answers(dir.path(), 443)).unwrap();
    let cfg = config::parse(&dir.path().join("t.toml"), &text).unwrap_or_else(|e| panic!("{e}\n{text}"));
    assert_eq!(cfg.raw.paths.database_dir, Path::new("/var/lib/trackcraft/database"));
    assert_eq!(cfg.raw.server.static_dir.as_deref(), Some(dir.path().join("Program Files").join("TrackCraft").join("web").as_path()));
}
