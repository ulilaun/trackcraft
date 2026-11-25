-- 0021: Sicherungsstatus und Vorgänge (#57; RFC-0012 „Status“, RFC-0001 offener Punkt 2, RFC-0003 offener Punkt 4).
--
-- `backup_state` (eine Zeile): letzte erfolgreiche Sicherung mit gesicherter Revision und Parameterstand und der
-- lokale Tag des letzten täglichen Versuchs, damit ein Neustart den heutigen Versuch nicht wiederholt.
--
-- `operations`: je Sicherungs- (später auch Restore-)Vorgang eine Zeile mit Status, Phase, Ergebnis und Fehler.
-- Der letzte Prüfversuch ist der zuletzt beendete Sicherungsvorgang. Ein beim Neustart noch offener Vorgang wird
-- als abgebrochen beendet; ein Sicherungsrest im Ziel bleibt dann als `.partial` liegen und wird später entfernt.
CREATE TABLE backup_state (
    id                        INTEGER PRIMARY KEY CHECK (id = 1),
    last_success_at_ms        INTEGER,
    last_success_name         TEXT,
    last_success_revision_id  TEXT,
    last_success_counter      INTEGER,
    last_success_parameters   TEXT,
    daily_last_date           TEXT CHECK (daily_last_date IS NULL OR date(daily_last_date) = daily_last_date)
) STRICT;

INSERT INTO backup_state (id) VALUES (1);

CREATE TABLE operations (
    id             TEXT PRIMARY KEY,
    kind           TEXT NOT NULL CHECK (kind IN ('backup', 'restore')),
    trigger        TEXT NOT NULL CHECK (trigger IN ('daily', 'manual')),
    status         TEXT NOT NULL CHECK (status IN ('queued', 'running', 'done', 'failed', 'rolled_back')),
    phase          TEXT,
    outcome        TEXT CHECK (outcome IS NULL OR outcome IN ('created', 'skipped_no_changes', 'skipped_target_unavailable', 'restored', 'failed')),
    error_code     TEXT,
    error_message  TEXT,
    -- Name des erstellten bzw. verwendeten Sicherungsstands
    backup_id      TEXT,
    started_at_ms  INTEGER NOT NULL,
    finished_at_ms INTEGER
) STRICT;

CREATE INDEX operations_kind_finished ON operations (kind, finished_at_ms);
