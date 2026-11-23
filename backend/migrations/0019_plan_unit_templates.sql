-- 0019: Einheiten-Vorlagen (Überarbeitung der Seite „Plan“, Uli 23.11.2026).
-- Eine Vorlage ist eine geplante Einheit ohne Datum und Status unter einem eindeutigen Namen (ohne Groß/Klein).
-- Im Einheiten-Dialog füllt „Aus Vorlage“ die Felder; verwaltet wird unter Einstellungen → Plan-Vorlagen.
CREATE TABLE plan_unit_templates (
    id            TEXT PRIMARY KEY,
    name          TEXT NOT NULL UNIQUE COLLATE NOCASE CHECK (length(trim(name)) > 0),
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
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL,
    CHECK ((mode = 'target') = (duration_min IS NOT NULL AND target_load IS NOT NULL)),
    CHECK ((mode = 'zones') = (zone_minutes IS NOT NULL))
) STRICT;
