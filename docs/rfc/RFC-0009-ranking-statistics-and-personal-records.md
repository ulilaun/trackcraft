# RFC-0009: Ranglisten, Statistiken und Rekorde

Status: Konsolidierte Fachregeln, Piggy 008.

## Segmentwertung

Eigene gültige Versuche, Benutzer der Aktivität, Sportklasse muss Segment entsprechen. cycling/ebike/running/skiing getrennt, unknown ausgeschlossen; swimming und studio haben keine Segmente (Uli 14.11.2026). Untertypen trennen Bestenlisten nicht.

Gesamte verstrichene Zeit inklusive Pausen/Autopause. Start/Ziel auf ms runden, halbe ms aufrunden, Dauer Differenz. Anzeige Zehntelsekunden entsprechend runden, keine Messgenauigkeitszusage.

Sortierung aufsteigend:

1. duration_ms.
2. Tatsächlicher Startzeitpunkt.
3. Unveränderliche Aktivitäts-ID.
4. Stabile Versuchsposition innerhalb Aufzeichnung.

Nicht auf bei Neuberechnung neu erzeugte Ergebnis-IDs stützen. Letzte Tie-Breaks dienen stabiler Sortierung, nicht Leistungsverbesserung.

Monats-/Jahresbestzeiten nach lokalem interpoliertem Segmentstart aus der Gerätezeit der Messpunkte (eine manuelle Startkorrektur der Aktivität wirkt hier nicht), aktuelle Konfigurationszeitzone initial Europe/Berlin. Durchfahrt über Periodengrenze bleibt im Startzeitraum.

Pro Segment PR, zweite/dritte Leistung, Versuche, Durchschnitt, erster/letzter Versuch, Verlauf/Verbesserungen. Exakte Aggregations-/Abstandsdetails im Datenvertrag festlegen.

## Saisons

Saisons sind frei benannte Zeiträume, die der Benutzer auf der Statistikseite anlegt, ändert und löscht (Uli, 13.11.2026, RFC-0006 „Statistik“); andere Seiten wählen sie nur aus. Sie sind Daten in der Datenbank, keine Konfiguration, und kommen deshalb mit in die Vollsicherung.

- Eine Saison hat einen Namen (1–80 Zeichen, ohne Leerraum am Rand, eindeutig ohne Rücksicht auf Groß- und Kleinschreibung), einen ersten und einen letzten Tag. Beide Tage sind lokale Kalendertage in der Zeitzone der TOML, der letzte Tag zählt mit.
- Saisons gelten für alle Sportklassen und überschneiden sich nicht, auch nicht an einem einzigen Tag. Lücken zwischen Saisons sind erlaubt. Ändern prüft gegen die übrigen Saisons.
- Zuordnung wie bei Monat und Jahr: Aktivitäten nach lokalem Startdatum (bei Korrektur der korrigierte Start), Segmentversuche nach lokalem interpoliertem Segmentstart aus der Gerätezeit.
- **Vorgabesaison** ist die Saison, in der heute liegt. Liegt heute in keiner Saison, ist es die zuletzt beendete, angezeigt als „beendet“. Gibt es nur künftige Saisons oder gar keine, gibt es keine Vorgabesaison.
- Einsatz: Zeitraum der Segment-Bestenliste (Gesamt / Jahr / Saison, frühere wählbar, RFC-0006), Filter der Aktivitätenliste, Block im Dashboard (RFC-0018) und Statistikseite (M4b).
- Saisons haben keine abhängigen Daten, Löschen entfernt nur den Zeitraum. Sie lösen keine Neuberechnung aus, weil alle Saisonwerte bei Abfrage gebildet werden.

## Historische PRs

Nach tatsächlichen Versuchszeitpunkten, nicht Importreihenfolge. Nachimport kann historische PR-Folge verändern, ebenso Korrektur und Löschung.

Erster gültiger Versuch setzt Erste Bestzeit, kein PR-Verbesserungszähler und keine Zeitersparnis. Nur strikt kürzere Dauer neuer PR, gleiche Dauer Rekordeinstellung. Aktuelle Bestzeit und historische Verbesserung unterschiedliche Begriffe, UI entsprechend benennen.

Bestzeitenart (`record_kind`) und Zeitersparnis (`improvement_ms`) werden wie der Rang nicht gespeichert, sondern bei Abfrage abgeleitet. Je Segment die gültigen Versuche des veröffentlichten Stands nach Startzeitpunkt, Aktivitäts-ID und Versuchsposition ordnen und mit der kürzesten Dauer aller vorherigen Versuche vergleichen: kein vorheriger Versuch Erste Bestzeit, strikt kürzer Verbesserung mit Zeitersparnis gegenüber dieser Dauer, gleich Einstellung, sonst keine. Nicht gewertete Versuche haben keine Bestzeitenart. Nachimport, Korrektur und Löschung wirken so sofort auf die PR-Folge.

## Aktivitätsstatistik und Kennzahlen

Die Statistikseite zeigt Gesamtanzahl/Distanz/verfügbare Höhen/Zeit, Segmentzahl, Bestzeiten/PR-Verbesserungen mit eindeutigem Label sowie die Aktivitätsrekorde. Das Dashboard zeigt nur Feed, laufende Woche und laufenden Monat (RFC-0018). Zusammenfassungen je ISO-Kalenderwoche, Monat und Jahr mit Anzahl, Distanz, Gesamtzeit und verfügbarem Aufstieg, je Sportklasse (auch skiing, swimming, studio) und zusätzlich gesamt (unknown nur in gesamt). Untertypen wie Laufen/Gehen zählen in ihrer Klasse mit (Uli 14.11.2026). Keine Trends in V1; Auswertungen über beliebige Zeiträume/Sportarten spätere Option.

Statistiken und Aktivitätsrekorde werden bei jeder Abfrage aus den Aktivitäten berechnet und nicht gespeichert (C4, Uli 13.11.2026); gespeicherte Summen erst, wenn Messungen auf dem Referenzgerät es verlangen. Zusätzlich gibt es Zusammenfassungen je Saison.

Ganze Aktivität nach lokalem Startdatum (bei Korrektur der korrigierte Start), keine Mitternachtssplits. ISO-Wochen montags samt ISO-Wochenjahr. Jede Aktivität hat einen Start, kein Importdatum als Ersatz.

Zeitpunkte nur UTC; aktuelle TOML-Zeitzone für Kalender/Anzeige. Das gilt bewusst auch für Aktivitäten in einer anderen Zeitzone: Ein Lauf auf Reisen erscheint in der TOML-Zeit, nicht in Ortszeit. UTC-Differenzen, lokaler Tag nicht zwingend 24 h. Zonenwechsel ändert gespeicherte UTC-Zeitpunkte nicht, Kalender neu ableiten.

Kennzahlquelle [RFC-0008](RFC-0008-activity-import-and-processing-pipeline.md):

- Distanz: Dateiwert (vorhanden und größer 0) vor GPS-Berechnung auf WGS84 ohne Höhe.
- Gesamtzeit inklusive Pausen/Unterbrechungen als einzige Zeitkennzahl; Quelle Datei, sonst erster/letzter GPS-Punkt, sonst manuelle Eingabe beim Import.
- Durchschnitt Distanz/Gesamtzeit. Keine Bewegungszeit, kein Bewegungsdurchschnitt.
- Höhen: Dateiwert (vorhanden und größer 0), sonst Hysterese über die Messpunkte aus dem veröffentlichten Stand. Segmenthöhen aus der Referenz mit Hysterese (RFC-0008).

Unbekannt nicht null, Herkunft anzeigen.

## Aktivitätsrekorde

Je Benutzer und Sportklasse cycling/ebike/running/skiing (swimming und studio ohne Rekorde, Uli 14.11.2026), unknown ausgeschlossen bis Zuordnung. Die Statistikseite zeigt die Rekorde der gewählten Auswahl (Zeitraum, Sportklasse, Untertyp); ohne Einschränkung sind es die Rekorde aller Zeit (Uli, 13.11.2026). Jeder vorhandene Wert zählt, kein Vollständigkeitsbegriff:

- Längste Distanz.
- Größter Aufstieg.
- Längste Gesamtzeit.
- Höchster Durchschnitt (Distanz/Gesamtzeit), kein Mindestumfang.

Fehlender Wert schließt nur jeweilige Kategorie aus. Manuell eingegebene Gesamtzeit zählt wie ein Dateiwert. Nach Nachimport/Korrektur/Löschung neu berechnen. Vergleich gerundet wie angezeigt: Distanz auf 10 m, Aufstieg auf 1 m, Gesamtzeit auf 1 s, Durchschnitt auf 0,1 km/h, beim Laufen als Pace auf 1 s/km (kleinere Pace gewinnt, RFC-0006), halbe Werte aufrunden; dies ist zugleich die Anzeigegenauigkeit. Bei Gleichstand behält die frühere Aktivität (nach ggf. korrigiertem Start) den Rekord, die spätere gilt als Einstellung.

## Revision und Ausschlüsse

Versionierte Neuberechnung bei wirksamen Rechenparametern, keine bei reinen Betriebsparametern. Letzten vollständigen Stand veraltet zeigen, Ergebnisse je Auswertung gemeinsam veröffentlichen. Lösch-/Ungültigkeitsregeln vorrangig, keine doppelten/falsch versionierten Ergebnisse. Datenänderungen als Nachtrag im veröffentlichten Stand, neue Gesamtversion nur bei Algorithmus- oder Parameteränderung (RFC-0007).

Keine Streak-Funktion oder Definition aktiver Serientage in V1. Frühere Abstimmungen hierzu verworfen, keine Ersatzfunktion automatisch hinzugefügt.

Trainingsbelastungs-/Stressscore je Aktivität und daraus Fitnesszustand ausschließlich sehr späte Zukunftsidee, keine V1-Formel/Messwertpflicht.

Keine globalen Leaderboards/Clubs/Challenges/Achievements, FTP-/Herzfrequenzanalysen als V1-Zusage. Diagramme mit Recharts (ADR-0004).
