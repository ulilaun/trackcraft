-- 0016: Planungshilfe (E8 Stufe 2, #12, Uli 23.11.2026).
-- Einheiten aus einem Vorschlag tragen dessen Kennung; ein neuer Vorschlag ersetzt sie, solange sie offen und
-- unverändert sind. Jede Änderung über die API löscht die Kennung, die Einheit gilt dann als fest.
ALTER TABLE planned_units ADD COLUMN suggestion_id TEXT;
CREATE INDEX idx_planned_units_suggestion ON planned_units (suggestion_id) WHERE suggestion_id IS NOT NULL;

-- Zuletzt übernommene Eingaben der Planungshilfe als JSON (Minuten je Wochentag, Sportklasse, Anstieg, 3:1, Intensität).
ALTER TABLE plan_settings ADD COLUMN assistant TEXT;
