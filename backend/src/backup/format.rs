//! Sicherungsformat `trackcraft-backup` (#57, RFC-0012): ein Ordner je Stand mit
//!
//! ```text
//! trackcraft-20261126-030000-s21-c3120/
//!   manifest.json     Formatkennung, Version, Datenstand, SHA-256 und Größe je Datei
//!   trackcraft.db     konsistente Kopie per VACUUM INTO
//!   trackcraft.toml   Referenzkopie der wirksamen Konfiguration (wird nie automatisch aktiviert)
//!   originals/<sha>.fit|gpx
//! ```
//!
//! Der Name nennt Ortszeit, Schemaversion und Änderungszähler. Ein Stand entsteht als `<name>.partial`, wird nach dem
//! Kopieren gegen das Manifest geprüft und erst dann umbenannt; nur Ordner ohne `.partial` zählen als Stand.
//! Pfade im Manifest sind relativ mit `/` (plattformunabhängig).

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::Row;

use crate::config::Validated;
use crate::db::Db;

/// Unveränderliche Formatkennung (RFC-0021).
pub const FORMAT: &str = "trackcraft-backup";
pub const FORMAT_VERSION: i64 = 1;
pub const MANIFEST: &str = "manifest.json";
pub const DB_FILE: &str = "trackcraft.db";
pub const TOML_FILE: &str = "trackcraft.toml";
pub const ORIGINALS: &str = "originals";
pub const PARTIAL: &str = ".partial";
/// Aufbewahrung: die letzten fünf erfolgreichen Stände (ADR-0014).
pub const KEEP: usize = 5;
const PREFIX: &str = "trackcraft-";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DataState {
    pub dataset_id: String,
    pub revision_id: String,
    pub predecessor_revision_id: Option<String>,
    pub change_counter: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileEntry {
    pub path: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub format: String,
    pub format_version: i64,
    pub created_at: String,
    /// daily, manual oder migration
    pub trigger: String,
    pub app_version: String,
    pub schema_version: i64,
    /// Parameter der Neuberechnung beim Sichern (`calculation::parameters_hash`)
    pub parameters_hash: String,
    pub data_state: Option<DataState>,
    #[serde(default)]
    pub activity_count: i64,
    #[serde(default)]
    pub segment_count: i64,
    /// In der Datenbank verzeichnet, aber beim Sichern nicht im Archiv vorhanden (relative Pfade)
    #[serde(default)]
    pub missing_originals: Vec<String>,
    /// Im Archiv vorhanden, aber mit anderer Prüfsumme als in der Datenbank
    #[serde(default)]
    pub damaged_originals: Vec<String>,
    pub hash: String,
    pub files: Vec<FileEntry>,
}

impl Manifest {
    pub fn total_bytes(&self) -> u64 {
        self.files.iter().map(|f| f.size).sum()
    }

    pub fn originals(&self) -> usize {
        self.files.iter().filter(|f| f.path.starts_with(&format!("{ORIGINALS}/"))).count()
    }
}

/// Name eines Stands aus Ortszeit, Schemaversion und Änderungszähler.
pub fn stand_name(local: chrono::NaiveDateTime, schema_version: i64, counter: i64) -> String {
    format!("{PREFIX}{}-s{schema_version}-c{counter}", local.format("%Y%m%d-%H%M%S"))
}

/// Fertiger Stand: erwartetes Präfix, nur Kleinbuchstaben, Ziffern und `-`, kein `.partial`.
pub fn is_stand_name(name: &str) -> bool {
    name.starts_with(PREFIX)
        && name.len() > PREFIX.len()
        && name.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !name.starts_with("trackcraft-vor-migration-")
}

fn is_partial_name(name: &str) -> bool {
    name.strip_suffix(PARTIAL).is_some_and(is_stand_name)
}

/// Zustand des Sicherungsziels (`TargetState`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetState {
    Available,
    Unavailable,
    NotWritable,
}

/// Ziel vorhanden und beschreibbar? Das Ziel selbst wird nie angelegt: fehlt der Ordner, fehlt das Laufwerk.
pub fn target_state(dir: &Path) -> (TargetState, Option<String>) {
    if !dir.is_dir() {
        return (TargetState::Unavailable, Some(format!("{} ist nicht vorhanden", dir.display())));
    }
    match crate::startup::check_writable(dir) {
        Ok(()) => (TargetState::Available, None),
        Err(e) => (TargetState::NotWritable, Some(format!("{e:#}"))),
    }
}

/// Wie [`target_state`], als Ergebnis.
pub fn target_ready(dir: &Path) -> Result<(), String> {
    match target_state(dir) {
        (TargetState::Available, _) => Ok(()),
        (_, e) => Err(e.unwrap_or_default()),
    }
}

/// Parameterstand, der eine Sicherung auslöst, wenn er sich ändert: Neuberechnung und Zeitzone (RFC-0012).
pub fn parameters(cfg: &Validated) -> String {
    format!("{}|{}", crate::calculation::parameters_hash(cfg), cfg.timezone.name())
}

pub fn sha256_file(path: &Path) -> anyhow::Result<(u64, String)> {
    let mut f = fs::File::open(path).with_context(|| format!("{} lässt sich nicht lesen", path.display()))?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    let mut size = 0u64;
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        size += n as u64;
        h.update(&buf[..n]);
    }
    Ok((size, h.finalize().iter().map(|b| format!("{b:02x}")).collect()))
}

/// Harter Link auf demselben Laufwerk, sonst Kopie. Originale sind unveränderlich; ein späteres Löschen im Archiv
/// entfernt nur den anderen Namen.
fn link_or_copy(src: &Path, dst: &Path) -> anyhow::Result<()> {
    if fs::hard_link(src, dst).is_err() {
        fs::copy(src, dst).with_context(|| format!("{} lässt sich nicht nach {} kopieren", src.display(), dst.display()))?;
    }
    Ok(())
}

async fn table_exists(conn: &mut sqlx::SqliteConnection, name: &str) -> sqlx::Result<bool> {
    let n: i64 =
        sqlx::query_scalar("SELECT count(*) FROM sqlite_schema WHERE type = 'table' AND name = ?").bind(name).fetch_one(conn).await?;
    Ok(n > 0)
}

/// Ergebnis von [`snapshot`].
pub struct Snapshot {
    pub manifest: Manifest,
}

/// Legt in `dir` (darf nicht existieren) einen vollständigen, geprüften Stand an.
///
/// Solange die einzige schreibende Verbindung belegt ist, warten alle Schreiber; Lesen geht weiter. In dieser kurzen
/// Sperre entstehen die Datenbankkopie und die Links auf die referenzierten Originale, danach wird ohne Sperre
/// gehasht. Funktioniert auch mit einer Datenbank älterer Schemaversion (Sicherung vor Migration).
pub async fn snapshot(cfg: &Validated, db: &Db, dir: &Path, trigger: &str) -> anyhow::Result<Snapshot> {
    fs::create_dir_all(dir.join(ORIGINALS)).with_context(|| format!("{} lässt sich nicht anlegen", dir.display()))?;

    let (schema_version, data_state, originals, missing, activity_count, segment_count) = {
        let mut conn = db.writer.acquire().await?;
        let schema_version: i64 = sqlx::query_scalar("PRAGMA user_version").fetch_one(&mut *conn).await?;
        sqlx::query("VACUUM INTO ?")
            .bind(dir.join(DB_FILE).to_string_lossy().to_string())
            .execute(&mut *conn)
            .await
            .context("Datenbankkopie (VACUUM INTO) fehlgeschlagen")?;
        let data_state = if table_exists(&mut conn, "data_state").await? {
            sqlx::query("SELECT dataset_id, revision_id, predecessor_revision_id, change_counter FROM data_state WHERE id = 1")
                .fetch_optional(&mut *conn)
                .await?
                .map(|r| DataState {
                    dataset_id: r.get("dataset_id"),
                    revision_id: r.get("revision_id"),
                    predecessor_revision_id: r.get("predecessor_revision_id"),
                    change_counter: r.get("change_counter"),
                })
        } else {
            None
        };
        let originals: Vec<(String, String, i64)> = if table_exists(&mut conn, "original_files").await? {
            sqlx::query("SELECT relative_path, sha256, size_bytes FROM original_files ORDER BY relative_path")
                .fetch_all(&mut *conn)
                .await?
                .iter()
                .map(|r| (r.get("relative_path"), r.get("sha256"), r.get("size_bytes")))
                .collect()
        } else {
            vec![]
        };
        let mut counts = [0i64; 2];
        for (i, table) in ["activities", "segments"].into_iter().enumerate() {
            if table_exists(&mut conn, table).await? {
                // Fester Tabellenname aus dieser Liste, kein Benutzerwert.
                counts[i] = sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT count(*) FROM {table}"))).fetch_one(&mut *conn).await?;
            }
        }
        // Links noch unter der Sperre: ein gleichzeitiges Löschen darf den Zwischenstand nicht beschädigen.
        let mut sources = Vec::with_capacity(originals.len());
        for (rel, _, _) in &originals {
            sources.push((crate::import_job::archive_path(cfg, rel)?, original_dest(dir, rel)?));
        }
        // Fehlt ein Original im Archiv, wird der Rest trotzdem gesichert und die Lücke im Manifest vermerkt;
        // sonst verhinderte eine einzelne verlorene Datei jede Sicherung und jedes Update.
        let missing = tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<usize>> {
            let mut missing = vec![];
            for (i, (src, dst)) in sources.into_iter().enumerate() {
                if src.is_file() {
                    link_or_copy(&src, &dst)?;
                } else {
                    missing.push(i);
                }
            }
            Ok(missing)
        })
        .await??;
        (schema_version, data_state, originals, missing, counts[0], counts[1])
    };

    if cfg.path.is_file() {
        fs::copy(&cfg.path, dir.join(TOML_FILE)).with_context(|| format!("{} lässt sich nicht kopieren", cfg.path.display()))?;
    }

    let missing_originals: Vec<String> = missing.iter().map(|&i| originals[i].0.clone()).collect();
    if let Some(first) = missing_originals.first() {
        tracing::warn!(count = missing_originals.len(), first = %first, "Originale fehlen im Archiv und fehlen auch in der Sicherung");
    }
    let dir_owned = dir.to_path_buf();
    let skip = missing;
    let (files, damaged_originals) = tokio::task::spawn_blocking(move || -> anyhow::Result<(Vec<FileEntry>, Vec<String>)> {
        let mut files = Vec::with_capacity(originals.len() + 2);
        let mut damaged = vec![];
        for name in [DB_FILE, TOML_FILE] {
            let p = dir_owned.join(name);
            if p.is_file() {
                let (size, sha256) = sha256_file(&p)?;
                files.push(FileEntry { path: name.to_string(), size, sha256 });
            }
        }
        for (i, (rel, expected, _)) in originals.iter().enumerate() {
            if skip.contains(&i) {
                continue;
            }
            let (size, sha256) = sha256_file(&original_dest(&dir_owned, rel)?)?;
            // Gesichert wird, was im Archiv liegt; eine abweichende Prüfsumme wird vermerkt.
            if &sha256 != expected {
                damaged.push(rel.clone());
            }
            files.push(FileEntry { path: format!("{ORIGINALS}/{rel}"), size, sha256 });
        }
        Ok((files, damaged))
    })
    .await??;
    if let Some(first) = damaged_originals.first() {
        tracing::warn!(count = damaged_originals.len(), first = %first, "Originale im Archiv weichen von ihrer Prüfsumme ab");
    }

    let manifest = Manifest {
        format: FORMAT.to_string(),
        format_version: FORMAT_VERSION,
        created_at: crate::import_job::timestamp(chrono::Utc::now().timestamp_millis()),
        trigger: trigger.to_string(),
        app_version: crate::version::APP_VERSION.to_string(),
        schema_version,
        parameters_hash: parameters(cfg),
        data_state,
        activity_count,
        segment_count,
        missing_originals,
        damaged_originals,
        hash: "sha256".to_string(),
        files,
    };
    fs::write(dir.join(MANIFEST), serde_json::to_vec_pretty(&manifest)?)?;
    Ok(Snapshot { manifest })
}

/// Zielpfad eines Originals im Stand; nur einfache Dateinamen bzw. relative Pfade ohne `..` (RFC-0011).
fn original_dest(dir: &Path, rel: &str) -> anyhow::Result<PathBuf> {
    safe_join(&dir.join(ORIGINALS), rel)
}

/// Hängt einen relativen `/`-Pfad an; lehnt absolute Pfade, `..`, Laufwerke und leere Teile ab.
pub fn safe_join(base: &Path, rel: &str) -> anyhow::Result<PathBuf> {
    let mut out = base.to_path_buf();
    for part in rel.split('/') {
        if part.is_empty() || part == "." || part == ".." || part.contains(['\\', ':']) {
            bail!("Unzulässiger Pfad im Sicherungsstand: {rel}");
        }
        out.push(part);
    }
    Ok(out)
}

/// Liest das Manifest eines Stands und prüft Formatkennung und -version.
pub fn read_manifest(dir: &Path) -> anyhow::Result<Manifest> {
    let bytes = fs::read(dir.join(MANIFEST)).with_context(|| format!("{} hat kein Manifest", dir.display()))?;
    let m: Manifest = serde_json::from_slice(&bytes).context("Manifest unlesbar")?;
    if m.format != FORMAT {
        bail!("Unbekanntes Sicherungsformat {}", m.format);
    }
    if m.format_version > FORMAT_VERSION {
        bail!("Sicherungsformat Version {} ist neuer als diese TrackCraft-Version ({FORMAT_VERSION})", m.format_version);
    }
    Ok(m)
}

/// Prüft jede Datei eines Stands gegen das Manifest (Größe und SHA-256).
pub fn verify(dir: &Path, m: &Manifest) -> anyhow::Result<()> {
    for f in &m.files {
        let p = safe_join(dir, &f.path)?;
        let (size, sha) = sha256_file(&p)?;
        if size != f.size || sha != f.sha256 {
            bail!("{} weicht vom Manifest ab", f.path);
        }
    }
    Ok(())
}

/// Kopiert einen Stand aus `src` nach `target/<name>`: erst als `.partial`, dann prüfen, dann umbenennen.
/// Synchron; aus async-Code per `spawn_blocking` aufrufen.
pub fn copy_stand(src: &Path, target: &Path, name: &str) -> anyhow::Result<PathBuf> {
    let m = read_manifest(src)?;
    let final_dir = target.join(name);
    if final_dir.exists() {
        bail!("Stand {name} gibt es im Ziel schon");
    }
    let partial = target.join(format!("{name}{PARTIAL}"));
    if partial.exists() {
        fs::remove_dir_all(&partial)?;
    }
    let result = (|| -> anyhow::Result<()> {
        fs::create_dir_all(&partial)?;
        for f in &m.files {
            let from = safe_join(src, &f.path)?;
            let to = safe_join(&partial, &f.path)?;
            if let Some(parent) = to.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&from, &to).with_context(|| format!("{} lässt sich nicht ins Ziel kopieren", f.path))?;
        }
        fs::copy(src.join(MANIFEST), partial.join(MANIFEST)).context("Manifest lässt sich nicht ins Ziel kopieren")?;
        // Gelesen wird aus dem Ziel: erst wenn dort alles stimmt, gilt der Stand.
        let copied = read_manifest(&partial)?;
        verify(&partial, &copied).context("Prüfung im Ziel fehlgeschlagen")?;
        fs::rename(&partial, &final_dir).context("Stand lässt sich im Ziel nicht umbenennen")?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&partial);
    }
    result.map(|()| final_dir)
}

/// Ein Stand im Ziel, wie ihn die Liste zeigt (ohne erneutes Hashen).
#[derive(Debug, Clone)]
pub struct StandInfo {
    pub name: String,
    pub manifest: Option<Manifest>,
    /// Manifest unlesbar, fremdes Format oder Dateien fehlen
    pub problem: Option<String>,
}

/// Alle fertigen Stände im Ziel, neueste zuerst. Prüft Manifest und Vorhandensein und Größe der Dateien.
pub fn list(target: &Path) -> anyhow::Result<Vec<StandInfo>> {
    let mut out = vec![];
    for entry in fs::read_dir(target).with_context(|| format!("{} lässt sich nicht lesen", target.display()))? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        if !entry.file_type()?.is_dir() || !is_stand_name(&name) {
            continue;
        }
        let dir = entry.path();
        out.push(match read_manifest(&dir) {
            Ok(m) => {
                let missing = m
                    .files
                    .iter()
                    .find(|f| safe_join(&dir, &f.path).ok().and_then(|p| fs::metadata(p).ok()).is_none_or(|md| md.len() != f.size));
                let problem = missing.map(|f| format!("{} fehlt oder hat die falsche Größe", f.path));
                StandInfo { name, manifest: Some(m), problem }
            }
            Err(e) => StandInfo { name, manifest: None, problem: Some(format!("{e:#}")) },
        });
    }
    // Der Name beginnt mit dem Zeitstempel; neueste zuerst.
    out.sort_by(|a, b| b.name.cmp(&a.name));
    Ok(out)
}

/// Aufbewahrung nach einem erfolgreichen Stand: die neuesten `KEEP` gültigen Stände bleiben, ältere gültige werden
/// entfernt; dazu Reste abgebrochener Läufe (`.partial`). Ungültige oder fremde Ordner bleiben unangetastet.
pub fn retention(target: &Path) -> anyhow::Result<Vec<String>> {
    let mut removed = vec![];
    for entry in fs::read_dir(target)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        if entry.file_type()?.is_dir() && is_partial_name(&name) {
            fs::remove_dir_all(entry.path())?;
            removed.push(name);
        }
    }
    let valid: Vec<StandInfo> = list(target)?.into_iter().filter(|s| s.problem.is_none()).collect();
    for s in valid.iter().skip(KEEP) {
        fs::remove_dir_all(target.join(&s.name)).with_context(|| format!("Alter Stand {} lässt sich nicht entfernen", s.name))?;
        removed.push(s.name.clone());
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        let t = chrono::NaiveDate::from_ymd_opt(2026, 11, 26).unwrap().and_hms_opt(3, 0, 5).unwrap();
        let n = stand_name(t, 21, 3120);
        assert_eq!(n, "trackcraft-20261126-030005-s21-c3120");
        assert!(is_stand_name(&n));
        assert!(!is_stand_name(&format!("{n}.partial")));
        assert!(is_partial_name(&format!("{n}.partial")));
        assert!(!is_stand_name("trackcraft-vor-migration-v20-v21"));
        assert!(!is_stand_name("Fotos"));
        assert!(!is_stand_name("trackcraft-"));
    }

    #[test]
    fn safe_join_rejects_escapes() {
        let base = Path::new("x");
        assert!(safe_join(base, "originals/abc.fit").is_ok());
        for bad in ["../a", "a/../b", "/a", "a//b", "c:/a", "a\\b", ""] {
            assert!(safe_join(base, bad).is_err(), "{bad}");
        }
    }
}
