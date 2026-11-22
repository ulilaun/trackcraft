# ADR-0014: Backup und Wiederherstellung

Status: Akzeptierte Regeln, konsolidiert am 2026-10-29 (Piggy 007).

## Entscheidung

V1 enthält genau eine Sicherungsart: eine Vollsicherung in ein per TOML konfiguriertes Verzeichnis, wahlweise externes USB-Laufwerk, zweite Platte oder Netzwerkfreigabe (z. B. auf der DS214). Einzige Ausnahme: vor jeder Schemamigration eine lokale Vollsicherung (nur die letzte bleibt), zusätzlich ins Ziel kopiert, falls erreichbar; die Kopie zählt zu den fünf Ständen. Scheitert die lokale Migrationssicherung, startet das Backend nicht. Snapshots von Synology oder Windows (Schattenkopien, Dateiversionsverlauf) sind nicht Teil der Definition. Bei externem Ziel bleiben regelmäßige Trennung beziehungsweise Rotation und ein Wiederherstellungstest ohne die ursprünglichen Datenträger des TrackCraft-Hosts vorgesehen. Ein Ziel auf demselben Rechner (zweite Platte) schützt nicht gegen Verlust des Rechners; dieses Risiko ist zu dokumentieren.

Täglich initial 03:00 in der konfigurierten Anwendungszeitzone prüfen, nur bei geänderten Daten und vorhandenem beschreibbarem Ziel sichern. Ein verpasster heutiger Termin kann nach Backendstart einmal nachgeholt werden. „Jetzt sichern“ manuell auf der Seite Backup/Restore, mit denselben Prüfungen. Keine parallelen Läufe, übersprungene Versuche sind kein Erfolg.

Unverschlüsselt in V1, Sicherungsziel physisch und zugriffsseitig schützen. Nur letzte fünf erfolgreiche validierte vollständige Stände behalten; ältere erst nach erfolgreicher neuer Sicherung bereinigen.

## Konsistenter Stand

Schreibzugriffe kurz sperren, Lesen bleibt möglich. Datenbank und referenzierte Originale als unabhängigen unveränderlichen Zwischenstand bereitstellen; Sperre vor Übertragung ins Ziel freigeben. Manifest/Prüfsummen und separate wirksame TOML-Kopie beilegen. Keine Logs, temporären Daten oder rekursiven Backups.

## Restore

Im gestarteten leeren und gefüllten Backend aus konfiguriertem Serververzeichnis. Kein Browser-Upload. Vollständiger fachlicher Ersatz einschließlich Originalen, kein Merge. Warnung und ausdrückliche Bestätigung bei älterem oder nicht vergleichbarem Datenstand. Datenstandsidentität umfasst Bestands-/Revisions-/Vorgänger-ID und Änderungszähler, nicht nur Datum.

Vorprüfung, getrennte Bereitstellung und rücknehmbarer Wechsel verhindern Mischbestand und Verlust bei Fehler/Neustart. TOML-Referenzkopie nicht automatisch aktivieren. Keine zeitliche Restore-Vorgabe.

Aktivität löschen entfernt Originaldatei erst bei letzter Referenz; Backups behalten gelöschte Daten bis Retention. Restore kann diese zurückbringen. Benutzer-ID und Zuordnungen aus Sicherung erhalten, Profil aus aktueller TOML übernehmen, keine zusätzliche Namensbestätigung. Tatsächliche Profiländerung und TOML-Änderungen, die eine Neuberechnung auslösen, sicherungsrelevant; unveränderter Abgleich und reine Betriebswerte nicht.

Vollständige Abläufe und offene technische Nachweise: RFC-0012. Diese Regeln sind Anforderungen, keine Bestätigung vorhandener Implementierung.
