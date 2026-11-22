//! Linux mit systemd (RFC-0005 „Linux“): Einrichtung (`setup`) und Zertifikat (`cert`).
//! Der Dienst selbst ist der normale Start im Vordergrund unter systemd.

pub mod setup;

/// Name der systemd-Unit und des Dienstbenutzers.
pub const SERVICE_NAME: &str = "trackcraft";
