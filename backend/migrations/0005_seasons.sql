-- 0005: Frei benannte Saisons (M4a, Uli 13.11.2026).

-- Zeitraum aus lokalen Kalendertagen in der Zeitzone der TOML, Ende einschließlich. Saisons gelten für alle
-- Sportklassen und überschneiden sich nicht, Lücken sind erlaubt; die Überschneidung prüft die Anwendung
-- (seasons::create/update) in derselben Schreibtransaktion.
CREATE TABLE seasons (
    id            TEXT PRIMARY KEY,
    name          TEXT NOT NULL CHECK (length(trim(name)) > 0),
    start_date    TEXT NOT NULL CHECK (start_date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
    end_date      TEXT NOT NULL CHECK (end_date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL,
    CHECK (start_date <= end_date)
) STRICT;

CREATE UNIQUE INDEX idx_seasons_name ON seasons (lower(trim(name)));
CREATE INDEX idx_seasons_start ON seasons (start_date);
