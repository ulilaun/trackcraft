//! Plattformabhängiges (RFC-0004): Pfadprüfungen, unter Windows Dienst, Einrichtung und Ereignisprotokoll,
//! unter Linux Einrichtung mit systemd.
//! Außerhalb dieses Moduls gibt es keine Plattformzweige in der Fachlogik.

use std::path::Path;

#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(windows)]
pub mod windows;

/// Liegt der Pfad auf einer Netzwerkfreigabe (UNC oder verbundenes Netzlaufwerk)?
pub fn is_network_path(path: &Path) -> bool {
    #[cfg(windows)]
    {
        windows::is_unc(path) || windows::is_mapped_network_drive(path)
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        false
    }
}

/// Verbundener Laufwerksbuchstabe einer Netzwerkfreigabe (für Dienste unsichtbar).
pub fn is_mapped_network_drive(path: &Path) -> bool {
    #[cfg(windows)]
    {
        windows::is_mapped_network_drive(path)
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        false
    }
}

/// Liegen beide (vorhandenen) Ordner auf demselben Laufwerk bzw. Dateisystem? (ADR-0011)
pub fn same_volume(a: &Path, b: &Path) -> std::io::Result<bool> {
    #[cfg(windows)]
    {
        Ok(windows::volume_root(a)? == windows::volume_root(b)?)
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Ok(std::fs::metadata(a)?.dev() == std::fs::metadata(b)?.dev())
    }
}

/// Startfehler, die vor den Logdateien auftreten (RFC-0013). Außerhalb von Windows nur stderr.
pub fn report_startup_error(message: &str) {
    eprintln!("TrackCraft: {message}");
    #[cfg(windows)]
    windows::event_log_error(message);
}
