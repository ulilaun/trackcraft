# RFC-0017: Aktivitätenverwaltung

Status: Konsolidierte Regeln, Piggy 008.

## Anzeigen und bearbeiten

Titel/Datum/bekannte Distanz/Gesamtzeit mit Herkunft/Höhen/Trainingswerte anzeigen. Ohne GPS Geometriehinweis, fehlende Werte unbekannt statt null. Herkunft kennzeichnen, kein Unvollständigkeitshinweis.

Aktivität besitzt eigenen `user_id`-Verweis, automatisch aktueller fester Benutzer beim Import, keine Auswahl. Titel/optionale Beschreibung/optionaler Ort (freies Feld, in V1 nur manuell; Bestimmung aus den Startkoordinaten ist spätere Option, RFC-0100)/Sportklasse editierbar; keine Track-/Distanzbearbeitung. Start und Gesamtzeit beim Import einzugeben, wenn die Datei weder GPS-Punkte noch Zeitangaben enthält, und bei jeder Aktivität jederzeit korrigierbar (etwa falsche Geräteuhr). Messpunkte und Segmentversuche behalten die Gerätezeit, Originalwerte bleiben erhalten, Herkunft „manuell korrigiert“. Untertyp als Freitext bearbeitbar.

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
