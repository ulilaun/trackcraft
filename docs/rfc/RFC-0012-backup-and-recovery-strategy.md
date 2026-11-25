# RFC-0012: Sicherung und Wiederherstellung

Status: Konsolidierte Regeln und technischer Entwurf, Piggy 007.

## Inhalt und Ziel

Vollsicherung: konsistente SQLite-DB mit Benutzer/Zuordnungen/Fachdaten/Datenstandsidentität, alle referenzierten Originale (gemeinsame FIT einmal), Manifest mit Formatkennung `trackcraft-backup` (unveränderlich, RFC-0021) und Formatversion, Prüfsummen und separate Kopie wirksamer TOML.

Keine Logs/temporären Import-/Operationsdateien/unreferenzierten oder zurückgewiesenen Uploads/anderen Sicherungen. Keine Rekursion. Prüfsumme beweist keine Herkunft.

Genau eine Sicherungsart in V1: diese Vollsicherung in ein per TOML konfiguriertes Sicherungsverzeichnis, wahlweise externes USB-Laufwerk, zweite Platte oder Netzwerkfreigabe (unter Windows als UNC-Pfad, z. B. auf der DS214, RFC-0005). Einzige lokale Ausnahme ist die Sicherung vor Schemamigration (siehe unten). Snapshots von Synology oder Windows sind nicht Teil der Definition. Das aktive FIT/GPX-Archiv (`activities_dir`) ist keine Sicherung.

Bei externem Ziel regelmäßige Trennung/Rotation und Restoretest ohne die ursprünglichen Datenträger des TrackCraft-Hosts. Ein Ziel auf demselben Rechner schützt nicht gegen Verlust des Rechners; Risiko dokumentieren.

Das Sicherungsformat ist plattformunabhängig: Pfade im Manifest und in der Datenbank sind relativ mit `/` als Trenner, eine unter Windows erstellte Sicherung lässt sich unter Docker wiederherstellen und umgekehrt. Die TOML-Referenzkopie enthält plattformabhängige Pfade und wird ohnehin nicht automatisch aktiviert.

Letzte fünf erfolgreiche validierte vollständige Stände unabhängig vom Alter; überzählige älteste erst nach erfolgreichem neuen Stand entfernen. Fehler/Überspringen keine Retention-Löschung, weniger als fünf alle behalten.

Unverschlüsselt, Sicherungsziel physisch und zugriffsseitig schützen: Trainings-/Positions-/Gesundheits-/Profildaten sowie Konfiguration lesbar. HTTPS unabhängig Pflicht.

Fachliches Löschen verändert historische Sicherungen nicht rückwirkend; Restore kann Gelöschtes zurückbringen.

## Automatischer Sicherungsversuch

Initial täglich 03:00, konfigurierte Anwendungszeitzone. Nach lokalem konsistentem Stand prüfen: sicherungsrelevante Änderung seit erfolgreicher Sicherung und Ziel vorhanden/beschreibbar?

Nur dann sichern. Keine Änderung/Ziel fehlt: überspringen ohne Erfolg vorzutäuschen. Erste Sicherung ohne Basis ausstehend. Bearbeiten/Löschen und tatsächliche Benutzerprofiländerungen zählen, unveränderter Profilabgleich/Logs/Backupprüfungen nicht.

TOML-Änderungen sind sicherungsrelevant, wenn sie eine Neuberechnung auslösen (Matching-, Höhen-Hysterese- und Zeitzonenparameter, siehe RFC-0021); reine Betriebswerte wie Log-Level oder Backupuhrzeit nicht.

Nur validierter Erfolg aktualisiert gesicherte Revision. Änderungen während Übertragung bleiben ausstehend. Ein alter Stand ohne spätere Änderungen nicht allein wegen Alter fachlich veraltet.

Geplanter Versuch höchstens einmal je lokalem Tag. Backendstart nach Uhrzeit: heute wegen Nichtlauf verpassten Versuch einmal nachholen, nicht alle vergangenen Tage. Schon geprüfte Zustände Ziel fehlt/keine Änderung nicht durch Neustart wiederholen. Persistenter Tages-/Operationszustand.

## Sicherung vor Schemamigration

Vor jeder Migration immer eine Vollsicherung, auch ohne Datenänderung. Sie wird lokal in einem per TOML konfigurierten Pfad abgelegt; lokal bleibt nur die letzte. Ist das Sicherungsziel erreichbar, zusätzlich dorthin kopieren; diese Kopie zählt zu den fünf Ständen. Scheitert die lokale Sicherung, bricht der Start ab und es wird nicht migriert. Scheitert nur die Kopie, protokollieren und migrieren.

## Manuell und konsistent

Jetzt sichern auf der Seite Backup/Restore, dieselben Änderungs-/Zielprüfungen, kein neuer Stand ohne Änderung. Manueller Versuch verbraucht Tagesversuch nicht, erfolgreicher aktualisiert Revision. Keine parallelen Läufe, Status/Ergebnis sichtbar.

1. Bedingungen prüfen.
2. Schreibende Services/Jobs koordinieren, laufende Änderungen geordnet abschließen, kurze Schreibsperre; Lesen möglich.
3. SQLite-konsistenten DB-/Originalstand unabhängig und unveränderlich bereitstellen.
4. Sperre lösen; spätere aktive Dateilöschung darf Zwischenstand nicht beschädigen.
5. Ohne Schreibsperre ins Sicherungsziel kopieren, Manifest/Prüfsumme/Integrität prüfen.
6. Erfolgsstand fortschreiben, dann Retention.

Keine hängende Sperre/falscher Erfolg bei Fehler. SQLite/WAL, Stagingplatz, Kopie/Snapshot, Hashformat und Bereinigung noch technisch ausarbeiten.

## Restoreauswahl und Wirkung

Im gestarteten fachlich leeren oder gefüllten Backend verfügbar; initialisierte Systemtabellen allein kein fachlicher Bestand. Fester Benutzer und Leerbestandsdefinition noch technisch abstimmen.

Auswahl nur aus TOML-konfiguriertem Verzeichnis (USB, zweite Platte oder Netzwerkfreigabe). Sicherungsreferenz/Datenstand/Prüfstatus zeigen, unzugänglich von leer unterscheiden. Kein Browserupload/beliebiger Pfad/Download.

Vollständiger fachlicher Ersatz mit Originalen, kein Merge. Fehlende aktuelle Fachdaten nicht mehr aktiv. Reguläre Sicherungen und Betriebs-TOML nicht automatisch ersetzen/löschen.

Vorschau Wirkung und aktuelle/gesicherte Revision. Älterer/nicht vergleichbarer Stand Warnung und ausdrückliche Zustimmung, gebunden an Sicherung und geprüften aktuellen Stand. Zwischenzeitliche Änderung erneut prüfen.

Benutzer-ID/Zuordnungen aus Sicherung erhalten. Profilfelder aus aktueller TOML auf diesen Benutzer übernehmen, keine zusätzliche Bestätigung wegen Namens-/Profilabweichung. Tatsächliche dadurch erzeugte Profiländerung sicherungsrelevant. Keine neue Benutzeridentität aus Namen ableiten.

## Ausfallsicherheit und Vergleich

Vorprüfung Integrität/Formatkompatibilität/DB-/Originalkonsistenz und sichere Pfade, vollständig getrennt bereitstellen. Alten Bestand bis Abschluss rücknehmbar halten. Beim Umschalten Schreibzugriffe/Jobs sperren.

Bei Fehler alten vollständigen Stand erhalten/automatisch herstellen; vorher leeres Backend bleibt leer. Neustart persistenter Operationszustand: vollständig alt oder wiederhergestellt, nie Mischung. Funktionsprüfung vor Freigabe, ungeklärte Rücknahme kein Erfolg/keine Schreibfreigabe. Kein Restorezeitlimit.

Bestands-ID, Revisions-ID, Vorgänger-ID, Änderungszähler. Bekannter Vorgänger älter, gleiche bestätigte Revision gleicher Stand, unterschiedliche/nicht belegte Linien nicht vergleichbar. Zeitstempel/höherer Zähler allein kein Beweis. Nach Restore neue eindeutige Revisionen vom Sicherungsstand. Schema/Berechnungsversion separat.

## Status und technische Restarbeit

Letzter Prüfversuch/Grund, letzte erfolgreiche Sicherung samt Revision, ausstehende Änderungen und laufender Vorgang. Erfolg/keine Änderungen/Ziel fehlt oder nicht beschreibbar/Fehler unterscheiden.

Kein garantiertes Datenverlustfenster: Änderungen seit der letzten erfolgreichen Sicherung können verloren gehen, ein USB-Ziel kann fehlen, ein Ziel auf demselben Rechner geht bei dessen Verlust mit verloren.

Umgesetzt mit #57, siehe „Umsetzung“. Offen: eine Platzprüfung vor dem Bereitstellen (bisher scheitert das Kopieren ohne Folgen, wenn der Platz fehlt). Restore unter anderen Berechnungsparametern technisch mit versionierter Neuberechnung koordinieren.

Cloudziele/automatische Restoretests spätere Optionen, keine Zusage. Keine Exportfunktion durch Betriebsbackup.

## Umsetzung (#57, Schemaversion 21)

Entscheidungen Uli 25.11.2026: ein Ordner je Stand, Wiederherstellen per internem Neustart, Umsetzung in zwei Schritten
(erst Sicherung, dann Restore).

**Format `trackcraft-backup`, Version 1.** Je Stand ein Ordner `trackcraft-<JJJJMMTT>-<HHMMSS>-s<schema>-c<zähler>`
(Ortszeit der Anwendungszeitzone, Schemaversion, Änderungszähler aus `data_state`) mit `trackcraft.db` (per `VACUUM
INTO`), `trackcraft.toml` (Referenzkopie, nie automatisch aktiviert), `originals/<relativer Pfad>` und `manifest.json`.
Das Manifest nennt Formatkennung und -version, Erstellzeit, Anlass (`daily`, `manual`, `migration`), App- und
Schemaversion, den Parameterhash der Neuberechnung samt Zeitzone, den Datenstand (Bestands-, Revisions-,
Vorgänger-ID, Zähler), Anzahl Aktivitäten und Segmente und je Datei relativen Pfad (`/`), Größe und SHA-256. Eine neuere
Formatversion lehnt das Programm ab.

**Ablauf.** Ziel vorhanden und beschreibbar? (Der Zielordner wird nie angelegt: fehlt er, fehlt das Laufwerk.)
Änderung seit der letzten erfolgreichen Sicherung (andere Revision oder anderer Parameterstand)? Dann belegt der
Vorgang die einzige schreibende Verbindung, erzeugt die Datenbankkopie und harte Links auf die referenzierten Originale
im Staging (gleiches Laufwerk, sonst Kopie) und gibt die Verbindung wieder frei. Ohne Sperre werden die Originale
gegen ihre gespeicherte Prüfsumme gehasht, das Manifest geschrieben, alles nach `<name>.partial` im Ziel kopiert, dort
erneut gelesen und gegen das Manifest geprüft und erst dann umbenannt. Danach Aufbewahrung: die neuesten fünf
gültigen Stände bleiben; ungültige oder fremde Ordner werden nicht angefasst, `.partial`-Reste entfernt.

**Sicherung vor Migration** im selben Format in `migration_dir` als `trackcraft-vor-migration-v<alt>-v<neu>` (nur die
letzte bleibt), Originale dort als harte Links; ist das Ziel erreichbar, zusätzlich als regulärer Stand dorthin kopiert
(zählt zu den fünf). Die Kopie ins Ziel darf scheitern, die lokale Sicherung nicht.

**Zustand.** `backup_state` hält die letzte erfolgreiche Sicherung (Zeit, Stand, Revision, Zähler, Parameterstand) und
den Tag des letzten täglichen Versuchs; ein Neustart nach der Uhrzeit holt den heutigen Versuch einmal nach, nicht
öfter. `operations` hält jeden Vorgang (`queued`, `running`, `done`, `failed`) mit Phase (`checking`, `staging`,
`copying`, `cleanup`) und Ergebnis (`created`, `skipped_no_changes`, `skipped_target_unavailable`, `failed`); beim
Start offene Vorgänge enden als `failed` mit `operation_interrupted`. Erfolg und Fehler schreiben einen Audit-Eintrag.
Vorgänge werden wie das Audit-Protokoll ein Jahr aufbewahrt.

**API.** `GET /backups/status`, `GET /backups`, `POST /backups` (Jetzt sichern, 202 mit Vorgang), `GET /operations/{id}`.
Die Liste prüft Manifest, Vorhandensein und Größe der Dateien; die Prüfsummen werden beim Erstellen im Ziel und vor
einem Restore geprüft.

**Wiederherstellen** (Schemaversion 22, Teil 2). Vergleich über `revision_history` (jede Revision mit Vorgänger, auch im
Stand gesichert): gleich, wenn die Revisionen übereinstimmen; älter, wenn die Revision des Stands im Verlauf der laufenden
Daten steht; neuer, wenn die laufende Revision im Verlauf des Stands steht; sonst nicht vergleichbar (auch andere
Bestands-ID oder Stand ohne Verlauf). Zustimmung bei älter und nicht vergleichbar, außer der Bestand ist leer.
Vorschau-Token = Stand-ID + Hash aus Stand-ID, SHA-256 des Manifests und laufender Revision; stimmt es nicht mehr, 409.

Ablauf: Vollprüfung im Ziel (Prüfsummen, `integrity_check`, Originale gegen Datenbank) → Kopie ins Staging mit erneuter
Prüfung → dort migrieren, `backup_state` und `operations` der laufenden Datenbank übernehmen, neue Revision vom Stand aus,
Audit → Merker `database_dir/wiederherstellung.json` → interner Neustart: Server und Hintergrundaufgaben enden, Datenbank
wird geschlossen, Sperrdatei frei, der Start läuft erneut im selben Prozess. Beim Start vor dem Öffnen der Datenbank
schaltet `apply_pending` in Schritten um, jeder im Merker vermerkt und wiederholbar: alte Datenbank und alte Originale nach
`database_dir/vor-wiederherstellung/`, neue Originale und Datenbank an ihren Platz. Hat sich die laufende Revision vor dem
Umschalten geändert, wird nichts ersetzt. Danach Start mit Funktionsprüfung (`quick_check`, Anzahl Aktivitäten wie im
Manifest, alle Originale vorhanden); scheitert sie oder der Start, nimmt `rollback` anhand des Schritts zurück und der Start
läuft mit dem alten Bestand (Vorgang `rolled_back`). Ein Absturz mitten im Umschalten setzt beim nächsten Start fort. Der
alte Bestand bleibt bis zum nächsten Wiederherstellen liegen. Regelmäßige Sicherungen, Migrationssicherung und TOML werden
nicht ersetzt.
