//! Logging nach RFC-0013: eine Datei `app.<datum>.log` in `logs_dir`, täglich rotiert, die 30 neuesten
//! bleiben; in Konsole und Container zusätzlich stdout.

/// Platzhalter für Geheimnisse im `Debug`-Format (ADR-0013): Strukturen mit Schlüssel oder Token
/// leiten `Debug` nicht ab, sondern zeigen an deren Stelle nur diesen Text.
pub const REDACTED: &str = "<ausgeblendet>";

/// Steht im `Debug`-Format für ein Geheimnis; `Option<Hidden>` zeigt noch, ob eines gesetzt ist.
pub struct Hidden;

impl std::fmt::Debug for Hidden {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(REDACTED)
    }
}

/// `Some(<ausgeblendet>)` oder `None`.
pub fn hidden<T>(value: &Option<T>) -> Option<Hidden> {
    value.as_ref().map(|_| Hidden)
}

use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{Builder, Rotation};
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::fmt;
use tracing_subscriber::prelude::*;

use crate::config::Validated;

pub fn init(cfg: &Validated, to_stdout: bool) -> anyhow::Result<WorkerGuard> {
    std::fs::create_dir_all(&cfg.raw.paths.logs_dir)?;
    let appender = Builder::new()
        .rotation(Rotation::DAILY)
        .filename_prefix("app")
        .filename_suffix("log")
        .max_log_files(30)
        .build(&cfg.raw.paths.logs_dir)?;
    let (writer, guard) = tracing_appender::non_blocking(appender);
    let level = LevelFilter::from_level(cfg.log_level);

    let file_layer = fmt::layer().with_writer(writer).with_ansi(false).with_target(true);
    let ansi = std::io::IsTerminal::is_terminal(&std::io::stdout());
    let stdout_layer = to_stdout.then(|| fmt::layer().with_target(false).with_ansi(ansi));
    tracing_subscriber::registry().with(level).with(file_layer).with(stdout_layer).try_init()?;
    Ok(guard)
}

/// Minimales Logging auf stderr, bevor die Konfiguration gelesen ist.
pub fn init_stderr() {
    let _ = tracing_subscriber::fmt().with_writer(std::io::stderr).with_max_level(tracing::Level::INFO).try_init();
}
