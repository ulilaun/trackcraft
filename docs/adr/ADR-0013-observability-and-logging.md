# ADR-0013: Observability und Logging

Status: Akzeptiert, konsolidiert am 2026-10-28.

Strukturierte Logs mit tracing/tracing-subscriber. Speicherpfad (`logs_dir`, Vorgabe `C:\ProgramData\TrackCraft\data\logs` bzw. `/data/logs`) und Log-Level aus TOML. Unter Windows gehen nur Startfehler vor dem Öffnen der Logdateien ins Ereignisprotokoll (RFC-0005). Unterstützte Ausgaben nach Logging-RFC: ERROR, WARN, INFO, DEBUG und TRACE. Genaue Freigabe/Redaktion sensibler Diagnosewerte ist noch auszuarbeiten.

Keine Passwörter, Tokens oder Sessioncookies protokollieren; kein App-Login in V1. Logging/Backup-Prüfmeldungen dürfen nicht selbst endlos neue fachliche Sicherungsstände auslösen.

Health, Readiness, Systeminformationen, Import-/Auswertungs-/Restorestatus und getrennte Sicherungsversuche/-erfolge nach RFC-0013.
