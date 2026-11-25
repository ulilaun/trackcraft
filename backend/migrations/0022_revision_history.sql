-- 0022: Verlauf der Datenstandsrevisionen (#57, RFC-0012 „Ausfallsicherheit und Vergleich“).
-- Jede neue Revision in `data_state` hinterlässt hier eine Zeile. Damit lässt sich beim Wiederherstellen belegen, ob
-- ein gesicherter Stand ein Vorgänger des aktuellen ist (älter) oder umgekehrt (neuer); Zeitstempel oder ein höherer
-- Zähler allein sind kein Beweis. Revisionen vor dieser Migration sind nicht verzeichnet, Sicherungen daraus gelten
-- als nicht vergleichbar. Der Verlauf gehört zum Datenstand und wird mitgesichert.
CREATE TABLE revision_history (
    revision_id             TEXT PRIMARY KEY,
    predecessor_revision_id TEXT,
    change_counter          INTEGER NOT NULL CHECK (change_counter >= 0),
    created_at_ms           INTEGER NOT NULL
) STRICT;

INSERT INTO revision_history (revision_id, predecessor_revision_id, change_counter, created_at_ms)
SELECT revision_id, predecessor_revision_id, change_counter, updated_at_ms FROM data_state WHERE id = 1;
