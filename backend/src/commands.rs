//! Plattformübergreifende Befehle: `check-config` (prüft, startet nichts, schreibt nichts).

use std::path::Path;

use crate::config;

/// `demo mark`: Datenbank der Konfiguration als Demo-Datenbank kennzeichnen (CI, nach `seed_demo.py`, Dienst gestoppt).
pub fn demo_mark(path: &Path) -> anyhow::Result<()> {
    let cfg = config::load(path)?;
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
    runtime.block_on(async {
        let started = crate::startup::prepare(&cfg).await?;
        let last = crate::demo::mark(&started.db, cfg.timezone).await?;
        // Alles in die Hauptdatei, damit sie allein kopiert werden kann
        sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)").execute(&started.db.writer).await?;
        started.db.close().await;
        println!("Demo-Datenbank gekennzeichnet, letzte Aktivität {last}: {}", cfg.database_file().display());
        Ok(())
    })
}

/// Gibt das Prüfergebnis aus; `true`, wenn TrackCraft mit dieser Datei starten kann.
pub fn check_config_report(path: &Path) -> bool {
    println!("Konfiguration: {}", path.display());
    let cfg = match config::load(path) {
        Ok(c) => c,
        Err(e) => {
            println!("FEHLER {e}");
            return false;
        }
    };
    let mut ok = true;
    let mut fail = |msg: String| {
        println!("FEHLER {msg}");
        ok = false;
    };

    if let Some(tls) = &cfg.raw.tls {
        match crate::tls::server_config(&tls.cert_file, &tls.key_file) {
            Ok(_) => println!("ok     Zertifikat und Schlüssel passen zusammen"),
            Err(e) if is_permission_denied(&e) => {
                println!("Hinweis Schlüssel nur für Dienst und Administratoren lesbar, Prüfung als Administrator wiederholen")
            }
            Err(e) => fail(format!("{e:#}")),
        }
    }
    if let Some(dir) = &cfg.raw.server.static_dir {
        if dir.join("index.html").is_file() {
            println!("ok     Frontend in {}", dir.display());
        } else {
            fail(format!("[server] static_dir {} enthält keine index.html", dir.display()));
        }
    }

    let p = &cfg.raw.paths;
    for (key, dir) in [
        ("database_dir", &p.database_dir),
        ("activities_dir", &p.activities_dir),
        ("logs_dir", &p.logs_dir),
        ("migration_dir", &cfg.raw.backup.migration_dir),
    ] {
        if dir.is_dir() {
            println!("ok     {key} {}", dir.display());
        } else if dir.exists() {
            fail(format!("{key} {} ist kein Ordner", dir.display()));
        } else {
            println!("Hinweis {key} {} fehlt und wird beim Start angelegt", dir.display());
        }
    }
    if p.database_dir.is_dir() && p.activities_dir.is_dir() {
        match crate::platform::same_volume(&p.database_dir, &p.activities_dir) {
            Ok(true) => {}
            Ok(false) => fail("database_dir und activities_dir liegen auf verschiedenen Laufwerken (ADR-0011)".into()),
            Err(e) => fail(format!("Laufwerk von database_dir/activities_dir nicht bestimmbar: {e}")),
        }
    }
    let target = &cfg.raw.backup.target_dir;
    if target.is_dir() {
        println!("ok     Sicherungsziel {} erreichbar", target.display());
    } else {
        println!("Hinweis Sicherungsziel {} derzeit nicht erreichbar (kein Startfehler)", target.display());
    }

    for o in &cfg.origins {
        println!("ok     Adresse {}", o.origin);
    }
    println!("ok     lauscht auf {}{}", cfg.listen, if cfg.raw.tls.is_some() { " (HTTPS)" } else { " (HTTP, nur intern)" });
    if ok {
        println!("Ergebnis: gültig");
    } else {
        println!("Ergebnis: ungültig");
    }
    ok
}

fn is_permission_denied(e: &anyhow::Error) -> bool {
    e.chain().any(|c| {
        c.downcast_ref::<std::io::Error>().is_some_and(|io| io.kind() == std::io::ErrorKind::PermissionDenied)
            || c.to_string().contains("Zugriff verweigert")
            || c.to_string().to_lowercase().contains("access is denied")
    })
}
