-- 0018: Mehrere Ziele im Plan, ohne Wochenvorlagen (Überarbeitung der Seite „Plan“, Uli 23.11.2026).
-- Ein Ziel ist ein Ereignis im Kalender (Wettkampf, Tour) mit Datum, Sportklasse, Name und optionalen Angaben.
-- Die Planungshilfe plant auf ein gewähltes Ziel; danach sind die vorgeschlagenen Einheiten normale Einheiten.
CREATE TABLE plan_goals (
    id            TEXT PRIMARY KEY,
    date          TEXT NOT NULL CHECK (date GLOB '[0-9][0-9][0-9][0-9]-[0-1][0-9]-[0-3][0-9]'),
    sport_class   TEXT NOT NULL CHECK (sport_class IN ('cycling', 'ebike', 'running', 'skiing', 'swimming', 'studio')),
    name          TEXT NOT NULL CHECK (length(trim(name)) > 0),
    description   TEXT,
    url           TEXT,
    distance_m    REAL CHECK (distance_m IS NULL OR distance_m > 0),
    target_time_s INTEGER CHECK (target_time_s IS NULL OR target_time_s > 0),
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL
) STRICT;
CREATE INDEX idx_plan_goals_date ON plan_goals (date);

-- Das bisherige einzelne Ziel übernehmen; Sportklasse aus den gemerkten Eingaben der Planungshilfe, sonst Radfahren.
INSERT INTO plan_goals (id, date, sport_class, name, created_at_ms, updated_at_ms)
SELECT lower(hex(randomblob(4)) || '-' || hex(randomblob(2)) || '-' || hex(randomblob(2)) || '-' || hex(randomblob(2)) || '-' || hex(randomblob(6))),
       goal_date,
       CASE WHEN json_valid(assistant)
                 AND json_extract(assistant, '$.sport_class') IN ('cycling', 'ebike', 'running', 'skiing', 'swimming', 'studio')
            THEN json_extract(assistant, '$.sport_class') ELSE 'cycling' END,
       goal_name,
       updated_at_ms,
       updated_at_ms
FROM plan_settings WHERE id = 1 AND goal_date IS NOT NULL;

-- plan_settings behält nur noch die gemerkten Eingaben der Planungshilfe (CHECK über goal_* → Neuaufbau).
CREATE TABLE plan_settings_new (
    id            INTEGER PRIMARY KEY CHECK (id = 1),
    assistant     TEXT,
    updated_at_ms INTEGER NOT NULL DEFAULT 0
) STRICT;
INSERT INTO plan_settings_new (id, assistant, updated_at_ms) SELECT id, assistant, updated_at_ms FROM plan_settings;
DROP TABLE plan_settings;
ALTER TABLE plan_settings_new RENAME TO plan_settings;

-- Vorgeschlagene Einheiten sind normale Einheiten: keine Kennung des Vorschlags mehr, nichts wird ersetzt.
DROP INDEX idx_planned_units_suggestion;
ALTER TABLE planned_units DROP COLUMN suggestion_id;

-- Wochenvorlagen entfallen (Uli 23.11.2026); Einheiten aus früher angewendeten Vorlagen bleiben im Kalender.
DROP TABLE plan_templates;
