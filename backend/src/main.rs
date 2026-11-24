//! `trackcraft.exe` bzw. `trackcraft`: Befehle nach RFC-0005 (Windows, Linux mit systemd) und Containerstart.

use std::process::ExitCode;

use clap::{Parser, Subcommand};
use trackcraft::{commands, config, logging, platform, server, version};

#[derive(Parser)]
#[command(name = "trackcraft", version = version::APP_VERSION,
    about = "TrackCraft: FIT-/GPX-Trainingsdaten im Heimnetz. Ohne Befehl: Start im Vordergrund.")]
struct Cli {
    #[command(subcommand)]
    command: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Konfiguration prüfen, nichts starten und nichts schreiben
    CheckConfig,
    /// Installieren oder aktualisieren (Windows: Administrator, bequemer über setup.cmd; Linux: root, setup.sh)
    #[cfg(any(windows, target_os = "linux"))]
    Setup {
        /// Installationsordner (Vorgabe: C:\Program Files\TrackCraft bzw. /opt/trackcraft oder der bisherige)
        #[arg(long)]
        install_dir: Option<std::path::PathBuf>,
        /// Namen im Heimnetz, durch Komma getrennt, z. B. pc.home.arpa,localhost
        #[arg(long = "names", value_delimiter = ',')]
        names: Vec<String>,
        /// Anzeigename
        #[arg(long)]
        display_name: Option<String>,
        /// Sicherungsziel (Ordner, Laufwerk oder \\server\freigabe\...)
        #[arg(long)]
        backup_dir: Option<std::path::PathBuf>,
        /// HTTPS-Port
        #[arg(long)]
        port: Option<u16>,
        /// Fehlende Werte abfragen (setup.cmd setzt das)
        #[arg(long)]
        interactive: bool,
    },
    /// HTTPS-Zertifikat erneuern bzw. für geänderte Namen ausstellen (Administrator bzw. root)
    #[cfg(any(windows, target_os = "linux"))]
    Cert {
        /// Neue TrackCraft-CA anlegen (nötig für Namen außerhalb der bisherigen; danach Handy neu einrichten)
        #[arg(long)]
        new_ca: bool,
        /// Namen im Heimnetz neu festlegen (ersetzt allowed_origins), z. B. localhost,trackcraft.home.arpa
        #[arg(long = "names", value_delimiter = ',')]
        names: Vec<String>,
    },
    /// Demo mit erfundenen Daten (scripts/demo)
    Demo {
        #[command(subcommand)]
        action: DemoAction,
    },
    /// Windows: Dienst, Ereignisquelle, Firewallregel und CA-Vertrauen entfernen; Daten bleiben (Administrator)
    #[cfg(windows)]
    Uninstall,
    /// Windows: Dienst steuern
    #[cfg(windows)]
    Service {
        #[command(subcommand)]
        action: ServiceAction,
    },
    /// Einstieg für den Dienststeuerungs-Manager
    #[cfg(windows)]
    #[command(hide = true)]
    ServiceRun,
}

#[derive(Subcommand)]
enum DemoAction {
    /// Datenbank der Konfiguration als Demo-Datenbank kennzeichnen (nur dann verschiebt [demo] enabled die Daten)
    Mark,
    /// Demo zum Ausprobieren starten: Daten, Zertifikat und Konfiguration in einem eigenen Ordner, ohne Installation
    Start {
        /// Ordner der Demo (Vorgabe: %LOCALAPPDATA%\TrackCraft-Demo bzw. ~/.local/share/trackcraft-demo)
        #[arg(long)]
        dir: Option<std::path::PathBuf>,
        /// Entpackte Demo-Datenbank (Vorgabe: Ordner demo-daten neben dem Programm)
        #[arg(long)]
        source: Option<std::path::PathBuf>,
        /// HTTPS-Port auf 127.0.0.1
        #[arg(long, default_value_t = 8443)]
        port: u16,
        /// Mit frischen Demo-Daten beginnen (bisherige Änderungen in der Demo gehen verloren)
        #[arg(long)]
        reset: bool,
        /// Browser nicht öffnen
        #[arg(long)]
        no_browser: bool,
    },
}

#[cfg(windows)]
#[derive(Subcommand)]
enum ServiceAction {
    Start,
    Stop,
    Status,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        None => run_foreground(),
        Some(Cmd::CheckConfig) => {
            return if commands::check_config_report(&config::config_path()) { ExitCode::SUCCESS } else { ExitCode::from(1) };
        }
        #[cfg(windows)]
        Some(Cmd::Setup { install_dir, names, display_name, backup_dir, port, interactive }) => {
            platform::windows::setup::setup(platform::windows::setup::SetupArgs {
                install_dir,
                names,
                display_name,
                backup_dir,
                port,
                interactive,
            })
        }
        #[cfg(target_os = "linux")]
        Some(Cmd::Setup { install_dir, names, display_name, backup_dir, port, interactive }) => {
            platform::linux::setup::setup(platform::linux::setup::SetupArgs {
                install_dir,
                names,
                display_name,
                backup_dir,
                port,
                interactive,
            })
        }
        #[cfg(windows)]
        Some(Cmd::Cert { new_ca, names }) => platform::windows::setup::cert_command(new_ca, names),
        #[cfg(target_os = "linux")]
        Some(Cmd::Cert { new_ca, names }) => platform::linux::setup::cert_command(new_ca, names),
        Some(Cmd::Demo { action: DemoAction::Mark }) => commands::demo_mark(&config::config_path()),
        Some(Cmd::Demo { action: DemoAction::Start { dir, source, port, reset, no_browser } }) => {
            demo_workspace(dir, source, port).and_then(|ws| commands::demo_start(ws, reset, !no_browser))
        }
        #[cfg(windows)]
        Some(Cmd::Uninstall) => platform::windows::setup::uninstall(),
        #[cfg(windows)]
        Some(Cmd::Service { action }) => {
            use platform::windows::setup::{ServiceCommand, service_command};
            service_command(match action {
                ServiceAction::Start => ServiceCommand::Start,
                ServiceAction::Stop => ServiceCommand::Stop,
                ServiceAction::Status => ServiceCommand::Status,
            })
        }
        #[cfg(windows)]
        Some(Cmd::ServiceRun) => platform::windows::service::run_dispatcher(),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Fehler: {e:#}");
            ExitCode::from(1)
        }
    }
}

fn demo_workspace(
    dir: Option<std::path::PathBuf>,
    source: Option<std::path::PathBuf>,
    port: u16,
) -> anyhow::Result<trackcraft::demo::Workspace> {
    use anyhow::Context;
    let exe_dir = std::env::current_exe()?.parent().context("Programmordner unbekannt")?.to_path_buf();
    let dir = match dir {
        Some(d) => d,
        None => {
            #[cfg(windows)]
            let base = std::env::var_os("LOCALAPPDATA").map(std::path::PathBuf::from).context("LOCALAPPDATA ist nicht gesetzt")?;
            #[cfg(not(windows))]
            let base =
                std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".local/share")).context("HOME ist nicht gesetzt")?;
            base.join(if cfg!(windows) { "TrackCraft-Demo" } else { "trackcraft-demo" })
        }
    };
    Ok(trackcraft::demo::Workspace {
        dir: std::path::absolute(dir)?,
        source: std::path::absolute(source.unwrap_or_else(|| exe_dir.join("demo-daten")))?,
        web: exe_dir.join("web"),
        port,
    })
}

/// Konsole (Windows-Diagnose) und Container: gleiche Konfiguration, gleiche Prüfungen.
fn run_foreground() -> anyhow::Result<()> {
    let path = config::config_path();
    let cfg = match config::load(&path) {
        Ok(c) => c,
        Err(e) => {
            platform::report_startup_error(&e.to_string());
            anyhow::bail!("Start abgebrochen");
        }
    };
    let _guard = logging::init(&cfg, true).inspect_err(|_| logging::init_stderr())?;
    tracing::info!(version = %version::long(), config = %path.display(), "TrackCraft startet");
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
    runtime.block_on(server::run(cfg, server::os_shutdown_signal(), |addr| {
        tracing::info!(%addr, "bereit, Beenden mit Strg+C");
    }))
}
