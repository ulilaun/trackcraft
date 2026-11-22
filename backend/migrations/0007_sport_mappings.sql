-- Schemaversion 7: Sportart-Zuordnung in der Datenbank statt in der TOML (Uli 14.11.2026).
-- Die Sportklassen bleiben fest im Code. Untertypen haben wie die Klassen einen festen Schlüssel; die Anzeige
-- übersetzt die Oberfläche (Lokalisierung), ein eigener Name überschreibt sie. Aktivitäten und Zuordnungen
-- speichern den Schlüssel mit Fremdschlüssel auf den Katalog. Den Katalog und die Zuordnung füllt danach der
-- Start (sports::ensure): einmalig die alten TOML-Abschnitte, dann die eingebauten Vorgaben.

-- Untertyp-Katalog. Eingebaute Einträge: fester Schlüssel, name NULL (Anzeige übersetzt, umbenannt = name gesetzt).
-- Eigene Einträge: erzeugter Schlüssel custom_…, name Pflicht. Die Eindeutigkeit des angezeigten Namens je Klasse
-- prüft die Anwendung. unknown nur für Einträge, die diese Migration aus vorhandenen Aktivitäten erzeugt.
CREATE TABLE sub_sports (
    sport_class TEXT NOT NULL
                CHECK (sport_class IN ('cycling', 'ebike', 'running', 'skiing', 'swimming', 'studio', 'unknown')),
    key         TEXT NOT NULL CHECK (length(key) BETWEEN 1 AND 64 AND key NOT GLOB '*[^a-z0-9_]*'),
    name        TEXT CHECK (name IS NULL OR (length(trim(name)) > 0 AND name = trim(name))),
    position    INTEGER NOT NULL,
    PRIMARY KEY (sport_class, key),
    CHECK (name IS NOT NULL OR key NOT GLOB 'custom_*')
) STRICT;

-- source_sport → Sportklasse und Untertyp-Schlüssel. Ohne exakten Eintrag gilt der Eintrag des Teils vor dem
-- ersten „-“, dann nur für die Sportklasse (RFC-0008 Abschnitt 4).
CREATE TABLE sport_mappings (
    source_sport  TEXT PRIMARY KEY CHECK (length(trim(source_sport)) > 0 AND source_sport = trim(source_sport)),
    sport_class   TEXT NOT NULL
                  CHECK (sport_class IN ('cycling', 'ebike', 'running', 'skiing', 'swimming', 'studio', 'unknown')),
    sub_sport     TEXT,
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL,
    FOREIGN KEY (sport_class, sub_sport) REFERENCES sub_sports (sport_class, key) ON UPDATE RESTRICT ON DELETE RESTRICT
) STRICT;

CREATE INDEX idx_sport_mappings_sub ON sport_mappings (sport_class, sub_sport);

-- Bereits angewendete Vorgaben (feste Schlüssel) und die einmalige TOML-Übernahme. Ein Eintrag, den der Benutzer
-- ändert oder löscht, kommt durch ein Update nicht zurück.
CREATE TABLE sport_defaults_applied (
    key           TEXT PRIMARY KEY,
    applied_at_ms INTEGER NOT NULL
) STRICT;

-- Vorhandene Untertyp-Texte der Aktivitäten in Schlüssel umwandeln: die deutschen Bezeichnungen der eingebauten
-- Einträge (Stand Schemaversion 6) werden zu deren Schlüssel, jeder andere Text wird ein eigener Eintrag seiner Klasse.
-- Muss zu DEFAULT_CATALOG in src/sports.rs passen.
CREATE TABLE sub_sport_builtin_labels (sport_class TEXT NOT NULL, key TEXT NOT NULL, label TEXT NOT NULL, position INTEGER NOT NULL);
INSERT INTO sub_sport_builtin_labels (sport_class, key, label, position) VALUES
    ('cycling', 'road', 'Rennrad', 0), ('cycling', 'mtb', 'MTB', 1), ('cycling', 'indoor', 'Indoor', 2),
    ('running', 'street', 'Straße', 0), ('running', 'trail', 'Trail', 1), ('running', 'walking', 'Gehen', 2),
    ('skiing', 'nordic', 'Nordic', 0),
    ('swimming', 'indoor', 'Indoor', 0),
    ('studio', 'strength', 'Krafttraining', 0);

INSERT INTO sub_sports (sport_class, key, name, position)
SELECT DISTINCT l.sport_class, l.key, NULL, l.position
FROM activities a JOIN sub_sport_builtin_labels l ON l.sport_class = a.sport_class AND l.label = a.sub_sport;

INSERT INTO sub_sports (sport_class, key, name, position)
SELECT sport_class, 'custom_' || lower(hex(randomblob(8))), sub_sport, 100 + row_number() OVER (ORDER BY sport_class, sub_sport)
FROM (
    SELECT DISTINCT a.sport_class, a.sub_sport FROM activities a
    WHERE a.sub_sport IS NOT NULL
      AND NOT EXISTS (SELECT 1 FROM sub_sport_builtin_labels l WHERE l.sport_class = a.sport_class AND l.label = a.sub_sport)
);

CREATE TABLE sub_sport_migration (sport_class TEXT NOT NULL, label TEXT NOT NULL, key TEXT NOT NULL);
INSERT INTO sub_sport_migration (sport_class, label, key)
SELECT s.sport_class, coalesce(s.name, l.label), s.key
FROM sub_sports s LEFT JOIN sub_sport_builtin_labels l ON l.sport_class = s.sport_class AND l.key = s.key;

-- Aktivitäten neu anlegen (Fremdschlüssel lässt sich nicht nachträglich ergänzen), Verfahren wie Migration 6.
CREATE TABLE activities_new (
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
    heart_rate_origin       TEXT CHECK (heart_rate_origin IS NULL OR heart_rate_origin IN ('file', 'points')),
    has_position            INTEGER CHECK (has_position IS NULL OR has_position IN (0, 1)),
    CHECK ((avg_heart_rate_bpm IS NULL) = (heart_rate_origin IS NULL)),
    CHECK ((distance_m IS NULL) = (distance_origin IS NULL)),
    UNIQUE (original_file_id, session_index),
    FOREIGN KEY (sport_class, sub_sport) REFERENCES sub_sports (sport_class, key) ON UPDATE RESTRICT ON DELETE RESTRICT
) STRICT;

INSERT INTO activities_new (id, user_id, original_file_id, session_index, title, description, location, sport_class, source_sport, sub_sport, start_time_ms, start_time_original_ms, start_time_origin, total_time_ms, total_time_original_ms, total_time_origin, distance_m, distance_origin, elevation_gain_file_m, processing_status, filtered_outliers_count, max_speed_threshold_kmh, revision, created_at_ms, updated_at_ms, avg_heart_rate_bpm, heart_rate_origin, has_position)
SELECT id, user_id, original_file_id, session_index, title, description, location, sport_class, source_sport, (SELECT m.key FROM sub_sport_migration m WHERE m.sport_class = a.sport_class AND m.label = a.sub_sport), start_time_ms, start_time_original_ms, start_time_origin, total_time_ms, total_time_original_ms, total_time_origin, distance_m, distance_origin, elevation_gain_file_m, processing_status, filtered_outliers_count, max_speed_threshold_kmh, revision, created_at_ms, updated_at_ms, avg_heart_rate_bpm, heart_rate_origin, has_position FROM activities a;
DROP TABLE activities;
ALTER TABLE activities_new RENAME TO activities;
CREATE INDEX idx_activities_start ON activities (start_time_ms);
CREATE INDEX idx_activities_class_start ON activities (sport_class, start_time_ms);
CREATE INDEX idx_activities_source_sport ON activities (source_sport);
CREATE INDEX idx_activities_sub_sport ON activities (sport_class, sub_sport);

DROP TABLE sub_sport_migration;
DROP TABLE sub_sport_builtin_labels;
