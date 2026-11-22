//! Audit-Protokoll (C11, RFC-0011): Tabelle `audit_log`, eine Zeile je Ereignis, Aufbewahrung ein Jahr.
//! Geschrieben wird in derselben Transaktion wie die Änderung, damit Protokoll und Daten übereinstimmen.

use serde_json::Value;
use sqlx::SqliteConnection;

use crate::db::new_id;

/// Aufbewahrung der Audit-Einträge (C11).
pub const RETENTION_MS: i64 = 365 * 24 * 3600 * 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Import,
    Replace,
    Delete,
    Backup,
    Restore,
}

impl Action {
    fn as_str(self) -> &'static str {
        match self {
            Self::Import => "import",
            Self::Replace => "replace",
            Self::Delete => "delete",
            Self::Backup => "backup",
            Self::Restore => "restore",
        }
    }
}

pub async fn record(
    conn: &mut SqliteConnection,
    action: Action,
    object_type: &str,
    object_id: Option<&str>,
    ok: bool,
    details: Value,
) -> sqlx::Result<()> {
    sqlx::query("INSERT INTO audit_log (id, at_ms, action, object_type, object_id, outcome, details) VALUES (?, ?, ?, ?, ?, ?, ?)")
        .bind(new_id())
        .bind(chrono::Utc::now().timestamp_millis())
        .bind(action.as_str())
        .bind(object_type)
        .bind(object_id)
        .bind(if ok { "ok" } else { "failed" })
        .bind(details.to_string())
        .execute(conn)
        .await?;
    Ok(())
}
