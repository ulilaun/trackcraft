-- 0014: Gewicht der Ermüdung in der Formkurve (#24, Uli 22.11.2026): Form = Fitness − Gewicht × Ermüdung.
-- Vorgabe 1,0 = bisheriges Verhalten; Vorlage „SportTracks-ähnlich“ setzt 21/12/0,75.
ALTER TABLE load_settings ADD COLUMN fatigue_weight REAL NOT NULL DEFAULT 1.0;
