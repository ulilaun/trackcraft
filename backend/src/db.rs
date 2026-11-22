//! SQLite-Betrieb nach C5 (RFC-0001): WAL, ein Schreiber, mehrere Leser, Schemaversion in `user_version`.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, bail};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions, SqliteSynchronous};
use sqlx::{Connection, Row};

/// Migrationen in Reihenfolge; die Nummer ist die Schemaversion nach der Migration.
pub const MIGRATIONS: &[(i64, &str)] = &[
    (1, include_str!("../migrations/0001_initial.sql")),
    (2, include_str!("../migrations/0002_import_audit.sql")),
    (3, include_str!("../migrations/0003_activity_view.sql")),
    (4, include_str!("../migrations/0004_segment_category.sql")),
    (5, include_str!("../migrations/0005_seasons.sql")),
    (6, include_str!("../migrations/0006_sport_classes.sql")),
    (7, include_str!("../migrations/0007_sport_mappings.sql")),
    (8, include_str!("../migrations/0008_garmin.sql")),
    (9, include_str!("../migrations/0009_sub_sport_icons.sql")),
    (10, include_str!("../migrations/0010_user_profile.sql")),
    (11, include_str!("../migrations/0011_training_load.sql")),
    (12, include_str!("../migrations/0012_form_models.sql")),
    (13, include_str!("../migrations/0013_places.sql")),
];

/// Schemaversion dieses Programms.
pub const SCHEMA_VERSION: i64 = MIGRATIONS[MIGRATIONS.len() - 1].0;

#[derive(Clone)]
pub struct Db {
    /// Genau eine schreibende Verbindung; ändernde Transaktionen laufen nacheinander.
    pub writer: SqlitePool,
    /// Lesende Verbindungen.
    pub reader: SqlitePool,
}

fn options(file: &Path) -> SqliteConnectOptions {
    SqliteConnectOptions::new()
        .filename(file)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Full)
        .busy_timeout(Duration::from_millis(5000))
        .foreign_keys(true)
}

impl Db {
    pub async fn open(file: &Path) -> anyhow::Result<Self> {
        let writer = SqlitePoolOptions::new()
            .max_connections(1)
            .min_connections(1)
            .connect_with(options(file))
            .await
            .with_context(|| format!("Datenbank {} lässt sich nicht öffnen", file.display()))?;
        let reader = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(options(file).read_only(true))
            .await
            .with_context(|| format!("Datenbank {} lässt sich nicht lesend öffnen", file.display()))?;
        Ok(Self { writer, reader })
    }

    pub async fn close(&self) {
        self.reader.close().await;
        self.writer.close().await;
    }

    pub async fn schema_version(&self) -> anyhow::Result<i64> {
        let v: i64 = sqlx::query_scalar("PRAGMA user_version").fetch_one(&self.writer).await?;
        Ok(v)
    }

    /// Enthält die Datenbank schon Daten? (Entscheidet über die Sicherung vor Migration.)
    pub async fn has_data(&self) -> anyhow::Result<bool> {
        let tables: i64 = sqlx::query_scalar("SELECT count(*) FROM sqlite_schema WHERE type = 'table' AND name NOT LIKE 'sqlite_%'")
            .fetch_one(&self.writer)
            .await?;
        Ok(tables > 0)
    }

    /// Prüft die Schemaversion; neuer als bekannt bricht ohne Änderung ab (RFC-0015).
    pub async fn pending_migrations(&self) -> anyhow::Result<Vec<(i64, &'static str)>> {
        let current = self.schema_version().await?;
        if current > SCHEMA_VERSION {
            bail!(
                "Die Datenbank hat Schemaversion {current}, dieses Programm kennt nur bis {SCHEMA_VERSION}. \
                 Ein älteres Programm startet nicht gegen ein neueres Schema (RFC-0015); es wurde nichts verändert."
            );
        }
        Ok(MIGRATIONS.iter().copied().filter(|(v, _)| *v > current).collect())
    }

    /// Führt jede Migration in einer eigenen Transaktion aus und setzt `user_version`.
    ///
    /// Fremdschlüssel sind während der Migration aus, damit eine Tabelle neu angelegt werden kann, ohne dass
    /// `DROP TABLE` abhängige Zeilen löscht (SQLite-Doku „Making Other Kinds Of Table Schema Changes“). Vor dem
    /// COMMIT prüft `foreign_key_check`, dass alle Verweise noch stimmen; sonst wird zurückgenommen.
    pub async fn migrate(&self, pending: &[(i64, &'static str)]) -> anyhow::Result<()> {
        let mut conn = self.writer.acquire().await?;
        sqlx::raw_sql("PRAGMA foreign_keys = OFF").execute(&mut *conn).await?;
        let result = Self::migrate_steps(&mut conn, pending).await;
        sqlx::raw_sql("PRAGMA foreign_keys = ON").execute(&mut *conn).await?;
        result
    }

    async fn migrate_steps(conn: &mut sqlx::SqliteConnection, pending: &[(i64, &'static str)]) -> anyhow::Result<()> {
        for (version, sql) in pending {
            tracing::info!(version, "Migration läuft");
            sqlx::raw_sql("BEGIN IMMEDIATE").execute(&mut *conn).await?;
            let result = async {
                sqlx::raw_sql(*sql).execute(&mut *conn).await?;
                let broken: i64 = sqlx::query_scalar("SELECT count(*) FROM pragma_foreign_key_check").fetch_one(&mut *conn).await?;
                if broken > 0 {
                    return Err(sqlx::Error::Protocol(format!("{broken} verletzte Fremdschlüssel nach der Migration")));
                }
                // Eine Ganzzahl aus der festen Migrationsliste, kein Benutzerwert.
                sqlx::raw_sql(sqlx::AssertSqlSafe(format!("PRAGMA user_version = {version}"))).execute(&mut *conn).await?;
                sqlx::raw_sql("COMMIT").execute(&mut *conn).await?;
                Ok::<_, sqlx::Error>(())
            }
            .await;
            if let Err(e) = result {
                let _ = sqlx::raw_sql("ROLLBACK").execute(&mut *conn).await;
                return Err(e).with_context(|| format!("Migration {version} fehlgeschlagen, zurückgenommen"));
            }
        }
        Ok(())
    }

    /// Konsistente Kopie der Datenbank per `VACUUM INTO` (nie als Dateikopie, RFC-0001).
    pub async fn vacuum_into(&self, target: &Path) -> anyhow::Result<()> {
        sqlx::query("VACUUM INTO ?").bind(target.to_string_lossy().to_string()).execute(&self.writer).await?;
        Ok(())
    }

    /// Profilabgleich und Anlegen von `data_state` beim ersten Start (Schritt 7, RFC-0015).
    /// Seit Schemaversion 10 führt die Datenbank: `[user] display_name` legt nur den ersten Benutzer an (sonst „Ich“),
    /// ein abweichender TOML-Wert wird danach nur gemeldet, nie übernommen.
    pub async fn sync_profile(&self, display_name: Option<&str>) -> anyhow::Result<ProfileSync> {
        let now = chrono::Utc::now().timestamp_millis();
        let mut conn = self.writer.acquire().await?;
        let mut tx = conn.begin().await?;

        let state = sqlx::query("SELECT revision_id FROM data_state WHERE id = 1").fetch_optional(&mut *tx).await?;
        if state.is_none() {
            sqlx::query("INSERT INTO data_state (id, dataset_id, revision_id, predecessor_revision_id, change_counter, updated_at_ms) VALUES (1, ?, ?, NULL, 0, ?)")
                .bind(new_id())
                .bind(new_id())
                .bind(now)
                .execute(&mut *tx)
                .await?;
        }

        let users = sqlx::query("SELECT id, display_name FROM users").fetch_all(&mut *tx).await?;
        let result = match users.as_slice() {
            [] => {
                sqlx::query("INSERT INTO users (id, display_name, created_at_ms, updated_at_ms) VALUES (?, ?, ?, ?)")
                    .bind(new_id())
                    .bind(display_name.unwrap_or(DEFAULT_NICKNAME))
                    .bind(now)
                    .bind(now)
                    .execute(&mut *tx)
                    .await?;
                ProfileSync::Created
            }
            [user] => {
                let current: String = user.get("display_name");
                if display_name.is_some_and(|d| d != current) { ProfileSync::TomlIgnored } else { ProfileSync::Unchanged }
            }
            _ => bail!("Die Datenbank enthält mehr als einen Benutzer; V1 kennt genau einen (ADR-0001)."),
        };
        tx.commit().await?;
        Ok(result)
    }

    pub async fn ping(&self) -> anyhow::Result<()> {
        sqlx::query("SELECT 1").execute(&self.reader).await?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileSync {
    Created,
    Unchanged,
    /// `[user] display_name` weicht vom Spitznamen in der Datenbank ab und wird ignoriert.
    TomlIgnored,
}

/// Spitzname des ersten Benutzers ohne `[user]` in der TOML.
pub const DEFAULT_NICKNAME: &str = "Ich";

/// Sicherungsrelevante Änderung: neue Revision des Datenstands, Zähler erhöhen (ADR-0014, RFC-0012).
pub async fn bump_revision(conn: &mut sqlx::SqliteConnection, now_ms: i64) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE data_state SET predecessor_revision_id = revision_id, revision_id = ?, \
         change_counter = change_counter + 1, updated_at_ms = ? WHERE id = 1",
    )
    .bind(new_id())
    .bind(now_ms)
    .execute(conn)
    .await?;
    Ok(())
}

/// UUIDv7 als Text (RFC-0001).
pub fn new_id() -> String {
    uuid::Uuid::now_v7().to_string()
}

/// Pfad der lokalen Sicherung vor Migration; es bleibt nur die letzte (RFC-0012).
pub fn migration_backup_path(dir: &Path, from: i64, to: i64) -> PathBuf {
    dir.join(format!("trackcraft-vor-migration-v{from}-v{to}.db"))
}

/// Prüft, dass ein Dateiname nur aus erwarteten Zeichen besteht (für Aufräumen im Migrationsordner).
pub fn is_migration_backup(name: &str) -> bool {
    name.starts_with("trackcraft-vor-migration-") && name.ends_with(".db")
}
