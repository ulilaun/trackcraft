-- 0003: Ansicht der Aktivitäten (M2).

-- Hat mindestens ein gespeicherter Messpunkt eine Position? NULL heißt noch nicht bestimmt; der Start füllt
-- das für vorhandene Aktivitäten aus den Messpunkten nach (calculation::backfill).
ALTER TABLE activities ADD COLUMN has_position INTEGER CHECK (has_position IS NULL OR has_position IN (0, 1));

-- Ein Hysteresewert je Aktivität und Berechnungsstand
CREATE INDEX idx_activity_calculations_activity ON activity_calculations (activity_id);
