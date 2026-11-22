-- 0004: Kategorie und Ort für Segmente und Strecken (M3d, Uli 12.11.2026).

-- Automatisch bestimmt (Segment: Anstieg/Abfahrt/Wellig, Strecke: Rundkurs/Hin & zurück/Strecke); NULL heißt noch
-- nicht bestimmt, der Start trägt sie nach (segments::store::backfill_categories)
ALTER TABLE segments ADD COLUMN category_auto TEXT
    CHECK (category_auto IS NULL OR category_auto IN ('climb', 'descent', 'rolling', 'loop', 'out_and_back', 'point_to_point'));
-- Von Hand gewählt, nur wenn abweichend von der automatischen
ALTER TABLE segments ADD COLUMN category_manual TEXT
    CHECK (category_manual IS NULL OR category_manual IN ('climb', 'descent', 'rolling', 'loop', 'out_and_back', 'point_to_point'));
-- Ort als freier Text, vor allem für Rundkurse
ALTER TABLE segments ADD COLUMN location TEXT;
