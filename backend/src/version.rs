//! Beim Build eingebettete Version (RFC-0015).

/// Version aus dem Release-Tag (CI setzt `TRACKCRAFT_VERSION`), sonst aus Cargo.toml.
pub const APP_VERSION: &str = match option_env!("TRACKCRAFT_VERSION") {
    Some(v) => v,
    None => env!("CARGO_PKG_VERSION"),
};

/// Commit des Builds, `unbekannt` außerhalb eines Git-Checkouts.
pub const COMMIT: &str = env!("TRACKCRAFT_COMMIT");

/// Builddatum (UTC, `YYYY-MM-DD`).
pub const BUILD_DATE: &str = env!("TRACKCRAFT_BUILD_DATE");

pub fn long() -> String {
    format!("{APP_VERSION} ({COMMIT}, {BUILD_DATE})")
}
