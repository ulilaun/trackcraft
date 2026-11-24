-- 0020: Kennzeichen der Demo-Datenbank (Uli 23.11.2026).
-- Nur die in der CI gebaute Demo-Datenbank hat hier eine Zeile (`trackcraft demo mark`). Mit `[demo] enabled = true`
-- verschiebt der Start alle Daten so, dass die letzte Aktivität gestern war; ohne diese Zeile verweigert er das,
-- damit echte Daten nie verschoben werden. `last_day` ist der lokale Tag der letzten Aktivität nach der letzten
-- Verschiebung.
CREATE TABLE demo_state (
    id         INTEGER PRIMARY KEY CHECK (id = 1),
    last_day   TEXT NOT NULL CHECK (date(last_day) = last_day),
    shifted_at_ms INTEGER NOT NULL
) STRICT;
