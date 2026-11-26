//! Abruf aus Garmin Connect über die persönliche Garmin-API (E4): einmaliger Gesamtimport der Historie
//! und danach regelmäßiger Abruf neuer Aktivitäten. Ein Planer prüft jede Minute, ob ein Lauf fällig ist; die
//! Oberfläche kann einen Lauf sofort anstoßen. Es läuft immer höchstens ein Lauf.

pub mod client;
pub mod store;
pub mod sync;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use tokio::sync::Notify;

use crate::config::Validated;
use crate::db::Db;
use crate::secrets::Secrets;

/// Abstand, in dem der Planer nachsieht.
const TICK: Duration = Duration::from_secs(60);

/// Fortschritt des laufenden bzw. letzten Laufs (nur im Speicher).
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct Progress {
    /// token, list, download, import
    pub phase: &'static str,
    /// Zu bearbeitende Aktivitäten dieses Laufs; steht fest, sobald die Liste gelesen ist (davor 0)
    pub total: u64,
    pub processed: u64,
    pub imported: u64,
    pub linked: u64,
    pub locked: u64,
    pub rejected: u64,
    pub failed: u64,
}

/// Gemeinsamer Zustand von Planer und API.
#[derive(Default)]
pub struct Runner {
    running: AtomicBool,
    requested: AtomicBool,
    wake: Notify,
    progress: Mutex<Progress>,
}

impl Runner {
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    pub fn progress(&self) -> Progress {
        self.progress.lock().map(|p| p.clone()).unwrap_or_default()
    }

    fn set_progress(&self, p: Progress) {
        if let Ok(mut g) = self.progress.lock() {
            *g = p;
        }
    }

    fn update(&self, f: impl FnOnce(&mut Progress)) {
        if let Ok(mut g) = self.progress.lock() {
            f(&mut g);
        }
    }

    /// Lauf sofort anfordern; `false`, wenn schon einer läuft.
    pub fn request(&self) -> bool {
        if self.is_running() {
            return false;
        }
        self.requested.store(true, Ordering::SeqCst);
        self.wake.notify_one();
        true
    }

    /// Führt einen Lauf aus, wenn keiner läuft (Planer, Tests).
    pub async fn run_once(&self, cfg: &Arc<Validated>, db: &Db, secrets: &Secrets) -> Option<Result<Progress, sync::RunError>> {
        if self.running.swap(true, Ordering::SeqCst) {
            return None;
        }
        let r = sync::run(cfg, db, self, secrets).await;
        if let Ok(mut g) = self.progress.lock() {
            g.phase = "idle";
        }
        self.running.store(false, Ordering::SeqCst);
        Some(r)
    }
}

/// Fällig: eingeschaltet, Schlüssel vorhanden und Intervall seit dem letzten Start verstrichen.
fn due(s: &store::Settings, now: i64) -> bool {
    s.enabled && s.api_key.is_some() && s.last_run_started_ms.is_none_or(|t| now - t >= s.interval_minutes * 60_000)
}

/// Läuft bis zum Abbruch der Aufgabe.
pub async fn scheduler(cfg: Arc<Validated>, db: Db, runner: Arc<Runner>, secrets: Arc<Secrets>) {
    loop {
        tokio::select! {
            _ = runner.wake.notified() => {}
            _ = tokio::time::sleep(TICK) => {}
        }
        let requested = runner.requested.swap(false, Ordering::SeqCst);
        let run = match store::load(&db, &secrets).await {
            Ok(s) => (requested && s.api_key.is_some()) || due(&s, chrono::Utc::now().timestamp_millis()),
            Err(e) => {
                tracing::error!(error = format!("{e:#}"), "Garmin-Einstellungen nicht lesbar");
                false
            }
        };
        if run {
            // Fehler sind im Status vermerkt und geloggt.
            let _ = runner.run_once(&cfg, &db, &secrets).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn due_needs_enabled_key_and_interval() {
        let mut s = store::Settings { interval_minutes: 30, ..Default::default() };
        assert!(!due(&s, 0));
        s.enabled = true;
        assert!(!due(&s, 0));
        s.api_key = Some("k".into());
        assert!(due(&s, 0));
        s.last_run_started_ms = Some(1_000_000);
        assert!(!due(&s, 1_000_000 + 29 * 60_000));
        assert!(due(&s, 1_000_000 + 30 * 60_000));
    }
}
