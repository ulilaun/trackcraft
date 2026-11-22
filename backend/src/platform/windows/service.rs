//! Betrieb als Windows-Dienst `TrackCraft` (RFC-0005). Der Dienst startet `trackcraft.exe service-run`.

use std::ffi::OsString;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use windows_service::service::{ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus, ServiceType};
use windows_service::service_control_handler::{self, ServiceControlHandlerResult, ServiceStatusHandle};
use windows_service::{define_windows_service, service_dispatcher};

use super::{SERVICE_NAME, event_log_error};
use crate::{config, logging, server};

/// Exit-Codes des Dienstes (dienstspezifisch). Sie gelten nicht als Absturz, daher kein Neustart (RFC-0005).
const EXIT_CONFIG: u32 = 1;
const EXIT_RUNTIME: u32 = 2;

define_windows_service!(ffi_service_main, service_main);

/// Übergibt an den Dienststeuerungs-Manager; kehrt zurück, wenn der Dienst endet.
pub fn run_dispatcher() -> anyhow::Result<()> {
    service_dispatcher::start(SERVICE_NAME, ffi_service_main)
        .map_err(|e| anyhow::anyhow!("Nur vom Dienststeuerungs-Manager aufrufbar ({e}). Für die Konsole: trackcraft.exe ohne Argument."))
}

fn service_main(_args: Vec<OsString>) {
    if let Err(e) = run() {
        event_log_error(&format!("Dienst konnte nicht gestartet werden: {e:#}"));
    }
}

fn status(handle: ServiceStatusHandle, state: ServiceState, exit: ServiceExitCode, wait: Duration) {
    let controls = if state == ServiceState::Running {
        ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN
    } else {
        ServiceControlAccept::empty()
    };
    let _ = handle.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: state,
        controls_accepted: controls,
        exit_code: exit,
        checkpoint: 0,
        wait_hint: wait,
        process_id: None,
    });
}

fn run() -> anyhow::Result<()> {
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let tx = Arc::new(Mutex::new(Some(tx)));
    let handler_tx = tx.clone();
    let handle = service_control_handler::register(SERVICE_NAME, move |control| match control {
        ServiceControl::Stop | ServiceControl::Shutdown | ServiceControl::Preshutdown => {
            if let Some(tx) = handler_tx.lock().ok().and_then(|mut t| t.take()) {
                let _ = tx.send(());
            }
            ServiceControlHandlerResult::NoError
        }
        ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
        _ => ServiceControlHandlerResult::NotImplemented,
    })?;
    // Migrationen können dauern; der Hinweis gibt dem Manager Zeit (Startablauf RFC-0015).
    status(handle, ServiceState::StartPending, ServiceExitCode::Win32(0), Duration::from_secs(900));

    let path = config::config_path();
    let cfg = match config::load(&path) {
        Ok(c) => c,
        Err(e) => {
            event_log_error(&e.to_string());
            status(handle, ServiceState::Stopped, ServiceExitCode::ServiceSpecific(EXIT_CONFIG), Duration::ZERO);
            return Ok(());
        }
    };
    let _guard = match logging::init(&cfg, false) {
        Ok(g) => g,
        Err(e) => {
            event_log_error(&format!("Logdateien in {} nicht nutzbar: {e:#}", cfg.raw.paths.logs_dir.display()));
            status(handle, ServiceState::Stopped, ServiceExitCode::ServiceSpecific(EXIT_CONFIG), Duration::ZERO);
            return Ok(());
        }
    };
    tracing::info!(version = %crate::version::long(), config = %path.display(), "TrackCraft startet als Dienst");

    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
    let result = runtime.block_on(server::run(
        cfg,
        async move {
            let _ = rx.await;
        },
        move |addr| {
            tracing::info!(%addr, "Dienst läuft");
            status(handle, ServiceState::Running, ServiceExitCode::Win32(0), Duration::ZERO);
        },
    ));
    let exit = match &result {
        Ok(()) => ServiceExitCode::Win32(0),
        Err(e) => {
            tracing::error!("Dienst beendet mit Fehler: {e:#}");
            event_log_error(&format!("{e:#}"));
            ServiceExitCode::ServiceSpecific(EXIT_RUNTIME)
        }
    };
    drop(runtime);
    status(handle, ServiceState::Stopped, exit, Duration::ZERO);
    Ok(())
}
