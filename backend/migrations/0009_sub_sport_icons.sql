-- Schemaversion 9: Symbol je Untertyp (Uli 15.11.2026). NULL = Vorgabesymbol, das die Oberfläche aus
-- Sportklasse und Schlüssel ableitet; sonst die Kennung eines Symbols aus dem festen Satz (sports::ICONS).
ALTER TABLE sub_sports ADD COLUMN icon TEXT CHECK (icon IS NULL OR (length(icon) BETWEEN 1 AND 32 AND icon NOT GLOB '*[^a-z0-9_]*'));
