-- 0023: Uhrzeit des täglichen Versuchs und Zahl der aufbewahrten Stände in der Oberfläche einstellbar (Uli 25.11.2026).
-- NULL heißt Vorgabe: Uhrzeit aus `[backup] daily_time` der TOML, Aufbewahrung fünf Stände (ADR-0014).
-- Eine geringere Zahl entfernt überzählige Stände erst nach der nächsten erfolgreichen Sicherung.
ALTER TABLE backup_state ADD COLUMN daily_time TEXT
    CHECK (daily_time IS NULL OR (length(daily_time) = 5 AND substr(daily_time, 3, 1) = ':'));
ALTER TABLE backup_state ADD COLUMN keep INTEGER CHECK (keep IS NULL OR keep BETWEEN 1 AND 30);
