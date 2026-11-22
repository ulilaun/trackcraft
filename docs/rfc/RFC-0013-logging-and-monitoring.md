# RFC-0013: Logging und Monitoring

Status: Konsolidierter Entwurf, 2026-10-29 (Piggy 008).

tracing/tracing-subscriber, Browserkonsole für lokale Diagnose. Levels ERROR/WARN/INFO/DEBUG/TRACE; Konfiguration aus TOML. Ablage in `logs_dir` (Windows `C:\ProgramData\TrackCraft\data\logs`, Container `/data/logs`); unter Windows zusätzlich Startfehler vor dem Öffnen der Logdateien im Ereignisprotokoll (Anwendung, Quelle `TrackCraft`), unter Docker zusätzlich Ausgabe auf stdout. Bisher app.log/error.log/import.log und 30 Tage als Betriebsentwurf. Rotations-/Redaktionsdetails noch auszuarbeiten.

Diagnose: Zeitpunkt, Komponente, Fehlermeldung und ggf. Stacktrace; keine Passwörter/Tokens/Sessioncookies. Schutz von Profilangaben sowie Positions-/Gesundheitsdetails in Logs noch definieren. Keine externe Telemetrie.

## Zustände

- Health `/api/v1/health`: Backend, SQLite, Dateisystem/Pfade, Speicher.
- Readiness `/api/v1/ready`, System `/api/v1/system`, Version `/api/v1/version`.
- Import und Auswertung getrennt, laufende/fehlgeschlagene Operationen, sichere Rücknahme/Restorestatus.
- Versionierter Berechnungsstand, veraltet/laufend/Fehler, letzte vollständig veröffentlichte Ergebnisse.
- Uptime, Version, Plattform (Windows-Dienst oder Container), Version der Zeitzonendaten, Speicherverbrauch; Aktivitäten/Segmente.
- Ablaufdatum des TLS-Zertifikats, wenn das Backend TLS selbst terminiert; Warnung 30 Tage vor Ablauf.
- Antwort-/Import-/Matchingzeiten messen; keine unbewiesenen Performancezusagen.

## Sicherung

Nicht nur „letztes Backup“: letzten Prüfversuch mit Ergebnis und letzte erfolgreiche Sicherung samt Datenstand getrennt zeigen. Erfolg, keine Änderungen, Ziel fehlt/nicht beschreibbar und Fehler unterscheiden. Ungesicherte Änderungen und laufenden Auftrag anzeigen. Ungesicherte Änderungen erscheinen als Warnung `backup_pending` mit Zeitpunkt der ersten ungesicherten Änderung (Gestaltung RFC-0006).

## Warnungen

Backup-/Import-/Auswertungs-/Wiederherstellungsfehler, geringer Speicherplatz (unter 1 GB frei auf Datenbereichen oder Sicherungsziel, TOML). Abgewiesene Uploads als Logeintrag. Fehlende Aktivitätswerte als unbekannt anzeigen, nicht als 0. Prüfmeldungen/Logs und unveränderter Profilabgleich dürfen nicht selbst neue fachliche Sicherungsrevisionen erzeugen.

Prometheus/Grafana/OpenTelemetry/externe Benachrichtigungen nur spätere Optionen. Exakte Health-/Readinessantworten und Zustandsmetadaten in OpenAPI noch auszuarbeiten.
