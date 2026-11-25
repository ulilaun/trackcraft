//! Startreihenfolge nach RFC-0015 (C5), Schritte 2 bis 7. Schritt 1 ist `config::load`, Schritt 8 das Öffnen des Ports.

use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};

use crate::backup::restore::{self, Pending};
use crate::config::Validated;
use crate::db::{Db, ProfileSync};

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

    // Bereitgestelltes Wiederherstellen umschalten (#57), bevor die Datenbank geöffnet wird.
    match restore::apply_pending(cfg).await? {
        None => {
            let (db, schema_version) = open(cfg, None).await?;
            Ok(Started { db, schema_version, _lock: lock })
        }
        Some(Pending::Aborted { operation_id, code, message }) => {
            tracing::warn!(message, "Wiederherstellen abgebrochen");
            let (db, schema_version) = open(cfg, None).await?;
            restore::record_failure(&db, &operation_id, code, &message, false).await;
            Ok(Started { db, schema_version, _lock: lock })
        }
        Some(Pending::Switched { operation_id, backup_id }) => match open(cfg, Some((&operation_id, &backup_id))).await {
            Ok((db, schema_version)) => {
                restore::finish(cfg, &db, &operation_id).await?;
                tracing::info!(backup_id, "Wiederherstellen abgeschlossen");
                Ok(Started { db, schema_version, _lock: lock })
            }
            Err(e) => {
                let message = format!("{e:#}");
                tracing::error!(error = message, "Start mit dem wiederhergestellten Stand gescheitert, wird zurückgenommen");
                restore::rollback(cfg)?;
                let (db, schema_version) = open(cfg, None).await?;
                restore::record_failure(&db, &operation_id, restore::ERROR_FAILED, &message, true).await;
                Ok(Started { db, schema_version, _lock: lock })
            }
        },
    }
}

/// Öffnet die Datenbank und führt Schritte 4 bis 7 aus; bei einem Fehler ist sie wieder geschlossen.
/// `restoring` (Vorgang, Stand) prüft nach dem Umschalten die Funktion, bevor Staging-Reste entfernt werden.
async fn open(cfg: &Validated, restoring: Option<(&str, &str)>) -> anyhow::Result<(Db, i64)> {
    let db = Db::open(&cfg.database_file()).await?;
    match prepare_db(cfg, &db, restoring).await {
        Ok(v) => Ok((db, v)),
        Err(e) => {
            db.close().await;
            Err(e)
        }
    }
}

async fn prepare_db(cfg: &Validated, db: &Db, restoring: Option<(&str, &str)>) -> anyhow::Result<i64> {
    // 4. Schemaversion prüfen
    let pending = db.pending_migrations().await?;
    let mut schema_version = db.schema_version().await?;

    // 5./6. Sicherung vor Migration, dann Migrationen
    if let Some(&(target, _)) = pending.last() {
        if db.has_data().await? {
            crate::backup::before_migration(cfg, db, schema_version, target).await?;
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

    // Funktionsprüfung nach dem Umschalten (#57); scheitert sie, wird zurückgenommen.
    if let Some((_, backup_id)) = restoring {
        restore::verify_switched(cfg, db, backup_id).await?;
        restore::clear_staged(cfg)?;
    }

    // Sportart-Zuordnung: einmalige TOML-Übernahme, dann neue Vorgaben.
    let sports = crate::sports::ensure(cfg, db).await?;
    if sports.toml_taken_over {
        tracing::info!("Sportart-Zuordnung aus der TOML in die Datenbank übernommen; die Abschnitte werden ab jetzt ignoriert");
    }
    if sports.defaults_applied > 0 {
        tracing::info!(count = sports.defaults_applied, "Neue Vorgaben der Sportart-Zuordnung eingetragen");
    }

    // Demo: Daten auf heute verschieben (nur Demo-Datenbank, sonst Abbruch)
    if cfg.raw.demo.enabled {
        let today = chrono::Utc::now().with_timezone(&cfg.timezone).date_naive();
        match crate::demo::shift_to(db, cfg.timezone, today).await? {
            Some(s) => tracing::info!(days = s.days, activities = s.activities, last_day = %s.last_day, "Demo-Daten auf heute verschoben"),
            None => tracing::info!("Demo-Daten sind aktuell"),
        }
    }

    // Unterbrochene Importe zurücknehmen, Staging aufräumen, Aufbewahrung anwenden (ADR-0011, C12).
    crate::import_job::recover(cfg, db).await?;
    // Berechnungsstand prüfen, fehlende Hysteresewerte und Positionsangaben nachtragen (M2, RFC-0007).
    crate::calculation::ensure_published(cfg, db).await?;
    let categorized = crate::segments::store::backfill_categories(db, cfg.raw.segments.route_loop_radius_m).await?;
    if categorized > 0 {
        tracing::info!(categorized, "Kategorien der Segmente nachgetragen bzw. neu eingeordnet");
    }
    let interrupted = crate::backup::recover(db, restoring.map(|(op, _)| op)).await?;
    if interrupted > 0 {
        tracing::warn!(interrupted, "Abgebrochene Sicherungsvorgänge als fehlgeschlagen beendet");
    }
    let requeued = crate::jobs::recover(db).await?;
    if requeued > 0 {
        tracing::info!(requeued, "Unterbrochene Auswertungen erneut eingereiht");
    }
    Ok(schema_version)
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
