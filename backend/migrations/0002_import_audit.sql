-- 0002: Importvorgänge mit Staging-Darstellung (RFC-0003 Abschnitt 8) und Audit-Protokoll (C11).

-- Geprüfte Sessions des Uploads (JSON), damit der Vorgang ohne erneutes Parsen angezeigt werden kann.
ALTER TABLE import_jobs ADD COLUMN sessions_json TEXT;
-- IDs der gespeicherten oder ersetzten Aktivitäten (JSON-Array).
ALTER TABLE import_jobs ADD COLUMN imported_activity_ids TEXT;

-- Aufräumen nach C12
CREATE INDEX idx_import_jobs_status ON import_jobs (status, created_at_ms);
CREATE INDEX idx_jobs_finished ON jobs (status, finished_at_ms);

CREATE TABLE audit_log (
    id          TEXT PRIMARY KEY,
    at_ms       INTEGER NOT NULL,
    action      TEXT NOT NULL CHECK (action IN ('import', 'replace', 'delete', 'backup', 'restore')),
    object_type TEXT NOT NULL CHECK (object_type IN ('activity', 'segment', 'import_job', 'backup')),
    object_id   TEXT,
    outcome     TEXT NOT NULL CHECK (outcome IN ('ok', 'failed')),
    details     TEXT
) STRICT;

CREATE INDEX idx_audit_at ON audit_log (at_ms);
