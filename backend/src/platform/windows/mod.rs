//! Windows: Pfadhilfen, Ereignisprotokoll, Dienst (`service`) und Einrichtung (`setup`).

pub mod service;
pub mod setup;

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::{Component, Path, Prefix};

use windows_sys::Win32::Storage::FileSystem::{GetDriveTypeW, GetVolumePathNameW};
use windows_sys::Win32::System::EventLog::{DeregisterEventSource, EVENTLOG_ERROR_TYPE, RegisterEventSourceW, ReportEventW};

/// Dienstname und Ereignisquelle, eingefroren (RFC-0021).
pub const SERVICE_NAME: &str = "TrackCraft";

const DRIVE_REMOTE: u32 = 4;

fn wide(s: &OsStr) -> Vec<u16> {
    s.encode_wide().chain(std::iter::once(0)).collect()
}

pub fn is_unc(path: &Path) -> bool {
    matches!(path.components().next(), Some(Component::Prefix(p)) if matches!(p.kind(), Prefix::UNC(..) | Prefix::VerbatimUNC(..)))
}

pub fn is_mapped_network_drive(path: &Path) -> bool {
    let letter = match path.components().next() {
        Some(Component::Prefix(p)) => match p.kind() {
            Prefix::Disk(l) | Prefix::VerbatimDisk(l) => l,
            _ => return false,
        },
        _ => return false,
    };
    let root = format!("{}:\\", letter as char);
    let w = wide(OsStr::new(&root));
    // SAFETY: nullterminierter UTF-16-String, der während des Aufrufs lebt.
    unsafe { GetDriveTypeW(w.as_ptr()) == DRIVE_REMOTE }
}

/// Wurzel des Volumes eines vorhandenen Pfads, z. B. `C:\` oder `\\server\freigabe\`, in Kleinbuchstaben.
pub fn volume_root(path: &Path) -> std::io::Result<String> {
    let w = wide(path.as_os_str());
    let mut buf = vec![0u16; 1024];
    // SAFETY: Eingabe nullterminiert, Puffergröße passend übergeben.
    let ok = unsafe { GetVolumePathNameW(w.as_ptr(), buf.as_mut_ptr(), buf.len() as u32) };
    if ok == 0 {
        return Err(std::io::Error::last_os_error());
    }
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    Ok(String::from_utf16_lossy(&buf[..len]).to_lowercase())
}

/// Schreibt einen Fehler ins Ereignisprotokoll (Anwendung, Quelle `TrackCraft`). Fehler hier werden ignoriert.
pub fn event_log_error(message: &str) {
    let source = wide(OsStr::new(SERVICE_NAME));
    let text = wide(OsStr::new(message));
    // SAFETY: Alle Zeiger zeigen auf nullterminierte Puffer, die bis zum Ende des Blocks leben.
    unsafe {
        let handle = RegisterEventSourceW(std::ptr::null(), source.as_ptr());
        if handle.is_null() {
            return;
        }
        let strings = [text.as_ptr()];
        ReportEventW(handle, EVENTLOG_ERROR_TYPE, 0, 1, std::ptr::null_mut(), 1, 0, strings.as_ptr(), std::ptr::null());
        DeregisterEventSource(handle);
    }
}
