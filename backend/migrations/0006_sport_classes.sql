-- Schemaversion 6: weitere Sportklassen. Aktivitäten erlauben skiing, swimming, studio; Segmente und
-- Strecken zusätzlich skiing (Ulis Entscheidung 14.11.2026). SQLite kann CHECK-Bedingungen nicht ändern, daher werden
-- beide Tabellen nach dem Verfahren der SQLite-Doku neu angelegt. Der Migrationslauf schaltet dafür die Fremdschlüssel
-- ab und prüft sie vor dem COMMIT (db::Db::migrate); Verweise anderer Tabellen bleiben über den Namen erhalten.

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
    UNIQUE (original_file_id, session_index)
) STRICT;

INSERT INTO activities_new (id, user_id, original_file_id, session_index, title, description, location, sport_class, source_sport, sub_sport, start_time_ms, start_time_original_ms, start_time_origin, total_time_ms, total_time_original_ms, total_time_origin, distance_m, distance_origin, elevation_gain_file_m, processing_status, filtered_outliers_count, max_speed_threshold_kmh, revision, created_at_ms, updated_at_ms, avg_heart_rate_bpm, heart_rate_origin, has_position)
SELECT id, user_id, original_file_id, session_index, title, description, location, sport_class, source_sport, sub_sport, start_time_ms, start_time_original_ms, start_time_origin, total_time_ms, total_time_original_ms, total_time_origin, distance_m, distance_origin, elevation_gain_file_m, processing_status, filtered_outliers_count, max_speed_threshold_kmh, revision, created_at_ms, updated_at_ms, avg_heart_rate_bpm, heart_rate_origin, has_position FROM activities;
DROP TABLE activities;
ALTER TABLE activities_new RENAME TO activities;
CREATE INDEX idx_activities_start ON activities (start_time_ms);
CREATE INDEX idx_activities_class_start ON activities (sport_class, start_time_ms);

CREATE TABLE segments_new (
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
    updated_at_ms            INTEGER NOT NULL,
    category_auto            TEXT
    CHECK (category_auto IS NULL OR category_auto IN ('climb', 'descent', 'rolling', 'loop', 'out_and_back', 'point_to_point')),
    category_manual          TEXT
    CHECK (category_manual IS NULL OR category_manual IN ('climb', 'descent', 'rolling', 'loop', 'out_and_back', 'point_to_point')),
    location                 TEXT
) STRICT;

INSERT INTO segments_new (id, creator_id, name, description, sport_class, kind, length_m, start_lat, start_lng, end_lat, end_lng, reference_format_version, reference_point_count, reference, created_at_ms, updated_at_ms, category_auto, category_manual, location)
SELECT id, creator_id, name, description, sport_class, kind, length_m, start_lat, start_lng, end_lat, end_lng, reference_format_version, reference_point_count, reference, created_at_ms, updated_at_ms, category_auto, category_manual, location FROM segments;
DROP TABLE segments;
ALTER TABLE segments_new RENAME TO segments;
