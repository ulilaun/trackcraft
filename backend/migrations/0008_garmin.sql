-- Schemaversion 8: Abruf aus Garmin Connect (Uli 14.11.2026). Gesamtimport der Historie und
-- regelmäßiger Abruf neuer Aktivitäten über die persönliche Garmin-API. Der Schlüssel ist ein Geheimnis: Die API
-- gibt ihn nie zurück, das Log nennt ihn nie (C7).

-- Einstellungen und Laufstatus, genau eine Zeile.
CREATE TABLE garmin_settings (
    id                  INTEGER PRIMARY KEY CHECK (id = 1),
    api_key             TEXT CHECK (api_key IS NULL OR length(api_key) BETWEEN 8 AND 4096),
    key_set_at_ms       INTEGER,
    enabled             INTEGER NOT NULL DEFAULT 0 CHECK (enabled IN (0, 1)),
    -- Frühester Starttag (YYYY-MM-DD, Konfigurationszone); NULL = gesamte Historie
    start_date          TEXT CHECK (start_date IS NULL OR start_date GLOB '[0-9][0-9][0-9][0-9]-[0-1][0-9]-[0-3][0-9]'),
    interval_minutes    INTEGER NOT NULL DEFAULT 30 CHECK (interval_minutes BETWEEN 5 AND 1440),
    -- Gesamtimport bis zum Listenende bzw. Anfangsdatum durchgelaufen; danach reicht der Blick auf neue Einträge
    backfill_done       INTEGER NOT NULL DEFAULT 0 CHECK (backfill_done IN (0, 1)),
    last_run_started_ms INTEGER,
    last_run_finished_ms INTEGER,
    last_success_ms     INTEGER,
    last_error_code     TEXT,
    last_error_message  TEXT,
    updated_at_ms       INTEGER NOT NULL
) STRICT;

INSERT INTO garmin_settings (id, updated_at_ms) VALUES (1, 0);

-- Herkunft je Garmin-Aktivität. Jede bearbeitete Garmin-ID steht hier genau einmal und wird nicht erneut geholt,
-- außer im Status failed (vorübergehender Fehler). Wird die TrackCraft-Aktivität gelöscht, bleibt der Eintrag
-- ohne Verweis stehen; der Abruf bringt sie nicht zurück.
CREATE TABLE garmin_activities (
    garmin_id        INTEGER PRIMARY KEY,
    start_time_ms    INTEGER NOT NULL,
    type_key         TEXT,
    -- imported: neu angelegt; linked: gab es schon (gleiche Datei oder Duplikatkandidat), nur verknüpft;
    -- locked: Session gesperrt (Fahrzeug, Punktgrenze, ohne Startzeit); rejected: Datei unlesbar;
    -- failed: Abruf gescheitert, wird beim nächsten Lauf erneut versucht
    status           TEXT NOT NULL CHECK (status IN ('imported', 'linked', 'locked', 'rejected', 'failed')),
    activity_id      TEXT REFERENCES activities (id) ON DELETE SET NULL,
    reason_code      TEXT,
    reason           TEXT,
    attempts         INTEGER NOT NULL DEFAULT 1,
    created_at_ms    INTEGER NOT NULL,
    updated_at_ms    INTEGER NOT NULL
) STRICT;

CREATE INDEX idx_garmin_activities_status ON garmin_activities (status, start_time_ms);
CREATE INDEX idx_garmin_activities_activity ON garmin_activities (activity_id);

-- Garmin-Typ ersetzt beim Abruf den source_sport der Datei (Uli); der Wert muss beim Bestätigen
-- wieder angewendet werden.
ALTER TABLE import_jobs ADD COLUMN source_sport_override TEXT;
