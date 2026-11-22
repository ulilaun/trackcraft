# RFC-0001: Datenmodell und Benutzer

Status: Neufassung, Piggy 011 (Entwurf zur Freigabe), ersetzt die Fassung Piggy 009. Das Schema ist SQLite-konform (STRICT-Tabellen) und gegen SQLite 3.45 geprüft, einschließlich der Constraints.

## 1. Grundsätze

- **Datenbank:** SQLite (ADR-0003). Fremdschlüssel werden auf jeder Verbindung mit `PRAGMA foreign_keys = ON` aktiviert. Betrieb nach C5 (08.11.2026): Journalmodus `WAL`, `synchronous = FULL`, `busy_timeout = 5000` ms auf jeder Verbindung. Das Backend hat genau eine schreibende Verbindung (alle ändernden Transaktionen laufen nacheinander über sie, `BEGIN IMMEDIATE`) und mehrere lesende. Datei `trackcraft.db` in `database_dir`. Die Schemaversion steht in `PRAGMA user_version`; Migrationen sind nummerierte SQL-Dateien im Programm, jede läuft in einer eigenen Transaktion und setzt `user_version`. Während der Migrationen sind die Fremdschlüssel aus, damit eine Tabelle neu angelegt werden kann (SQLite kann CHECK-Bedingungen nicht ändern, z. B. Schemaversion 6); vor jedem COMMIT prüft `foreign_key_check`, danach sind sie wieder an. Konsistente Kopien (Sicherung) nur über die SQLite-Backup-API oder `VACUUM INTO`, nie als Dateikopie der laufenden Datenbank.
- **IDs:** UUID als `TEXT`, im Backend erzeugt (UUIDv7). Es gibt keine datenbankseitige ID-Erzeugung.
- **Zeit:** Alle Zeitpunkte sind UTC als `INTEGER` in Millisekunden seit 1970-01-01 (Spalten `*_ms`). Lokale Kalenderzuordnung wird aus der TOML-Zeitzone abgeleitet und nicht gespeichert (RFC-0009). Das Geburtsdatum des Benutzers ist ein reines Datum (`YYYY-MM-DD`).
- **Messpunkte:** Ein BLOB je Aufzeichnungsabschnitt (`track_sections.data`), mit `format_version`. Das Binärformat steht in [RFC-0022](RFC-0022-track-point-binary-format.md).
- **Nicht gespeichert, sondern abgeleitet:** Rang einer Bestzeit, Bestzeitenart und Zeitersparnis eines Versuchs (RFC-0009), Aufstieg und Abstieg eines Segments aus seiner Referenz (RFC-0008), Richtungsvektoren und Radien des Matchings (kommen aus Segmentreferenz und TOML, RFC-0002), Wochen-/Monats-/Jahressummen und Aktivitätsrekorde (RFC-0009).
- **Löschverhalten:** Aktivität löschen entfernt Abschnitte und Versuche (`CASCADE`). Segment löschen entfernt Versuche und Jobs, nie Aktivitäten. Die Originaldatei (`original_files`) wird von der Anwendung erst entfernt, wenn keine Aktivität mehr darauf verweist (ADR-0011), die Datenbank verhindert das vorzeitige Löschen (`RESTRICT`). Der Benutzer wird nie gelöscht.

## 2. Schema

```sql
-- TrackCraft V1 Schema (SQLite >= 3.37, STRICT). Zeitpunkte: UTC, Millisekunden seit 1970-01-01.
-- Pro Verbindung ausführen: PRAGMA foreign_keys = ON;

CREATE TABLE users (
    id            TEXT PRIMARY KEY,
    display_name  TEXT NOT NULL CHECK (length(trim(display_name)) > 0),
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL
    -- Schemaversion 10: first_name, last_name, birthday (ISO-Datum), avatar_color
) STRICT;

CREATE TABLE original_files (
    id            TEXT PRIMARY KEY,
    format        TEXT NOT NULL CHECK (format IN ('fit', 'gpx')),
    relative_path TEXT NOT NULL UNIQUE,
    sha256        TEXT NOT NULL UNIQUE CHECK (length(sha256) = 64),
    size_bytes    INTEGER NOT NULL CHECK (size_bytes > 0),
    created_at_ms INTEGER NOT NULL
) STRICT;

CREATE TABLE activities (
    id                      TEXT PRIMARY KEY,
    user_id                 TEXT NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    original_file_id        TEXT NOT NULL REFERENCES original_files(id) ON DELETE RESTRICT,
    session_index           INTEGER NOT NULL DEFAULT 0 CHECK (session_index >= 0),

    title                   TEXT NOT NULL CHECK (length(trim(title)) > 0),
    description             TEXT,
    location                TEXT CHECK (location IS NULL OR length(trim(location)) > 0),
    sport_class             TEXT NOT NULL DEFAULT 'unknown'
                            CHECK (sport_class IN ('cycling', 'ebike', 'running', 'skiing', 'swimming', 'studio', 'unknown')),
    source_sport            TEXT,
    sub_sport               TEXT,

    start_time_ms           INTEGER NOT NULL,
    start_time_original_ms  INTEGER,
    start_time_origin       TEXT NOT NULL
                            CHECK (start_time_origin IN ('file', 'first_point', 'manual', 'manual_corrected')),
    total_time_ms           INTEGER NOT NULL CHECK (total_time_ms >= 0),
    total_time_original_ms  INTEGER CHECK (total_time_original_ms IS NULL OR total_time_original_ms >= 0),
    total_time_origin       TEXT NOT NULL
                            CHECK (total_time_origin IN ('file', 'gps_points', 'manual', 'manual_corrected')),

    distance_m              REAL CHECK (distance_m IS NULL OR distance_m >= 0),
    distance_origin         TEXT CHECK (distance_origin IS NULL OR distance_origin IN ('file', 'gps_points')),
    elevation_gain_file_m   REAL CHECK (elevation_gain_file_m IS NULL OR elevation_gain_file_m > 0),

    processing_status       TEXT NOT NULL DEFAULT 'pending'
                            CHECK (processing_status IN ('pending', 'running', 'done', 'failed')),
    filtered_outliers_count INTEGER NOT NULL DEFAULT 0 CHECK (filtered_outliers_count >= 0),
    max_speed_threshold_kmh REAL CHECK (max_speed_threshold_kmh IS NULL OR max_speed_threshold_kmh > 0),

    revision                INTEGER NOT NULL DEFAULT 1,
    created_at_ms           INTEGER NOT NULL,
    updated_at_ms           INTEGER NOT NULL,
    avg_heart_rate_bpm      INTEGER CHECK (avg_heart_rate_bpm IS NULL OR (avg_heart_rate_bpm > 0 AND avg_heart_rate_bpm <= 300)),
    has_position            INTEGER CHECK (has_position IS NULL OR has_position IN (0, 1)),  -- Migration 3, NULL = noch nicht bestimmt
    heart_rate_origin       TEXT CHECK (heart_rate_origin IS NULL OR heart_rate_origin IN ('file', 'points')),
    CHECK ((avg_heart_rate_bpm IS NULL) = (heart_rate_origin IS NULL)),
    CHECK ((distance_m IS NULL) = (distance_origin IS NULL)),
    UNIQUE (original_file_id, session_index)
) STRICT;

CREATE INDEX idx_activities_start ON activities (start_time_ms);
CREATE INDEX idx_activities_class_start ON activities (sport_class, start_time_ms);

CREATE TABLE track_sections (
    id             TEXT PRIMARY KEY,
    activity_id    TEXT NOT NULL REFERENCES activities(id) ON DELETE CASCADE,
    section_index  INTEGER NOT NULL CHECK (section_index >= 0),
    format_version INTEGER NOT NULL CHECK (format_version >= 1),
    point_count    INTEGER NOT NULL CHECK (point_count > 0),
    first_time_ms  INTEGER,
    last_time_ms   INTEGER,
    data           BLOB NOT NULL,
    UNIQUE (activity_id, section_index)
) STRICT;

CREATE TABLE segments (
    id                       TEXT PRIMARY KEY,
    creator_id               TEXT NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    name                     TEXT NOT NULL CHECK (length(trim(name)) > 0),
    description              TEXT,
    sport_class              TEXT NOT NULL CHECK (sport_class IN ('cycling', 'ebike', 'running', 'skiing')),
    kind                     TEXT NOT NULL DEFAULT 'segment' CHECK (kind IN ('segment', 'route')),  -- Strecke: RFC-0016
    length_m                 REAL NOT NULL CHECK (length_m > 0),
    start_lat                REAL NOT NULL,
    start_lng                REAL NOT NULL,
    end_lat                  REAL NOT NULL,
    end_lng                  REAL NOT NULL,
    reference_format_version INTEGER NOT NULL CHECK (reference_format_version >= 1),
    reference_point_count    INTEGER NOT NULL CHECK (reference_point_count >= 2),
    reference                BLOB NOT NULL,
    created_at_ms            INTEGER NOT NULL,
    updated_at_ms            INTEGER NOT NULL
) STRICT;

CREATE TABLE calculation_versions (
    id                TEXT PRIMARY KEY,
    algorithm_version INTEGER NOT NULL,
    parameters_hash   TEXT NOT NULL,
    status            TEXT NOT NULL CHECK (status IN ('running', 'published', 'failed', 'superseded')),
    error_message     TEXT,
    created_at_ms     INTEGER NOT NULL,
    published_at_ms   INTEGER
) STRICT;

CREATE UNIQUE INDEX idx_calc_one_published ON calculation_versions (status) WHERE status = 'published';

CREATE TABLE activity_calculations (
    calculation_version_id TEXT NOT NULL REFERENCES calculation_versions(id) ON DELETE CASCADE,
    activity_id            TEXT NOT NULL REFERENCES activities(id) ON DELETE CASCADE,
    hysteresis_gain_m      REAL CHECK (hysteresis_gain_m IS NULL OR hysteresis_gain_m >= 0),
    PRIMARY KEY (calculation_version_id, activity_id)
) STRICT;

CREATE TABLE segment_efforts (
    id                     TEXT PRIMARY KEY,
    calculation_version_id TEXT NOT NULL REFERENCES calculation_versions(id) ON DELETE CASCADE,
    segment_id             TEXT NOT NULL REFERENCES segments(id) ON DELETE CASCADE,
    activity_id            TEXT NOT NULL REFERENCES activities(id) ON DELETE CASCADE,
    section_id             TEXT NOT NULL REFERENCES track_sections(id) ON DELETE CASCADE,
    attempt_position       INTEGER NOT NULL CHECK (attempt_position >= 0),

    status                 TEXT NOT NULL CHECK (status IN ('valid', 'not_ranked')),
    not_ranked_reason      TEXT CHECK (not_ranked_reason IS NULL
                                       OR not_ranked_reason IN ('ambiguous_geometry', 'time_invalid')),
    start_time_ms          INTEGER,
    end_time_ms            INTEGER,
    duration_ms            INTEGER,

    start_deviation_m      REAL,
    end_deviation_m        REAL,
    start_angle_deg        REAL,
    end_angle_deg          REAL,
    coverage_ratio         REAL CHECK (coverage_ratio IS NULL OR (coverage_ratio >= 0 AND coverage_ratio <= 1)),
    avg_heart_rate_bpm     INTEGER CHECK (avg_heart_rate_bpm IS NULL OR avg_heart_rate_bpm > 0),  -- RFC-0002 Abschnitt 6
    max_heart_rate_bpm     INTEGER CHECK (max_heart_rate_bpm IS NULL OR max_heart_rate_bpm > 0),
    created_at_ms          INTEGER NOT NULL,

    CHECK (  (status = 'valid' AND not_ranked_reason IS NULL
              AND start_time_ms IS NOT NULL AND end_time_ms IS NOT NULL
              AND duration_ms IS NOT NULL AND duration_ms > 0
              AND end_time_ms - start_time_ms = duration_ms)
          OR (status = 'not_ranked' AND not_ranked_reason IS NOT NULL)),
    UNIQUE (calculation_version_id, segment_id, section_id, attempt_position)
) STRICT;

CREATE INDEX idx_efforts_segment_rank ON segment_efforts (segment_id, status, duration_ms, start_time_ms);
CREATE INDEX idx_efforts_segment_history ON segment_efforts (calculation_version_id, segment_id, status, start_time_ms);
CREATE INDEX idx_efforts_activity ON segment_efforts (activity_id);

CREATE TABLE jobs (
    id                     TEXT PRIMARY KEY,
    kind                   TEXT NOT NULL CHECK (kind IN ('segment_history', 'activity_evaluation', 'recalculation')),
    segment_id             TEXT REFERENCES segments(id) ON DELETE CASCADE,
    activity_id            TEXT REFERENCES activities(id) ON DELETE CASCADE,
    calculation_version_id TEXT REFERENCES calculation_versions(id) ON DELETE CASCADE,
    status                 TEXT NOT NULL DEFAULT 'queued' CHECK (status IN ('queued', 'running', 'done', 'failed')),
    priority               INTEGER NOT NULL DEFAULT 10,
    processed_items        INTEGER NOT NULL DEFAULT 0 CHECK (processed_items >= 0),
    total_items            INTEGER NOT NULL DEFAULT 0 CHECK (total_items >= 0),
    error_message          TEXT,
    created_at_ms          INTEGER NOT NULL,
    started_at_ms          INTEGER,
    finished_at_ms         INTEGER
) STRICT;

CREATE INDEX idx_jobs_queue ON jobs (status, priority, created_at_ms);

CREATE TABLE import_jobs (
    id                      TEXT PRIMARY KEY,
    original_filename       TEXT NOT NULL,
    format                  TEXT CHECK (format IS NULL OR format IN ('fit', 'gpx')),
    size_bytes              INTEGER,
    sha256                  TEXT CHECK (sha256 IS NULL OR length(sha256) = 64),
    status                  TEXT NOT NULL
                            CHECK (status IN ('staged', 'awaiting_decision', 'running', 'done', 'failed', 'rolled_back')),
    error_code              TEXT,
    error_message           TEXT,
    imported_activity_count INTEGER CHECK (imported_activity_count IS NULL OR imported_activity_count >= 0),
    created_at_ms           INTEGER NOT NULL,
    finished_at_ms          INTEGER,
    sessions_json           TEXT,   -- geprüfte Sessions des Uploads (RFC-0003 Abschnitt 8), Migration 2
    imported_activity_ids   TEXT    -- JSON-Array der gespeicherten oder ersetzten Aktivitäten, Migration 2
) STRICT;

CREATE INDEX idx_import_jobs_status ON import_jobs (status, created_at_ms);
CREATE INDEX idx_jobs_finished ON jobs (status, finished_at_ms);

-- Audit-Protokoll (C11, RFC-0011), Migration 2
CREATE TABLE audit_log (
    id          TEXT PRIMARY KEY,
    at_ms       INTEGER NOT NULL,
    action      TEXT NOT NULL CHECK (action IN ('import', 'replace', 'delete', 'backup', 'restore')),
    object_type TEXT NOT NULL CHECK (object_type IN ('activity', 'segment', 'import_job', 'backup')),
    object_id   TEXT,
    outcome     TEXT NOT NULL CHECK (outcome IN ('ok', 'failed')),
    details     TEXT
) STRICT;

CREATE INDEX idx_audit_at ON audit_log (at_ms);

-- Frei benannte Saisons (RFC-0009), Migration 5; keine Überschneidung, prüft die Anwendung
CREATE TABLE seasons (
    id            TEXT PRIMARY KEY,
    name          TEXT NOT NULL CHECK (length(trim(name)) > 0),
    start_date    TEXT NOT NULL,  -- lokaler Kalendertag YYYY-MM-DD
    end_date      TEXT NOT NULL,  -- einschließlich
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL,
    CHECK (start_date <= end_date)
) STRICT;

CREATE UNIQUE INDEX idx_seasons_name ON seasons (lower(trim(name)));
CREATE INDEX idx_seasons_start ON seasons (start_date);

-- Sportart-Zuordnung und Untertyp-Katalog (RFC-0008 Abschnitt 4), Migration 7
CREATE TABLE sub_sports (
    sport_class TEXT NOT NULL,      -- sieben Sportklassen; unknown nur aus der Migration vorhandener Texte
    key         TEXT NOT NULL,      -- fester Schlüssel (road, mtb …) oder custom_…, nur a-z 0-9 _
    name        TEXT,               -- eigener Name; NULL = Oberfläche übersetzt den Schlüssel
    position    INTEGER NOT NULL,   -- Anzeigereihenfolge je Klasse
    PRIMARY KEY (sport_class, key),
    CHECK (name IS NOT NULL OR key NOT GLOB 'custom_*')
) STRICT;

CREATE TABLE sport_mappings (
    source_sport  TEXT PRIMARY KEY,
    sport_class   TEXT NOT NULL,    -- eine der sieben Sportklassen, auch unknown
    sub_sport     TEXT,             -- Schlüssel
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL,
    FOREIGN KEY (sport_class, sub_sport) REFERENCES sub_sports (sport_class, key) ON UPDATE RESTRICT ON DELETE RESTRICT
) STRICT;

CREATE TABLE sport_defaults_applied (   -- angewendete Vorgaben und einmalige TOML-Übernahme
    key           TEXT PRIMARY KEY,
    applied_at_ms INTEGER NOT NULL
) STRICT;

-- activities: neu angelegt mit FOREIGN KEY (sport_class, sub_sport) REFERENCES sub_sports (sport_class, key)
CREATE INDEX idx_activities_source_sport ON activities (source_sport);
CREATE INDEX idx_activities_sub_sport ON activities (sport_class, sub_sport);

CREATE TABLE data_state (
    id                    INTEGER PRIMARY KEY CHECK (id = 1),
    dataset_id            TEXT NOT NULL,
    revision_id           TEXT NOT NULL,
    predecessor_revision_id TEXT,
    change_counter        INTEGER NOT NULL DEFAULT 0 CHECK (change_counter >= 0),
    updated_at_ms         INTEGER NOT NULL
) STRICT;
```

### Benutzerprofil (Schemaversion 10)

- `users.display_name` ist der **Spitzname** (Pflicht). Dazu freiwillig `first_name`, `last_name`, `birthday` (ISO-Datum, `date(birthday) = birthday`) und `avatar_color` (Farbschlüssel des Benutzersymbols, Liste in `profile.rs`).
- Die Datenbank ist führend: `[user] display_name` der TOML legt nur beim ersten Start den Benutzer an (ohne Abschnitt Spitzname „Ich“), danach wird ein abweichender Wert nur im Log gemeldet. Änderungen über `PATCH /profile`, eine tatsächliche Änderung erhöht den Änderungszähler (RFC-0012).
- **Zweck des Geburtstags** (B13, Uli 16.11.2026): freiwillig; das Alter dient später zum Vorbelegen von HF-Zonen und für Auswertungen (Trainingsdaten, Prognosen). Der Wert liegt wie alle Profildaten unverschlüsselt in der Sicherung.
- Zuordnung: `activities.user_id` (Besitzer) und `segments.creator_id` („erstellt von“, Segmente und Strecken) bestehen seit Schema 1 und werden mit dem einzigen Benutzer gefüllt. Im **Einzelbenutzerbetrieb** (bisher der einzige) zeigt die Oberfläche keine Zuordnungen; ein Schalter für Mehrbenutzerbetrieb folgt später in den Einstellungen.
- Ein eigenes Bild als Benutzersymbol folgt später, gemeinsam mit Bildern für Ausrüstung (E1).

### Garmin-Abruf (Schemaversion 8)

- `garmin_settings`: genau eine Zeile mit Schlüssel (Geheimnis, nie ausgegeben), `enabled`, `start_date`, `interval_minutes`, `backfill_done` und Laufstatus (letzter Start, letztes Ende, letzter Erfolg, letzter Fehler).
- `garmin_activities`: Garmin-ID als Primärschlüssel, Start, Garmin-Typ, Status (`imported`, `linked`, `locked`, `rejected`, `failed`), Verweis auf die Aktivität (`ON DELETE SET NULL`), Grund, Versuche.
- `import_jobs.source_sport_override`: Garmin-Typ, der beim Prüfen und Bestätigen den `source_sport` der Datei ersetzt.

## 3. Zuordnung zu den Fachregeln

| Regel | Umsetzung |
| :--- | :--- |
| Ein fester Benutzer, Besitzer-/Erstellerbezug (ADR-0001, ADR-0012) | `users`, `activities.user_id`, `segments.creator_id`; die Einzigkeit des Benutzers sichert die Anwendung |
| Mehrere FIT-Sessions teilen eine Originaldatei, fehlende Sessions nachimportierbar (ADR-0011) | `original_files` (eine Zeile je identischer Datei per `sha256`), `activities.session_index`, `UNIQUE (original_file_id, session_index)`; GPX immer `session_index = 0` |
| Ersetzen behält die Aktivitäts-ID (RFC-0003) | `original_file_id`/`session_index` werden auf derselben Zeile getauscht |
| Sportklasse `cycling/ebike/running/skiing/swimming/studio/unknown` (Schemaversion 6), Untertyp Freitext (ADR-0005, RFC-0017) | `sport_class`, `sub_sport`; `source_sport` hält die Sportart laut Datei für Mapping und Outlier-Filter |
| Start und Gesamtzeit mit Herkunft, Originalwerte bleiben bei Korrektur (RFC-0017) | `start_time_*`, `total_time_*`, jeweils mit `*_origin` und `*_original_ms` |
| Fehlende Werte sind unbekannt, nie 0 (RFC-0009) | `distance_m`, `elevation_gain_file_m`, `hysteresis_gain_m` und `avg_heart_rate_bpm` nullable, `*_origin` genau dann gesetzt, wenn der Wert vorhanden ist |
| Durchschnittliche Herzfrequenz in KPI-Leiste und Feed (RFC-0006, RFC-0018), keine HF-Analysen (RFC-0009) | `avg_heart_rate_bpm` mit `heart_rate_origin` (`file` oder `points`), nur als Anzeigewert |
| Ort als freies Feld (RFC-0006, RFC-0018) | `activities.location`, optional, ohne Geocoding oder externe Quelle |
| Keine Bewegungszeit (ADR-0001) | keine entsprechende Spalte |
| Importabschluss getrennt von Auswertung (ADR-0001) | `processing_status` gilt nur für die Auswertung; eine Aktivität existiert erst nach abgeschlossenem Import |
| Outlier-Filter (RFC-0008) | `filtered_outliers_count`, `max_speed_threshold_kmh` (`NULL`, wenn der Filter nicht angewendet wurde) |
| Segment: vollständige Referenz, kopiert, ohne Bezug zu Quelle oder Originaldatei (ADR-0005) | `segments.reference` (BLOB), kein Verweis auf Aktivität oder Datei |
| Segment: Sportklasse fest, `unknown` nicht möglich (ADR-0005, ADR-0015) | `CHECK` auf `cycling/ebike/running/skiing` |
| Versuche: ms-Dauer, stabile Sortierung, keine doppelten Versuche (ADR-0015, RFC-0009) | `duration_ms`, `attempt_position`, `UNIQUE (calculation_version_id, segment_id, section_id, attempt_position)` |
| Nicht wertbare Treffer mit sichtbarem Grund (RFC-0016) | `status = 'not_ranked'` mit `not_ranked_reason` |
| Erste Bestzeit / Verbesserung / Einstellung (RFC-0009) | nicht gespeichert, bei Abfrage chronologisch aus den gültigen Versuchen abgeleitet (`idx_efforts_segment_history`) |
| Aufstieg: Dateiwert vor Hysterese, Hysterese hängt von `hysteresis_m` ab (RFC-0008, RFC-0021) | `activities.elevation_gain_file_m` hält nur den Dateiwert, `activity_calculations.hysteresis_gain_m` je Berechnungsversion; Wert und Herkunft (`file`/`hysteresis`) werden bei Abfrage gewählt |
| Versionierte Neuberechnung, gemeinsame Veröffentlichung (RFC-0007, RFC-0009) | `calculation_versions` (höchstens ein veröffentlichter Stand), `segment_efforts.calculation_version_id`, `activity_calculations.calculation_version_id`; Datenänderungen als Nachtrag im veröffentlichten Stand (RFC-0007) |
| Asynchrone, wiederaufnehmbare Auswertung (RFC-0007, ADR-0006) | `jobs` |
| Importvorgänge und deren Status (RFC-0003) | `import_jobs`; die geprüften Sessions stehen als JSON in `sessions_json`, Sperren durch vorhandene Sessions und Duplikatkandidaten werden bei jeder Abfrage neu bestimmt. Aufbewahrung nach C12 (RFC-0008 Abschnitt 8.2) |
| Audit für Import, Ersetzen, Löschen, Sicherung, Restore (RFC-0011, C11) | `audit_log`, geschrieben in derselben Transaktion wie die Änderung, Aufbewahrung ein Jahr |
| Saisons als frei benannte Zeiträume ohne Überschneidung (RFC-0009) | `seasons`; Überschneidung und Namensgleichheit prüft die Anwendung in der Schreibtransaktion, Zuordnung zu Aktivitäten und Versuchen bei Abfrage |
| Sportart-Zuordnung pflegbar, Updates ergänzen Vorgaben einmalig (RFC-0008) | `sport_mappings`, `sub_sports`, `sport_defaults_applied`; Untertypen als Schlüssel mit Fremdschlüssel aus `activities` und `sport_mappings`, Umbenennen ändert nur `sub_sports.name`, Löschen eines verwendeten Untertyps sperrt die Anwendung (und der Fremdschlüssel) |
| Statistik und Aktivitätsrekorde (RFC-0009, C4) | nicht gespeichert, bei Abfrage aus `activities` und dem veröffentlichten Stand berechnet |
| Datenstandsidentität für Sicherung/Restore (ADR-0014, RFC-0012) | `data_state` (genau eine Zeile) |

## 4. Offene Punkte

1. Veröffentlichungsablauf von `calculation_versions` und Aufräumen alter Stände. Erste Stufe seit M2: Beim Start wird ein veröffentlichter Stand zu Algorithmus und Parametern (`parameters_hash` über Hysterese und Matching-Parameter) sichergestellt. Passt er nicht, wird ein neuer Stand vollständig berechnet und ersetzt den alten (`superseded`, dessen Hysteresewerte entfallen). Fehlende Werte werden nachgetragen, der Import trägt neue Aktivitäten direkt ein. Den vollständigen Ablauf mit Segmenten legt C3 in M3 fest.
2. Sicherungsstatus (letzter Prüfversuch, letzte erfolgreiche Sicherung, Tageszustand) und Zustände für Restore (RFC-0012).
3. ~~Ob Statistiken und Aktivitätsrekorde bei Abfrage berechnet oder gespeichert werden.~~ Entschieden (C4, 13.11.2026): bei Abfrage.
4. Weitere Sensorwerte neben der durchschnittlichen Herzfrequenz (Verläufe, Kadenz, Leistung) sind nicht Teil von V1.
5. Entfällt: Journalmodus, Sperrverhalten und Migrationen sind in Abschnitt 1 festgelegt (C5).
