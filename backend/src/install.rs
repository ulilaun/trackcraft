//! Erzeugt die erste `trackcraft.toml` aus der Beispieldatei mit den Antworten der Einrichtung (RFC-0005).
//! Kommentare und Reihenfolge der Vorlage bleiben erhalten. Eine vorhandene TOML wird nie verändert.

use std::path::Path;

use anyhow::Context;
use toml_edit::{Array, DocumentMut, value};

use crate::config::Origin;

#[derive(Debug, Clone)]
pub struct Answers {
    pub display_name: String,
    /// Rechnernamen, unter denen TrackCraft im Browser aufgerufen wird.
    pub names: Vec<String>,
    pub port: u16,
    pub static_dir: std::path::PathBuf,
    pub cert_file: std::path::PathBuf,
    pub key_file: std::path::PathBuf,
    pub backup_dir: std::path::PathBuf,
}

/// Programmordner aus dem Pfad, den Windows für einen Dienst speichert. Bei Leerzeichen steht er in
/// Anführungszeichen, ggf. mit Argumenten dahinter: `"C:\Program Files\TrackCraft\trackcraft.exe" service-run`.
pub fn exe_dir_from_service_path(raw: &str) -> Option<std::path::PathBuf> {
    let raw = raw.trim();
    let exe = match raw.strip_prefix('"') {
        Some(rest) => rest.split('"').next()?,
        None => {
            // Ohne Anführungszeichen: alles bis einschließlich ".exe", sonst der ganze Text.
            let lower = raw.to_ascii_lowercase();
            match lower.find(".exe") {
                Some(i) => &raw[..i + 4],
                None => raw,
            }
        }
    };
    let dir = std::path::Path::new(exe).parent()?;
    (!dir.as_os_str().is_empty()).then(|| dir.to_path_buf())
}

/// Platzhalter der Beispieldatei für `[user] display_name`.
pub const PLACEHOLDER_NAME: &str = "Beispielname";

/// Gilt die TOML als noch nicht eingerichtet? Das ist sie, solange der Platzhalter-Anzeigename aus der
/// Beispieldatei drinsteht, egal aus welcher Fassung der Vorlage sie stammt. `setup` darf sie dann ersetzen.
pub fn is_unconfigured(text: &str) -> bool {
    text.parse::<DocumentMut>()
        .ok()
        .and_then(|doc| doc.get("user")?.get("display_name")?.as_str().map(|s| s.trim() == PLACEHOLDER_NAME))
        .unwrap_or(false)
}

/// Herkunft im Browser je Name; der Port erscheint nur, wenn er nicht 443 ist.
pub fn origins(names: &[String], port: u16) -> Vec<String> {
    names
        .iter()
        .map(|n| {
            let n = n.trim().to_ascii_lowercase();
            if port == 443 { format!("https://{n}") } else { format!("https://{n}:{port}") }
        })
        .collect()
}

/// Rechnernamen aus den erlaubten Herkünften (für das Zertifikat).
pub fn hosts(origins: &[Origin]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for o in origins {
        if !out.contains(&o.host) {
            out.push(o.host.clone());
        }
    }
    out
}

/// Ersetzt nur `[server] allowed_origins` in einer vorhandenen TOML; alles andere bleibt unverändert.
pub fn set_origins(text: &str, names: &[String], port: u16) -> anyhow::Result<String> {
    let mut doc: DocumentMut = text.parse().context("Konfiguration ist kein gültiges TOML")?;
    let mut list = Array::new();
    for o in origins(names, port) {
        list.push(o);
    }
    doc["server"]["allowed_origins"] = value(list);
    Ok(doc.to_string())
}

pub fn render(template: &str, a: &Answers) -> anyhow::Result<String> {
    let mut doc: DocumentMut = template.parse().context("Beispieldatei ist kein gültiges TOML")?;
    let path = |p: &Path| p.to_string_lossy().to_string();

    doc["user"]["display_name"] = value(a.display_name.trim());
    let mut list = Array::new();
    for o in origins(&a.names, a.port) {
        list.push(o);
    }
    doc["server"]["allowed_origins"] = value(list);
    doc["server"]["listen"] = value(format!("0.0.0.0:{}", a.port));
    doc["server"]["static_dir"] = value(path(&a.static_dir));
    doc["tls"]["cert_file"] = value(path(&a.cert_file));
    doc["tls"]["key_file"] = value(path(&a.key_file));
    doc["backup"]["target_dir"] = value(path(&a.backup_dir));
    Ok(doc.to_string())
}
