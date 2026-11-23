# RFC-0018: Dashboard und Startseite

Status: Konsolidierte UI-Spezifikation (SportTracks.mobi Layout), Piggy 010.

## Dashboard-Layout & Kacheln

Das Dashboard ist als hochdichtes Zwei-Spalten-Grid aufgebaut:

1. **Linke Spalte (7 Spalten)** (Uli 23.11.2026):
   - **Diese Woche** oben: Einheiten und Zeit als „erledigt von geplant“ mit Fortschrittsbalken (ohne Plan nur Ist), dazu die Distanz. Je Tag ein Balken mit der erledigten Zeit, nach Sportklasse gestapelt, vor einem gestrichelt umrandeten Balken der geplanten Zeit (Trainingsplan E8, ausgelassene Einheiten zählen nicht). Darunter Sportklassen-Symbol und Wochentag, „Heute“ hervorgehoben.
   - **Haupt-Feed** darunter: die **letzten 5 Aktivitäten** als Kacheln. Links eine feste Mini-Karte der Spur (ohne Zoom und Verschieben, Klick öffnet die Aktivität; Spur und Kartenkacheln laden erst, wenn die Kachel ins Bild kommt), ohne GPS das Sport-Symbol auf getönter Fläche. Rechts Titel, Datum, Untertyp, optionaler Ort und die Kennzahlen Distanz, Gesamtzeit, Schnitt (Laufen Pace), Aufstieg, durchschnittliche Herzfrequenz; ist die Aktivität mit einer Plan-Einheit verknüpft, eine Marke „✓ Plan: Titel“.
   - Darunter der Link „Alle Aktivitäten“ zur Aktivitätenliste. Kein Nachladen im Feed.
2. **Aggregations-Widgets (Rechts, 5 Spalten)**:
   - **Diesen Monat**: KPI-Zahlenblöcke + Balkendiagramm der **Distanz** je Tag des laufenden Monats.
   - **Jahr** (Uli 23.11.2026, ersetzt die Saison): nur die Summen Einheiten, Zeit und Distanz des laufenden Kalenderjahres, ohne Diagramm.
   - Monat und Jahr zählen über **alle Sportklassen einschließlich `unknown`**. Die Monatsbalken sind nach Sportklasse gestapelt und in deren Farbe (RFC-0006), der Tooltip zeigt die Anteile. Tage ohne Aktivität haben keinen Balken. Es gibt keine Trendlinien (RFC-0009: keine Trends in V1).
3. **Quick-Upload-Zone**:
   - Header-Button `+ Aktivität importieren` und Drag-and-Drop-Zone für FIT-/GPX-Dateien. Beides öffnet den Importdialog (Sessionauswahl, Duplikatprüfung, Eingabe von Start und Gesamtzeit nach RFC-0008). Ohne Bestätigung wird nichts gespeichert.
