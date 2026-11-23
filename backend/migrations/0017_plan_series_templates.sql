-- 0017: Wiederkehrende Einheiten und Wochenvorlagen (E8, #12, Uli 23.11.2026).
-- Einheiten einer Serie (Wiederholen oder Vorlage über mehrere Wochen) tragen dieselbe Kennung; „diese und folgende“
-- ändert oder löscht die offenen Einheiten der Serie ab einem Datum. Jede Einheit bleibt eine normale Einheit.
ALTER TABLE planned_units ADD COLUMN series_id TEXT;
CREATE INDEX idx_planned_units_series ON planned_units (series_id, date) WHERE series_id IS NOT NULL;

-- Wochenvorlagen: Name und Einheiten als JSON-Array (Wochentag 0 = Montag, Sportklasse, Untertyp, Titel, Notiz,
-- Belastung wie in planned_units, ohne Status).
CREATE TABLE plan_templates (
    id            TEXT PRIMARY KEY,
    name          TEXT NOT NULL UNIQUE COLLATE NOCASE CHECK (length(trim(name)) > 0),
    units         TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL
) STRICT;
