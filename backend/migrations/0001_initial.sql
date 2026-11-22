-- 0001: Erstes TrackCraft-Schema, übernommen aus RFC-0001 Abschnitt 2 (SQLite >= 3.37, STRICT).
-- Zeitpunkte: UTC, Millisekunden seit 1970-01-01. PRAGMA foreign_keys setzt die Verbindung.

CREATE TABLE users (
    id            TEXT PRIMARY KEY,
    display_name  TEXT NOT NULL CHECK (length(trim(display_name)) > 0),
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL
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
                            CHECK (sport_class IN ('cycling', 'ebike', 'running', 'unknown')),
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
    sport_class              TEXT NOT NULL CHECK (sport_class IN ('cycling', 'ebike', 'running')),
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
    finished_at_ms          INTEGER
) STRICT;

CREATE TABLE data_state (
    id                    INTEGER PRIMARY KEY CHECK (id = 1),
    dataset_id            TEXT NOT NULL,
    revision_id           TEXT NOT NULL,
    predecessor_revision_id TEXT,
    change_counter        INTEGER NOT NULL DEFAULT 0 CHECK (change_counter >= 0),
    updated_at_ms         INTEGER NOT NULL
) STRICT;
