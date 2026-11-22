-- 0011: HF-Zonen, Belastung je Aktivität und Formkurve (E7, Uli 22.11.2026).
-- Zonensätze gelten je Sportklasse (NULL = allgemeiner Satz) ab einem lokalen Datum; der älteste Satz gilt auch davor.
-- `bounds` enthält die fünf unteren Zonengrenzen in Prozent der Bezugsgröße des Modells als JSON-Liste.
CREATE TABLE hr_zone_sets (
    id            TEXT PRIMARY KEY,
    sport_class   TEXT,
    valid_from    TEXT NOT NULL CHECK (date(valid_from) = valid_from),
    max_hr        INTEGER NOT NULL CHECK (max_hr BETWEEN 100 AND 240),
    resting_hr    INTEGER NOT NULL CHECK (resting_hr BETWEEN 25 AND 120),
    threshold_hr  INTEGER CHECK (threshold_hr IS NULL OR threshold_hr BETWEEN 60 AND 240),
    model         TEXT NOT NULL CHECK (model IN ('max', 'reserve', 'threshold')),
    bounds        TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL
) STRICT;
CREATE UNIQUE INDEX hr_zone_sets_valid_from ON hr_zone_sets (coalesce(sport_class, ''), valid_from);

-- Einstellungen der Belastungsformel und der Formkurve (genau eine Zeile).
-- `estimate_per_hour`: Belastung je Stunde für Aktivitäten ohne Herzfrequenz, je Sportklasse (JSON-Objekt).
CREATE TABLE load_settings (
    id                INTEGER PRIMARY KEY CHECK (id = 1),
    sex_factor        REAL NOT NULL DEFAULT 1.92,
    fitness_days      INTEGER NOT NULL DEFAULT 42,
    fatigue_days      INTEGER NOT NULL DEFAULT 7,
    estimate_per_hour TEXT NOT NULL DEFAULT '{"cycling":50,"ebike":35,"running":65,"skiing":55,"swimming":60,"studio":40,"unknown":45}',
    updated_at_ms     INTEGER NOT NULL DEFAULT 0
) STRICT;
INSERT INTO load_settings (id) VALUES (1);

-- Abgeleitete Belastung je Aktivität. `inputs_hash` fasst alle Eingaben zusammen (Formelversion, Zonensatz,
-- Faktoren, Revision der Aktivität); weicht er ab, rechnet der Hintergrund die Zeile neu.
CREATE TABLE activity_loads (
    activity_id     TEXT PRIMARY KEY REFERENCES activities(id) ON DELETE CASCADE,
    inputs_hash     TEXT NOT NULL,
    method          TEXT NOT NULL CHECK (method IN ('samples', 'average', 'estimated')),
    zone_set_id     TEXT,
    trimp           REAL NOT NULL CHECK (trimp >= 0),
    trimp_aerobic   REAL NOT NULL CHECK (trimp_aerobic >= 0),
    trimp_anaerobic REAL NOT NULL CHECK (trimp_anaerobic >= 0),
    zone_seconds    TEXT,
    computed_at_ms  INTEGER NOT NULL
) STRICT;
