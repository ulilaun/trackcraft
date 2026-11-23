//! Startreihenfolge nach RFC-0015 (C5), Schritte 2 bis 7. Schritt 1 ist `config::load`, Schritt 8 das Öffnen des Ports.

use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};

use crate::config::Validated;
use crate::db::{self, Db, ProfileSync};

/// Laufender Zustand nach erfolgreichem Start. Die Sperrdatei bleibt belegt, solange er lebt.
pub struct Started {
    pub db: Db,
    pub schema_version: i64,
    _lock: File,
}

pub async fn prepare(cfg: &Validated) -> anyhow::Result<Started> {
    // 2. Sperrdatei
    let db_dir = &cfg.raw.paths.database_dir;
    fs::create_dir_all(db_dir).with_context(|| format!("Datenbankordner {} lässt sich nicht anlegen", db_dir.display()))?;
    let lock = acquire_lock(&db_dir.join("trackcraft.lock"))?;

    // 3. Datenordner, Migrationsordner, Staging
    for dir in data_dirs(cfg) {
        fs::create_dir_all(&dir).with_context(|| format!("Ordner {} lässt sich nicht anlegen", dir.display()))?;
        check_writable(&dir)?;
    }
    if !crate::platform::same_volume(&cfg.raw.paths.database_dir, &cfg.raw.paths.activities_dir)? {
        bail!("database_dir und activities_dir müssen auf demselben Laufwerk liegen (Staging, ADR-0011)");
    }

    // 4. Datenbank öffnen, Schemaversion prüfen
    let db = Db::open(&cfg.database_file()).await?;
    let pending = db.pending_migrations().await?;
    let mut schema_version = db.schema_version().await?;

    // 5./6. Sicherung vor Migration, dann Migrationen
    if let Some(&(target, _)) = pending.last() {
        if db.has_data().await? {
            backup_before_migration(cfg, &db, schema_version, target).await?;
        }
        db.migrate(&pending).await?;
        schema_version = db.schema_version().await?;
        tracing::info!(schema_version, "Migrationen abgeschlossen");
    }

    // 7. Profilabgleich
    match db.sync_profile(cfg.raw.user.as_ref().map(|u| u.display_name.trim())).await? {
        ProfileSync::Created => tracing::info!("Benutzer angelegt"),
        ProfileSync::TomlIgnored => {
            tracing::info!(
                "[user] display_name weicht vom Spitznamen ab und wird ignoriert; der Spitzname wird auf der Kontoseite geändert"
            )
        }
        ProfileSync::Unchanged => {}
    }

    // Sportart-Zuordnung: einmalige TOML-Übernahme, dann neue Vorgaben.
    let sports = crate::sports::ensure(cfg, &db).await?;
    if sports.toml_taken_over {
        tracing::info!("Sportart-Zuordnung aus der TOML in die Datenbank übernommen; die Abschnitte werden ab jetzt ignoriert");
    }
    if sports.defaults_applied > 0 {
        tracing::info!(count = sports.defaults_applied, "Neue Vorgaben der Sportart-Zuordnung eingetragen");
    }

    // Demo: Daten auf heute verschieben (nur Demo-Datenbank, sonst Abbruch)
    if cfg.raw.demo.enabled {
        let today = chrono::Utc::now().with_timezone(&cfg.timezone).date_naive();
        match crate::demo::shift_to(&db, cfg.timezone, today).await? {
            Some(s) => tracing::info!(days = s.days, activities = s.activities, last_day = %s.last_day, "Demo-Daten auf heute verschoben"),
            None => tracing::info!("Demo-Daten sind aktuell"),
        }
    }

    // Unterbrochene Importe zurücknehmen, Staging aufräumen, Aufbewahrung anwenden (ADR-0011, C12).
    crate::import_job::recover(cfg, &db).await?;
    // Berechnungsstand prüfen, fehlende Hysteresewerte und Positionsangaben nachtragen (M2, RFC-0007).
    crate::calculation::ensure_published(cfg, &db).await?;
    let categorized = crate::segments::store::backfill_categories(&db, cfg.raw.segments.route_loop_radius_m).await?;
    if categorized > 0 {
        tracing::info!(categorized, "Kategorien der Segmente nachgetragen bzw. neu eingeordnet");
    }
    let requeued = crate::jobs::recover(&db).await?;
    if requeued > 0 {
        tracing::info!(requeued, "Unterbrochene Auswertungen erneut eingereiht");
    }

    Ok(Started { db, schema_version, _lock: lock })
}

pub fn data_dirs(cfg: &Validated) -> Vec<PathBuf> {
    vec![
        cfg.raw.paths.database_dir.clone(),
        cfg.raw.paths.activities_dir.clone(),
        cfg.raw.paths.logs_dir.clone(),
        cfg.raw.backup.migration_dir.clone(),
        cfg.staging_dir(),
    ]
}

fn acquire_lock(path: &Path) -> anyhow::Result<File> {
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(path)
        .with_context(|| format!("Sperrdatei {} lässt sich nicht öffnen", path.display()))?;
    match file.try_lock() {
        Ok(()) => Ok(file),
        Err(fs::TryLockError::WouldBlock) => {
            bail!("TrackCraft läuft bereits auf diesen Daten (Sperrdatei {} ist belegt)", path.display())
        }
        Err(fs::TryLockError::Error(e)) => Err(e).with_context(|| format!("Sperrdatei {} lässt sich nicht sperren", path.display())),
    }
}

pub fn check_writable(dir: &Path) -> anyhow::Result<()> {
    let probe = dir.join(".trackcraft-schreibtest");
    fs::write(&probe, b"ok").with_context(|| format!("Ordner {} ist nicht beschreibbar", dir.display()))?;
    fs::remove_file(&probe).with_context(|| format!("Ordner {} ist nicht beschreibbar", dir.display()))?;
    Ok(())
}

/// Lokale Sicherung vor Migration, nur die letzte bleibt (RFC-0012). Scheitert sie, startet das Backend nicht.
///
/// Stand M0: Gesichert wird die Datenbank per `VACUUM INTO`. Originale und das vollständige Format
/// `trackcraft-backup` mit Kopie ins Sicherungsziel folgen mit M5 (RFC-0012, C13).
async fn backup_before_migration(cfg: &Validated, db: &Db, from: i64, to: i64) -> anyhow::Result<()> {
    let dir = &cfg.raw.backup.migration_dir;
    let target = db::migration_backup_path(dir, from, to);
    let partial = target.with_extension("db.partial");
    let _ = fs::remove_file(&partial);
    db.vacuum_into(&partial)
        .await
        .with_context(|| format!("Sicherung vor Migration nach {} fehlgeschlagen, es wird nicht migriert", dir.display()))?;
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        if db::is_migration_backup(&name) {
            fs::remove_file(entry.path())?;
        }
    }
    fs::rename(&partial, &target)?;
    tracing::info!(path = %target.display(), from, to, "Sicherung vor Migration angelegt");
    Ok(())
}
