# RFC-0013: Logging und Monitoring

Status: Konsolidierter Entwurf, 2026-10-29 (Piggy 008); Logdateien und Redaktion entschieden 2026-11-26 (#3).

tracing/tracing-subscriber, Browserkonsole für lokale Diagnose. Levels ERROR/WARN/INFO/DEBUG/TRACE; Konfiguration aus TOML. Ablage in `logs_dir` (Windows `C:\ProgramData\TrackCraft\data\logs`, Container `/data/logs`); unter Windows zusätzlich Startfehler vor dem Öffnen der Logdateien im Ereignisprotokoll (Anwendung, Quelle `TrackCraft`), unter Docker zusätzlich Ausgabe auf stdout. Keine externe Telemetrie.

## Logdateien

Entschieden 2026-11-26 (#3):

- Eine Datei für alles: `app.<JJJJ-MM-TT>.log`. Jede Zeile trägt Zeitpunkt, Level und Komponente; Fehler findet man per Suche nach `ERROR`/`WARN`. Getrennte `error.log`/`import.log` gibt es nicht.
- Tägliche Rotation; die 30 neuesten Tagesdateien bleiben, ältere löscht TrackCraft beim Rotieren. Tage ohne Logeintrag erzeugen keine Datei. Kein eigener TOML-Wert; wer länger aufbewahren will, sichert `logs_dir` selbst.
- Logs gehören nicht zur fachlichen Sicherung (RFC-0012).

## Was ins Log darf

Entschieden 2026-11-26 (#3). Die Logs bleiben auf dem eigenen Rechner; was hier erlaubt ist, gilt dafür. Wer ein Log weitergibt, sieht es vorher durch.

| Inhalt | Regel |
|---|---|
| Passwörter, Garmin-Schlüssel, Zugriffs-, Vorschau- und Zustands-Tokens, Sessioncookies | nie, auch nicht gekürzt |
| Positionen, Messpunkte, Strecken- und Segmentgeometrie | nie |
| Gesundheits- und Leistungswerte (Puls, Leistung, Gewicht, HF-Zonen, Belastung) | nie als Einzelwert; Anzahlen und Kennungen ja |
| Profilangaben (Name, Spitzname, Geburtstag) | nie |
| Dateinamen beim Upload, Datei- und Ordnerpfade | erlaubt, zur Diagnose nötig |
| IDs (Aktivität, Import, Sicherung, Operation), Fehlercodes, Anzahlen, Dauern | erlaubt |

Technische Absicherung: Strukturen mit Schlüssel- oder Tokenfeld leiten `Debug` nicht ab, sondern zeigen an dessen Stelle `<ausgeblendet>` (`logging::Hidden`). `backend/tests/redaction_tests.rs` prüft das und schlägt an, wenn eine Struktur mit Feld `api_key`, `token`, `*_token`, `password`, `secret` oder `cookie` `Debug` ableitet. Der Garmin-Schlüssel steht nur im HTTP-Header, nie in URLs. Neue Logaufrufe halten sich an die Tabelle; der Review prüft das.

## Diagnose

Zeitpunkt, Komponente, Fehlermeldung und ggf. Fehlerkette.

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
