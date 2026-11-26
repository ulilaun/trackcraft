#![allow(dead_code)]

pub mod contract;

use std::path::{Path, PathBuf};

use trackcraft::config::{self, Validated};

/// TOML-Literalstring für einen Pfad (Windows-Backslashes ohne Escape).
pub fn lit(p: &Path) -> String {
    format!("'{}'", p.display())
}

/// Minimale gültige Konfiguration mit allen Datenordnern unter `root`.
pub fn config_text(root: &Path, extra_server: &str, extra: &str) -> String {
    format!(
        r#"
[user]
display_name = "Testperson"

[server]
allowed_origins = ["https://trackcraft.test", "https://pc.home.arpa:8443"]
listen = "127.0.0.1:0"
{extra_server}

[paths]
database_dir   = {db}
activities_dir = {act}
logs_dir       = {logs}

[backup]
target_dir    = {target}
migration_dir = {mig}

{extra}
"#,
        db = lit(&root.join("data/database")),
        act = lit(&root.join("data/activities")),
        logs = lit(&root.join("data/logs")),
        target = lit(&root.join("ziel")),
        mig = lit(&root.join("data/backups")),
    )
}

pub fn validated(root: &Path) -> Validated {
    let text = config_text(root, "", "");
    config::parse(&root.join("trackcraft.toml"), &text).expect("Testkonfiguration gültig")
}

pub fn temp() -> tempfile::TempDir {
    tempfile::tempdir().expect("Temp-Ordner")
}

pub fn path(root: &Path, rel: &str) -> PathBuf {
    root.join(rel)
}
