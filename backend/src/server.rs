//! Start des Servers (Schritt 8, RFC-0015) und geordnetes Beenden.

use std::future::Future;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::Context;
use axum_server::Handle;
use axum_server::tls_rustls::RustlsConfig;
use tokio::sync::Notify;

use crate::api::{self, AppState};
use crate::config::Validated;
use crate::{startup, tls};

/// Startet TrackCraft vollständig und läuft, bis `shutdown` endet.
/// `on_listening` wird mit der tatsächlichen Adresse aufgerufen, sobald der Port zum ersten Mal offen ist.
///
/// Fordert ein Vorgang den internen Neustart an (Wiederherstellen, #57), fährt der Server geordnet herunter, schließt
/// die Datenbank, gibt die Sperrdatei frei und durchläuft den Start erneut; der Prozess bleibt dabei bestehen.
pub async fn run<S, L>(cfg: Validated, shutdown: S, on_listening: L) -> anyhow::Result<()>
where
    S: Future<Output = ()> + Send + 'static,
    L: FnOnce(SocketAddr) + Send + 'static,
{
    let cfg = Arc::new(cfg);
    // TLS vor dem Datenbankstart laden: ein Zertifikatsfehler soll nichts migrieren.
    let tls = match &cfg.raw.tls {
        Some(t) => Some(tls::server_config(&t.cert_file, &t.key_file)?),
        None => None,
    };

    let stop = Arc::new(Notify::new());
    let stopped = Arc::new(AtomicBool::new(false));
    let (s, f) = (stop.clone(), stopped.clone());
    tokio::spawn(async move {
        shutdown.await;
        tracing::info!("Beenden angefordert");
        f.store(true, Ordering::SeqCst);
        s.notify_waiters();
    });

    let mut on_listening = Some(on_listening);
    loop {
        let restart = serve_once(&cfg, tls.clone(), &stop, &stopped, &mut on_listening).await?;
        if !restart || stopped.load(Ordering::SeqCst) {
            break;
        }
        tracing::info!("Interner Neustart");
    }
    tracing::info!("TrackCraft beendet");
    Ok(())
}

/// Ein Durchlauf von Start bis Beenden; `true`, wenn ein interner Neustart angefordert war.
async fn serve_once<L>(
    cfg: &Arc<Validated>,
    tls: Option<Arc<rustls::ServerConfig>>,
    stop: &Arc<Notify>,
    stopped: &Arc<AtomicBool>,
    on_listening: &mut Option<L>,
) -> anyhow::Result<bool>
where
    L: FnOnce(SocketAddr) + Send + 'static,
{
    let started = startup::prepare(cfg).await?;
    let db = started.db.clone();
    let garmin = Arc::new(crate::garmin::Runner::default());
    let backup = Arc::new(crate::backup::Runner::default());
    let app = api::router(AppState { cfg: cfg.clone(), db: db.clone(), garmin: garmin.clone(), backup: backup.clone() });

    // Aufbewahrung stündlich (C12); der erste Lauf war beim Start.
    let (mcfg, mdb) = (cfg.clone(), db.clone());
    let maintenance = tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(3600));
        tick.tick().await;
        loop {
            tick.tick().await;
            if let Err(e) = crate::import_job::cleanup(&mcfg, &mdb, chrono::Utc::now().timestamp_millis()).await {
                tracing::warn!(error = format!("{e:#}"), "Aufräumen fehlgeschlagen");
            }
        }
    });

    // Auswertungsaufträge im Hintergrund (Segmentversuche, RFC-0007)
    let worker = tokio::spawn(crate::jobs::worker(cfg.clone(), db.clone()));

    // Belastung je Aktivität (E7)
    let load_task = tokio::spawn(crate::load::worker(cfg.clone(), db.clone()));

    // Abruf aus Garmin Connect
    let garmin_task = tokio::spawn(crate::garmin::scheduler(cfg.clone(), db.clone(), garmin));

    // Tägliche Sicherung und „Jetzt sichern“ (#57)
    let backup_task = tokio::spawn(crate::backup::scheduler(cfg.clone(), db.clone(), backup.clone()));

    let handle: Handle<SocketAddr> = Handle::new();
    if let Some(cb) = on_listening.take() {
        let watcher = handle.clone();
        tokio::spawn(async move {
            if let Some(addr) = watcher.listening().await {
                cb(addr);
            }
        });
    }
    let restart = Arc::new(AtomicBool::new(false));
    let (stopper, stop, stopped, r) = (handle.clone(), stop.clone(), stopped.clone(), restart.clone());
    let control = tokio::spawn(async move {
        tokio::select! {
            // Auch ein Beenden zwischen Prüfen und Warten geht nicht verloren: höchstens eine halbe Sekunde später.
            _ = async {
                while !stopped.load(Ordering::SeqCst) {
                    tokio::select! {
                        _ = stop.notified() => {}
                        _ = tokio::time::sleep(Duration::from_millis(500)) => {}
                    }
                }
            } => {}
            _ = backup.restart_requested() => {
                r.store(true, Ordering::SeqCst);
            }
        }
        stopper.graceful_shutdown(Some(Duration::from_secs(10)));
    });

    let addr = cfg.listen;
    let service = app.into_make_service();
    let result = match tls {
        Some(tls) => {
            tracing::info!(%addr, "HTTPS");
            axum_server::bind_rustls(addr, RustlsConfig::from_config(tls)).handle(handle).serve(service).await
        }
        None => {
            tracing::info!(%addr, "HTTP (nur intern, Container)");
            axum_server::bind(addr).handle(handle).serve(service).await
        }
    };
    control.abort();
    maintenance.abort();
    worker.abort();
    load_task.abort();
    garmin_task.abort();
    backup_task.abort();
    // Abgebrochene Aufgaben geben ihre Verbindungen frei, bevor die Datenbank geschlossen wird.
    for t in [maintenance, worker, load_task, garmin_task, backup_task] {
        let _ = t.await;
    }
    db.close().await;
    drop(started);
    result.with_context(|| format!("Port {addr} lässt sich nicht öffnen oder der Server brach ab"))?;
    Ok(restart.load(Ordering::SeqCst))
}

/// Strg+C in der Konsole, SIGTERM im Container.
pub async fn os_shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let term = async {
        if let Ok(mut s) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            s.recv().await;
        }
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {},
        _ = term => {},
    }
}
