//! Startreihenfolge, Migration und Profilabgleich (RFC-0015 C5, RFC-0001, RFC-0014 Fälle 30 und 32).

mod common;

use trackcraft::config;
use trackcraft::db::{Db, SCHEMA_VERSION};
use trackcraft::startup;

#[tokio::test]
async fn fresh_start_creates_schema_user_and_data_state() {
    let dir = common::temp();
    let cfg = common::validated(dir.path());
    let started = startup::prepare(&cfg).await.unwrap();
    assert_eq!(started.schema_version, SCHEMA_VERSION);

    let name: String = sqlx::query_scalar("SELECT display_name FROM users").fetch_one(&started.db.reader).await.unwrap();
    assert_eq!(name, "Testperson");
    let counter: i64 = sqlx::query_scalar("SELECT change_counter FROM data_state").fetch_one(&started.db.reader).await.unwrap();
    assert_eq!(counter, 0);
    let mode: String = sqlx::query_scalar("PRAGMA journal_mode").fetch_one(&started.db.writer).await.unwrap();
    assert_eq!(mode, "wal");
    let fk: i64 = sqlx::query_scalar("PRAGMA foreign_keys").fetch_one(&started.db.writer).await.unwrap();
    assert_eq!(fk, 1);

    for d in startup::data_dirs(&cfg) {
        assert!(d.is_dir(), "{} fehlt", d.display());
    }
    // Eine leere neue Datenbank braucht keine Sicherung vor Migration.
    assert_eq!(std::fs::read_dir(&cfg.raw.backup.migration_dir).unwrap().count(), 0);
    started.db.close().await;
}

#[tokio::test]
async fn second_instance_on_same_data_is_refused() {
    let dir = common::temp();
    let cfg = common::validated(dir.path());
    let first = startup::prepare(&cfg).await.unwrap();
    let err = startup::prepare(&cfg).await.err().expect("zweiter Start muss scheitern");
    assert!(err.to_string().contains("läuft bereits"), "{err:#}");
    first.db.close().await;
    drop(first);
    let again = startup::prepare(&cfg).await.unwrap();
    again.db.close().await;
}

#[tokio::test]
async fn newer_schema_is_refused_without_change() {
    let dir = common::temp();
    let cfg = common::validated(dir.path());
    let started = startup::prepare(&cfg).await.unwrap();
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("PRAGMA user_version = {}", SCHEMA_VERSION + 1))).execute(&started.db.writer).await.unwrap();
    started.db.close().await;
    drop(started);

    let err = startup::prepare(&cfg).await.err().expect("neueres Schema muss abbrechen");
    assert!(err.to_string().contains("älteres Programm"), "{err:#}");
    let db = Db::open(&cfg.database_file()).await.unwrap();
    assert_eq!(db.schema_version().await.unwrap(), SCHEMA_VERSION + 1);
    db.close().await;
}

#[tokio::test]
async fn toml_display_name_only_seeds_the_first_user() {
    let dir = common::temp();
    let cfg = common::validated(dir.path());
    let started = startup::prepare(&cfg).await.unwrap();
    let rev1: String = sqlx::query_scalar("SELECT revision_id FROM data_state").fetch_one(&started.db.reader).await.unwrap();
    started.db.close().await;
    drop(started);

    // Unveränderter Abgleich ändert nichts.
    let started = startup::prepare(&cfg).await.unwrap();
    let (rev, counter): (String, i64) =
        sqlx::query_as("SELECT revision_id, change_counter FROM data_state").fetch_one(&started.db.reader).await.unwrap();
    assert_eq!((rev.as_str(), counter), (rev1.as_str(), 0));
    started.db.close().await;
    drop(started);

    // Seit Schemaversion 10 führt die Datenbank: ein geänderter TOML-Name wird ignoriert.
    let text = common::config_text(dir.path(), "", "").replace("Testperson", "Neuer Name");
    let cfg2 = config::parse(&dir.path().join("t.toml"), &text).unwrap();
    let started = startup::prepare(&cfg2).await.unwrap();
    let (name, rev, counter): (String, String, i64) =
        sqlx::query_as("SELECT u.display_name, d.revision_id, d.change_counter FROM users u, data_state d")
            .fetch_one(&started.db.reader)
            .await
            .unwrap();
    assert_eq!((name.as_str(), rev.as_str(), counter), ("Testperson", rev1.as_str(), 0));
    started.db.close().await;
}

#[tokio::test]
async fn without_user_section_the_first_user_is_named_default() {
    let dir = common::temp();
    let text = common::config_text(dir.path(), "", "").replace("[user]\ndisplay_name = \"Testperson\"\n", "");
    assert!(!text.contains("[user]"));
    let cfg = config::parse(&dir.path().join("t.toml"), &text).unwrap();
    let started = startup::prepare(&cfg).await.unwrap();
    let name: String = sqlx::query_scalar("SELECT display_name FROM users").fetch_one(&started.db.reader).await.unwrap();
    assert_eq!(name, trackcraft::db::DEFAULT_NICKNAME);
    started.db.close().await;
}

#[tokio::test]
async fn staging_leftovers_are_removed_on_start() {
    let dir = common::temp();
    let cfg = common::validated(dir.path());
    let stale = cfg.staging_dir().join("0190-unbekannt");
    std::fs::create_dir_all(&stale).unwrap();
    std::fs::write(stale.join("teil.fit"), b"x").unwrap();
    let started = startup::prepare(&cfg).await.unwrap();
    assert!(!stale.exists());
    assert!(cfg.staging_dir().is_dir());
    started.db.close().await;
}

#[tokio::test]
async fn migration_on_existing_data_makes_local_backup_first() {
    let dir = common::temp();
    let cfg = common::validated(dir.path());
    // Datenbank mit Inhalt, aber Schemaversion 0: die anstehende Migration muss vorher sichern.
    std::fs::create_dir_all(&cfg.raw.paths.database_dir).unwrap();
    let db = Db::open(&cfg.database_file()).await.unwrap();
    sqlx::raw_sql("CREATE TABLE altbestand (x INTEGER)").execute(&db.writer).await.unwrap();
    db.close().await;

    // Ältere Reste im Migrationsordner (alte Einzeldatei, älterer Ordner) werden ersetzt, nur die letzte bleibt.
    let mig = cfg.raw.backup.migration_dir.clone();
    std::fs::create_dir_all(mig.join("trackcraft-vor-migration-v0-v1")).unwrap();
    std::fs::write(mig.join("trackcraft-vor-migration-v0-v0.db"), b"alt").unwrap();
    // Das Sicherungsziel ist erreichbar: die Sicherung wird zusätzlich dorthin kopiert.
    std::fs::create_dir_all(&cfg.raw.backup.target_dir).unwrap();

    let started = startup::prepare(&cfg).await.unwrap();
    let files: Vec<String> = std::fs::read_dir(&mig).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().to_string()).collect();
    assert_eq!(files, vec![format!("trackcraft-vor-migration-v0-v{SCHEMA_VERSION}")]);
    started.db.close().await;

    let local = mig.join(&files[0]);
    let m = trackcraft::backup::format::read_manifest(&local).unwrap();
    assert_eq!(m.trigger, "migration");
    assert_eq!(m.schema_version, 0);
    trackcraft::backup::format::verify(&local, &m).unwrap();
    let backup = Db::open(&local.join("trackcraft.db")).await.unwrap();
    let n: i64 =
        sqlx::query_scalar("SELECT count(*) FROM sqlite_schema WHERE name = 'altbestand'").fetch_one(&backup.reader).await.unwrap();
    assert_eq!(n, 1);
    assert_eq!(backup.schema_version().await.unwrap(), 0);
    backup.close().await;

    let stands = trackcraft::backup::format::list(&cfg.raw.backup.target_dir).unwrap();
    assert_eq!(stands.len(), 1);
    assert_eq!(stands[0].manifest.as_ref().unwrap().schema_version, 0);
    assert!(stands[0].problem.is_none());
}

/// Migration 6 legt `activities` und `segments` neu an. Abhängige Zeilen (Track, Versuche, Jobs) dürfen
/// dabei nicht über `ON DELETE CASCADE` verschwinden, und die neuen Sportklassen sind danach erlaubt.
#[tokio::test]
async fn migration_6_keeps_dependent_rows_and_allows_new_sport_classes() {
    use trackcraft::db::MIGRATIONS;
    let dir = common::temp();
    let db = Db::open(&dir.path().join("t.db")).await.unwrap();
    let (old, new): (Vec<_>, Vec<_>) = MIGRATIONS.iter().copied().partition(|(v, _)| *v <= 5);
    db.migrate(&old).await.unwrap();
    sqlx::raw_sql(
        "INSERT INTO users VALUES ('u', 'Uli', 0, 0);
         INSERT INTO original_files VALUES ('f', 'fit', 'a.fit', '0000000000000000000000000000000000000000000000000000000000000000', 1, 0);
         INSERT INTO activities (id, user_id, original_file_id, title, sport_class, start_time_ms, start_time_origin,
             total_time_ms, total_time_origin, created_at_ms, updated_at_ms, has_position)
             VALUES ('a', 'u', 'f', 'Runde', 'cycling', 0, 'file', 1000, 'file', 0, 0, 1);
         INSERT INTO track_sections VALUES ('t', 'a', 0, 1, 1, 0, 0, x'00');
         INSERT INTO segments (id, creator_id, name, sport_class, length_m, start_lat, start_lng, end_lat, end_lng,
             reference_format_version, reference_point_count, reference, created_at_ms, updated_at_ms, location)
             VALUES ('s', 'u', 'Anstieg', 'cycling', 100, 0, 0, 0, 0, 1, 2, x'00', 0, 0, 'Ort');
         INSERT INTO calculation_versions VALUES ('c', 1, 'h', 'published', NULL, 0, 0);
         INSERT INTO activity_calculations VALUES ('c', 'a', 12.0);
         INSERT INTO segment_efforts (id, calculation_version_id, segment_id, activity_id, section_id, attempt_position,
             status, start_time_ms, end_time_ms, duration_ms, created_at_ms)
             VALUES ('e', 'c', 's', 'a', 't', 0, 'valid', 0, 500, 500, 0);
         INSERT INTO jobs (id, kind, segment_id, activity_id, created_at_ms) VALUES ('j', 'segment_history', 's', 'a', 0);",
    )
    .execute(&db.writer)
    .await
    .unwrap();

    db.migrate(&new).await.unwrap();
    assert_eq!(db.schema_version().await.unwrap(), SCHEMA_VERSION);
    let fk: i64 = sqlx::query_scalar("PRAGMA foreign_keys").fetch_one(&db.writer).await.unwrap();
    assert_eq!(fk, 1, "Fremdschlüssel nach der Migration wieder an");
    for (table, n) in
        [("activities", 1), ("track_sections", 1), ("segments", 1), ("activity_calculations", 1), ("segment_efforts", 1), ("jobs", 1)]
    {
        let c: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT count(*) FROM {table}"))).fetch_one(&db.reader).await.unwrap();
        assert_eq!(c, n, "{table}");
    }
    let loc: String = sqlx::query_scalar("SELECT location FROM segments").fetch_one(&db.reader).await.unwrap();
    assert_eq!(loc, "Ort");

    for class in ["skiing", "swimming", "studio"] {
        sqlx::query("UPDATE activities SET sport_class = ?").bind(class).execute(&db.writer).await.unwrap();
    }
    sqlx::query("UPDATE segments SET sport_class = 'skiing'").execute(&db.writer).await.unwrap();
    assert!(sqlx::query("UPDATE segments SET sport_class = 'swimming'").execute(&db.writer).await.is_err());
    // Fremdschlüssel greifen weiter: Löschen der Aktivität nimmt Track und Versuche mit.
    sqlx::query("DELETE FROM activities").execute(&db.writer).await.unwrap();
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM track_sections").fetch_one(&db.reader).await.unwrap();
    assert_eq!(left, 0);
    db.close().await;
}
