# RFC-0018: Dashboard und Startseite

Status: Konsolidierte UI-Spezifikation (SportTracks.mobi Layout), Piggy 010.

## Dashboard-Layout & Kacheln

Das Dashboard ist als hochdichtes Zwei-Spalten-Grid aufgebaut:

1. **Haupt-Feed (Links, 7 Spalten)**:
   - Chronologische Liste der letzten Aktivitäten mit Sportklassen-Icon, Titel, Untertyp, optionalem Ort (freies Feld) und Datum.
   - Integrierter KPI-Strip je Aktivität: Distanz, Gesamtzeit, Schnitt (Laufen Pace), Aufstieg, durchschnittliche Herzfrequenz.
   - Der Feed zeigt die **letzten 10 Aktivitäten**, darunter den Link „Alle Aktivitäten“ zur Aktivitätenliste. Kein Nachladen im Feed.
2. **Aggregations-Widgets (Rechts, 5 Spalten)**:
   - **Diesen Monat**: KPI-Zahlenblöcke + Balkendiagramm der **Distanz** je Tag des laufenden Monats.
   - **Saison** (M4a, Uli 13.11.2026): KPI-Zahlenblöcke + Balkendiagramm der **Distanz je ISO-Woche** der laufenden Saison (RFC-0009 „Saisons“). Liegt heute in keiner Saison, zeigt der Block die zuletzt beendete mit dem Hinweis „beendet“. Ohne solche Saison entfällt der Block. Der Saisonname verweist auf die Aktivitätenliste, gefiltert auf die Saison.
   - Kein Block „Diese Woche“ mehr: Mit der Saison wurde die Spalte so hoch, dass die Import-Ablage unter den Bildschirmrand rutschte (Uli 14.11.2026). `GET /stats/dashboard` liefert `week` weiterhin.
   - Monat und Saison zählen über **alle Sportklassen einschließlich `unknown`**. Die Balken sind nach Sportklasse gestapelt und in deren Farbe (RFC-0006), der Tooltip zeigt die Anteile. Tage ohne Aktivität haben keinen Balken. Es gibt keine Trendlinien und außer der Saison keine Zeiträume über den laufenden Monat hinaus (RFC-0009: keine Trends in V1).
3. **Quick-Upload-Zone**:
   - Header-Button `+ Aktivität importieren` und Drag-and-Drop-Zone für FIT-/GPX-Dateien. Beides öffnet den Importdialog (Sessionauswahl, Duplikatprüfung, Eingabe von Start und Gesamtzeit nach RFC-0008). Ohne Bestätigung wird nichts gespeichert.
