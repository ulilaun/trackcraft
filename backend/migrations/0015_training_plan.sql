-- 0015: Trainingsplanung, Stufe 1 (E8, #12, Uli 23.11.2026).
-- Geplante Einheiten getrennt von den Aktivitäten. Belastung entweder als Zielbelastung (mit Anteil „intensiv“) oder als
-- Minuten je HF-Zone; die Belastung selbst wird nicht gespeichert, sondern mit dem am Tag gültigen Zonensatz berechnet.
-- „Erledigt“ heißt: mit einer Aktivität verknüpft (activity_id); gelöschte Aktivität → wieder geplant.
CREATE TABLE planned_units (
    id            TEXT PRIMARY KEY,
    date          TEXT NOT NULL CHECK (date GLOB '[0-9][0-9][0-9][0-9]-[0-1][0-9]-[0-3][0-9]'),
    sport_class   TEXT NOT NULL CHECK (sport_class IN ('cycling', 'ebike', 'running', 'skiing', 'swimming', 'studio')),
    sub_sport     TEXT,
    title         TEXT NOT NULL CHECK (length(trim(title)) > 0),
    note          TEXT,
    mode          TEXT NOT NULL CHECK (mode IN ('target', 'zones')),
    duration_min  INTEGER CHECK (duration_min IS NULL OR duration_min BETWEEN 1 AND 1440),
    target_load   REAL CHECK (target_load IS NULL OR target_load >= 0),
    intensive_pct INTEGER NOT NULL DEFAULT 0 CHECK (intensive_pct BETWEEN 0 AND 100),
    -- JSON-Array mit fünf Minutenwerten (nur mode = 'zones')
    zone_minutes  TEXT,
    status        TEXT NOT NULL DEFAULT 'planned' CHECK (status IN ('planned', 'skipped')),
    activity_id   TEXT REFERENCES activities(id) ON DELETE SET NULL,
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL,
    CHECK ((mode = 'target') = (duration_min IS NOT NULL AND target_load IS NOT NULL)),
    CHECK ((mode = 'zones') = (zone_minutes IS NOT NULL))
) STRICT;
CREATE INDEX idx_planned_units_date ON planned_units (date);
CREATE UNIQUE INDEX idx_planned_units_activity ON planned_units (activity_id) WHERE activity_id IS NOT NULL;

-- „Nicht verknüpfen“: dieser Vorschlag erscheint für diese Aktivität nicht mehr.
CREATE TABLE planned_unit_declines (
    unit_id     TEXT NOT NULL REFERENCES planned_units(id) ON DELETE CASCADE,
    activity_id TEXT NOT NULL REFERENCES activities(id) ON DELETE CASCADE,
    PRIMARY KEY (unit_id, activity_id)
) STRICT;

-- Ein Zieltermin (Wettkampf, Tour) als Markierung in Plan und Vorschau.
CREATE TABLE plan_settings (
    id            INTEGER PRIMARY KEY CHECK (id = 1),
    goal_date     TEXT CHECK (goal_date IS NULL OR goal_date GLOB '[0-9][0-9][0-9][0-9]-[0-1][0-9]-[0-3][0-9]'),
    goal_name     TEXT,
    updated_at_ms INTEGER NOT NULL DEFAULT 0,
    CHECK ((goal_date IS NULL) = (goal_name IS NULL))
) STRICT;
INSERT INTO plan_settings (id) VALUES (1);
