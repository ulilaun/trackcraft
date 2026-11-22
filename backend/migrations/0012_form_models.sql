-- 0012: Formkurve Stufe 2 und 3 (E7, Uli 22.11.2026).
-- `curve_model`: 'standard' (Fitness/Ermüdung als gleitende Mittel) oder 'two_curve' (Ermüdung getrennt aerob und
-- anaerob, Ruhetags-Bonus). Foster: Monotonie und Strain werden immer angezeigt; der Aufschlag auf die Ermüdung bei
-- Monotonie über 2,0 ist abschaltbar (Vorgabe aus).
ALTER TABLE load_settings ADD COLUMN curve_model TEXT NOT NULL DEFAULT 'standard' CHECK (curve_model IN ('standard', 'two_curve'));
ALTER TABLE load_settings ADD COLUMN aerobic_days INTEGER NOT NULL DEFAULT 5;
ALTER TABLE load_settings ADD COLUMN anaerobic_days INTEGER NOT NULL DEFAULT 12;
ALTER TABLE load_settings ADD COLUMN rest_bonus_pct INTEGER NOT NULL DEFAULT 18;
ALTER TABLE load_settings ADD COLUMN foster_surcharge INTEGER NOT NULL DEFAULT 0 CHECK (foster_surcharge IN (0, 1));
ALTER TABLE load_settings ADD COLUMN foster_surcharge_pct INTEGER NOT NULL DEFAULT 15;
