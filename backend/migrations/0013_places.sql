-- 0013: Ortsvorschlag im Bearbeiten-Dialog (#22, Uli 22.11.2026). Zuerst eigene Daten (Aktivitäten und Strecken mit Ort
-- nahe dem Startpunkt), sonst OpenStreetMap (Nominatim). Der externe Abruf sendet die Startkoordinate und ist abschaltbar.
CREATE TABLE place_settings (
    id            INTEGER PRIMARY KEY CHECK (id = 1),
    osm_enabled   INTEGER NOT NULL DEFAULT 1 CHECK (osm_enabled IN (0, 1)),
    updated_at_ms INTEGER NOT NULL DEFAULT 0
) STRICT;
INSERT INTO place_settings (id) VALUES (1);
