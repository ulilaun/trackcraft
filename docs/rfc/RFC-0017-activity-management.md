# RFC-0017: Aktivitätenverwaltung

Status: Konsolidierte Regeln, Piggy 008.

## Anzeigen und bearbeiten

Titel/Datum/bekannte Distanz/Gesamtzeit mit Herkunft/Höhen/Trainingswerte anzeigen. Ohne GPS Geometriehinweis, fehlende Werte unbekannt statt null. Herkunft kennzeichnen, kein Unvollständigkeitshinweis.

Aktivität besitzt eigenen `user_id`-Verweis, automatisch aktueller fester Benutzer beim Import, keine Auswahl. Titel/optionale Beschreibung/optionaler Ort (freies Feld; ist er leer, belegt der Bearbeiten-Dialog einen Vorschlag vor, siehe „Ortsvorschlag“)/Sportklasse editierbar; keine Track-/Distanzbearbeitung. Start und Gesamtzeit beim Import einzugeben, wenn die Datei weder GPS-Punkte noch Zeitangaben enthält, und bei jeder Aktivität jederzeit korrigierbar (etwa falsche Geräteuhr). Messpunkte und Segmentversuche behalten die Gerätezeit, Originalwerte bleiben erhalten, Herkunft „manuell korrigiert“. Untertyp als Freitext bearbeitbar.

Sport nur eindeutige Quelle, sonst unknown; manuelle Korrektur mit Neuberechnung. Untertyp als Schlüssel aus dem Katalog der Sportklasse (Auswahl, „Neuer Untertyp …“ legt einen Katalogeintrag an); der Import setzt ihn nur über einen exakten Eintrag der Sportart-Zuordnung, sonst leer (RFC-0008). Titel beim Import aus der Datei, sonst Ersatztitel (RFC-0008). Nur Sportklasse trennt Bestenlisten.

## Import

[RFC-0008](RFC-0008-activity-import-and-processing-pipeline.md) führt Details. FIT je gewählter Session, GPX je Datei mit Abschnitten. Sessionliste alle vorausgewählt; identische FIT-Datei fehlende Sessions ergänzen, vorhandene gesperrt.

Andere vermutete Duplikate Vergleich/Kandidaten, kein Import/zusätzlich/ersetzen (alte Fassung bleibt nur in bestehenden Sicherungen). Ziel pro Vorgang nur einmal; ersetzende Aktivität behält die ID der alten. Relevante Konflikte vor Ausführung erneut prüfen, Änderungen erneut bestätigen, vorher keine Änderungen.

Vorausgewählt abwählbar Metadatenübernahme Titel/Beschreibung/Ort/Sportklasse; Untertyp ebenfalls übernehmbar. Duplikatvergleich mit Originalwerten.

Gewählte Sessions gemeinsam bis Abschluss speichern/ersetzen oder zurücknehmen. Browser bei unbestätigtem Konflikt schließen: kein Import, Upload bereinigen; bestätigte Jobs sicher abschließen/rücknehmen.

Import von Hintergrundauswertung trennen. Auswertungsfehler ohne Datei-Neuimport wiederholbar, keine Rücknahme erfolgreich gespeicherter Trainingsdaten.

## Löschen und Datenstände

Aktivität samt Messpunkten/Zuordnungen/Versuchen entfernen, PRs/Statistik neu berechnen, Segmente erhalten. Original erst nach letzter Referenz und sicherer Dateikoordination.

Historische Backups bleiben bis Retention, Restore kann Gelöschtes zurückbringen. Vollständiger Reimport nach vollständiger Entfernung; fehlende FIT-Sessions schon bei verbliebenen Geschwistern ergänzbar.

Erstellungs-/Änderungszeit und fachliche Revision führen. Tags/Bulkedit/Massenlöschung und manuelle Erfassung von Aktivitäten ohne Datei spätere Option (V2, RFC-0100)en. Kein Export/Original-/Backupdownload, keine Streak.

## Ortsvorschlag

#22, Uli 22.11.2026. Ist der Ort einer Aktivität mit GPS-Punkten leer, fragt der Bearbeiten-Dialog `GET /activities/{id}/location-suggestion` und belegt das Feld mit dem Vorschlag vor; ein Hinweis darunter nennt die Quelle. Wer das Feld ändert oder leert, behält seine Eingabe. Gespeichert wird erst mit „Speichern“.

1. **Eigene Daten**: Aktivitäten (Start = erster Punkt mit Position des ersten Abschnitts) und Strecken/Segmente (Startpunkt der Referenz) mit Ort, die höchstens 1,5 km vom Start der Aktivität beginnen. Es gewinnt der häufigste Ort, bei Gleichstand der nächste. Kein Netzzugriff.
2. **OpenStreetMap**: Ohne eigenen Treffer und nur, wenn unter Einstellungen → Orte erlaubt (Vorgabe an, `place_settings.osm_enabled`), fragt das Backend Nominatim (`[places] nominatim_url`, RFC-0021) per Rückwärtssuche. Gesendet wird nur die auf vier Nachkommastellen (gut 10 m) gerundete Startkoordinate; höchstens eine Anfrage pro Sekunde, Antworten bleiben bis zum Neustart zwischengespeichert. Name: Gemeinde (`village`, `town`, `city`, `municipality`) vor Ortsteil (`hamlet`, `suburb`). Fehler des Dienstes ergeben keinen Vorschlag.
