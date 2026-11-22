# RFC-0007: Backendarchitektur und Anwendungsschichten

Status: Konsolidierter Entwurf, 2026-10-29 (Piggy 005).

Rust, Axum, Tokio, Serde, SQLx, SQLite. Klassische Schichten: HTTP API -> Services -> Repositories -> SQLite, Domainmodelle unabhängig von HTTP.

## Module und Verantwortungen

- `api`: Routing, DTO-Abbildung, Requestvalidierung, Fehlercodes; keine Fachlogik.
- `services`: ActivityService, SegmentService, StatisticsService.
- `repositories`/`db`: reine DB-Zugriffe und Transaktionen.
- `domain`: fester Benutzer mit Besitzer-/Erstellerbezügen, Aktivitäten/Messpunkte, Segmente/Versuche.
- `geo`: Distanz, räumliche Indizes, Vereinfachung.
- `segment`: matcher, direction, scoring, leaderboard.
- `import`, `jobs`, `config`: Importnormalisierung, Operationskoordination, zentrale Parameter.

Geplante Jobs: ImportJob, SegmentHistoryJob, StatisticsJob, BackupJob und Wiederherstellungs-/Neuberechnungsoperationen. Noch kein abschließend entworfener persistenter Zustandsautomat.

## Import und Ergebnisse

Vorprüfung/Staging -> Sessionauswahl/Konfliktentscheidungen/ggf. Eingabe Start und Gesamtzeit -> erneute relevante Prüfung -> ggf. gesicherte Ersetzung -> Original und alle gewählten Aktivitäten/Messpunkte gemeinsam speichern -> Importabschluss -> wiederholbare Matching-/Statistikjobs. Identische FIT-Datei darf fehlende Sessions ergänzen, vorhandene Sessions bleiben unverändert und gesperrt. Ersetzungsziele je Import eindeutig halten; Ersetzen behält Aktivitäts-ID.

Dateiimport bis Abschluss alles oder nichts, einschließlich gewählter Sessionersetzungen und Rücknahme. Spätere Auswertungsfehler nehmen erfolgreiche Speicherung nicht zurück. Neue Segmente automatisch gegen Historie prüfen, Fortschritt sichtbar.

## Berechnungsversionen und Nachträge

Versionsstand aus Algorithmus und Rechenparametern (`algorithm_version`, `parameters_hash`). Nur deren Änderung erzeugt eine neue Gesamtversion (Job `recalculation`): alle Aktivitäten gegen alle Segmente neu matchen und ihren Aufstieg nach Hysterese neu berechnen, alter vollständiger Stand währenddessen veraltet (`stale`), Veröffentlichung gemeinsam am Ende. Keine doppelten Versuche, kein Wiederanlegen gelöschter Objekte. Umsetzung (M3b): Der neue Stand wird beim Start vollständig berechnet, bevor der Dienst Anfragen annimmt, und danach veröffentlicht; ältere Stände werden sofort gelöscht (C3). Nachträge (`activity_evaluation`, `segment_history`) arbeitet ein Hintergrund-Worker nacheinander ab, unterbrochene Aufträge werden beim Start erneut eingereiht.

Datenänderungen erzeugen keine neue Version, sondern einen Nachtrag im veröffentlichten Stand (Jobs `activity_evaluation`, `segment_history`). Die Versuche und der Hysteresewert (`activity_calculations`) der betroffenen Aktivität bzw. die Versuche des betroffenen Segments werden je Auswertung in einer Transaktion ersetzt; Ergebnisse eines veröffentlichten Stands sind damit veränderlich. Rang, Bestzeitenart und Zeitersparnis sind abgeleitet (RFC-0009) und brauchen keinen Nachtrag, ebenso kein Nachrechnen späterer Versuche. Während eines Nachtrags bleibt der Stand `current`, den Fortschritt zeigen `processing_status` der Aktivität bzw. der Job.

| Änderung | Nachtrag |
| :--- | :--- |
| Import, auch Nachimport aus der Vergangenheit oder fehlende FIT-Session | Aufstieg nach Hysterese berechnen, Aktivität gegen die Segmente ihrer Sportklasse matchen, beides einfügen |
| Ersetzen (Aktivitäts-ID bleibt) | Hysteresewert und Versuche der Aktivität ersetzen |
| Aktivität löschen | Versuche und Hysteresewert per `CASCADE` entfernt, kein weiterer Schritt |
| Sportklasse korrigieren | Versuche der Aktivität entfernen und gegen die Segmente der neuen Klasse matchen; bei `unknown` keine Versuche |
| Segment anlegen | Historie der Sportklasse matchen, Fortschritt sichtbar, Versuche gemeinsam am Ende einfügen |
| Segment löschen | Versuche per `CASCADE` entfernt |
| Start, Gesamtzeit oder Metadaten korrigieren | Kein Nachtrag, Versuche nutzen die Gerätezeit der Messpunkte (RFC-0009) |

Läuft eine Gesamtversion, gilt der Nachtrag für den veröffentlichten Stand, und die laufende Version wiederholt die Auswertung der betroffenen Aktivität bzw. des Segments, bevor sie veröffentlicht. Ohne veröffentlichten Stand wird nur die laufende Version ergänzt. Aufstieg und Abstieg von Segmenten sind nicht versioniert, sondern werden aus der Referenz abgeleitet (RFC-0008) und folgen einer geänderten Schwelle sofort nach dem Neustart. Persistenz, Deduplizierung und Wiederaufnahme der Jobs noch auszuarbeiten.

## Sicherung und Restore

Backup für konsistenten Zwischenstand kurze anwendungsweite Schreibsperre; aktive schreibende Vorgänge abschließen/koordinieren. Lesen möglich. Unabhängigen unveränderlichen Dateistand bereitstellen, dann Sperre lösen und ins konfigurierte Sicherungsziel übertragen.

Zeitplan, manueller Start und heutiges einmaliges Nachholen aus RFC-0012, gegen Doppelstart sichern.

Restore im laufenden leeren/gefüllten Backend: Vorprüfung/Staging, vorherigen Bestand rücknehmbar halten, Schreibsperre für Umschalten, nach Absturz kein Mischbestand. Bei ungeklärtem Zustand keine Schreibfreigabe. Vollständiger fachlicher Ersatz, Konfiguration getrennt.

## Konfiguration und Fehler

TOML allein, nur `TRACKCRAFT_CONFIG` zur Pfadauswahl; Startvalidierung vor Verarbeitung. Neustart für Änderungen, kein Reload. Fehlende Datei/Pflichtwerte, unbekannte Schlüssel, ungültige Kombinationen verhindern Start. Vollständiges Beispielschema noch zu erstellen. Profil aus `[user]` auf denselben DB-Benutzer abgleichen, nur tatsächliche Änderungen revisionieren. UTC-only, gemeinsame Konfigurationszone auch für Frontend.

AppError mit Validation, NotFound, Conflict, Internal als Ausgangsgruppen; sichere Clientfehler ohne interne Details. tracing, Unit-/Integrations-/E2E-Tests. Konkrete Job-/Datei-/Revisionspersistenz und sichere Zustandsübergänge bleiben technische offene Arbeit, nicht mit bereits existierendem Code verwechseln.
