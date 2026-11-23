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

Keine globalen Leaderboards/Clubs/Challenges/Achievements, FTP-Analysen als V1-Zusage. Diagramme mit Recharts (ADR-0004).

## Belastung und Form

Seit Schemaversion 11 (Entscheidungen Uli 22.11.2026). Leistung (Watt) liegt kaum vor, daher rechnet TrackCraft die Belastung für alle Sportarten aus der Herzfrequenz.

**HF-Zonen** (`hr_zone_sets`, Kontoseite „Herzfrequenz“): maximale HF, Ruhepuls, optional Schwellen-HF; Zonenmodell % der max. HF, % der HF-Reserve (Karvonen, Vorgabe) oder % der Schwellen-HF (Friel, nur mit Schwellen-HF); fünf untere Grenzen in Prozent, aufsteigend, Vorgaben je Modell. Jeder Satz gilt ab einem lokalen Datum. Ein allgemeiner Satz gilt für alle Sportklassen; eine Sportklasse kann eigene Sätze haben, die ab ihrem Datum vorgehen. Vor dem ältesten allgemeinen Satz gilt dieser rückwirkend. Vorschlag für die maximale HF aus dem Geburtstag nach Tanaka (208 − 0,7 × Alter). Anaerob zählt ab der Schwellen-HF, ohne sie ab Beginn von Zone 4.

**Belastung je Aktivität** (`activity_loads`, abgeleitet): Banister-TRIMP aus den Messpunkten, Σ Minuten × x × 0,64 × e^(k·x) mit x = Anteil der HF-Reserve (auf 0–1 begrenzt) und k = 1,92 bzw. 1,67 (Einstellung „Faktor der Belastungsformel“). Es zählt jedes Intervall zwischen zwei Messpunkten mit HF am ersten Punkt; Lücken über 60 s gelten als Pause. Dazu die Zeit in den Zonen (unter Zone 2 zählt zu Zone 1) und die Aufteilung aerob/anaerob, damit ein späteres Zwei-Kurven-Modell ohne neue Berechnung möglich ist. Hat die Datei nur eine Durchschnitts-HF, gilt sie für die ganze Dauer (`average`). Ohne HF oder ohne jeden Zonensatz wird geschätzt: Dauer × Belastung je Stunde der Sportklasse (`estimated`, einstellbar, Vorgaben Rad 50, E-Bike 35, Laufen 65, Ski 55, Schwimmen 60, Studio 40, Unbekannt 45). Jede Zeile trägt einen Hash ihrer Eingaben (Formelversion, Zonensatz, Faktor, Revision der Aktivität); weicht er ab, rechnet ein Hintergrunddienst nach dem Start, nach Änderungen und alle 30 s neu. Zonensätze und Einstellungen sind sicherungsrelevant, die Belastung selbst nicht.

**Formkurve** (Seite „Form“): Tagesbelastung nach lokalem Datum; Fitness und Ermüdung als exponentiell gleitende Mittel mit den Zeitkonstanten 42 und 7 Tagen (einstellbar, Ermüdung kürzer als Fitness), Form = Fitness − Ermüdung des Vortags. Zustand der Form: über +25 erholt, +5 bis +25 frisch, −10 bis +5 ausgeglichen, −30 bis −10 im Aufbau, darunter Überlastungsgefahr. Vorschau ab morgen (Vorgabe 28 Tage, gestrichelt): „Ruhe“ ohne Belastung oder „wie bisher“ mit dem Mittel der letzten 28 Tage; die spätere Trainingsplanung ersetzt diese Annahme durch geplante Einheiten. Zeitraum wie in der Aktivitätenliste (Gesamt, 12 Wochen, Jahr, Saison, Von–Bis), dazu Wochenbalken aerob/anaerob/geschätzt.

**Gewicht der Ermüdung und Vorlagen** (#24, Schemaversion 14): Form = Fitness − Gewicht × Ermüdung, Gewicht 0,25 bis 3 (Vorgabe 1,0). Die Einstellungen haben zwei Vorlagen: „Standard“ (Mittel, 42/7 Tage, Gewicht 1,0) und „SportTracks-ähnlich“ (21/12 Tage, Gewicht 0,75). SportTracks rechnet Banister-Summen ohne Mittelung (aus seinen Diagrammwerten nachgerechnet: τ 21/12, Erschöpfung ≈ 1,28 × Fitness); eine Summe ist das gleitende Mittel mal 1/(1−e^(−1/τ)), in unserer Mittel-Darstellung entspricht das 21/12 mit Gewicht ≈ 0,75. Absolute Zahlen sind nebensächlich, es zählen Verlauf und Verhältnis (Uli 22.11.2026). Mit Gewicht unter 1 ist die Form bei gleichmäßigem Training positiv. Eine automatische Anpassung an Segmentzeiten gibt es bewusst nicht: Ein Probelauf ergab eine nachweisbare Fitness (τ₁ ≥ ~40 Tage), aber keine bestimmbare Ermüdung.

**Aufbau der Seite** (#25, Uli 23.11.2026): Zeitraum und Vorschau oben, darunter getrennte Diagramme, breit zwei Spalten, schmal untereinander: „Belastung“ (Fitness und Ermüdung als Flächen), „Form“ (Fläche, über null grün, darunter orange), „Monotonie & Strain“ (Monotonie links, Achse bis 4, höhere Werte am Rand markiert, Warnbereich ab 2,0 rot; Strain rechts als Fläche) und die Wochenbalken. Die drei Tagesdiagramme teilen die Zeitachse: Überfahren zeigt überall denselben Tag. Über jedem Tagesdiagramm eine Kachelreihe wie bei SportTracks: aktueller Wert und Änderung gegenüber vor einem Monat (30 Tage), vor einer Woche sowie in einer Woche mit der gewählten Vorschau (ohne Vorschau leer); für die Belastung je eine Zeile Fitness und Ermüdung. Im Zwei-Kurven-Modell sind die Teile der Ermüdung ausgeblendet, bis ein Schalter sie einblendet; die Wahl merkt sich der Browser.

**Zwei-Kurven-Modell** (Stufe 2, seit Schemaversion 12, in den Einstellungen der Formkurve umschaltbar, Vorgabe Standard): Die Ermüdung besteht aus einem aeroben Teil (aerobe plus geschätzte Belastung, Zeitkonstante 5 Tage) und einem anaeroben Teil (12 Tage); an Tagen ohne Belastung sind beide Zeitkonstanten um den Ruhetags-Bonus kürzer (Vorgabe 18 %). Form = Fitness − (aerob + anaerob) im gewohnten Maßstab, die Zustandsgrenzen gelten weiter (Uli 22.11.2026: keine Gewichtung k₂ auf den Mittelwert-Kurven, sonst wäre die Form bei gleichmäßigem Training dauerhaft negativ). Die Seite zeigt beide Teile auf Wunsch als eigene Linien (#25).

**Monotonie und Strain** (Stufe 3, Foster): über die letzten 7 Tage einschließlich heute, fehlende Tage als 0; Monotonie = Mittel / Standardabweichung der Tagesbelastung, Strain = Wochensumme × Monotonie; ohne Belastung oder ohne Streuung kein Wert. Ab Monotonie 2,0 warnt die Seite. Optional (Vorgabe aus) erhöht ein Aufschlag (Vorgabe 15 %) an solchen Tagen die Belastung, die in die Ermüdung eingeht; die Fitness bleibt unberührt.

## Trainingsplanung

E8 Stufe 1 (#12, Schemaversion 15, Entscheidungen Uli 23.11.2026, Mockup abgenommen) Stufe 2 Planungshilfe (Schemaversion 16) sowie wiederkehrende Einheiten und Wochenvorlagen (Schemaversion 17), siehe unten. Eine automatische Anpassung des Modells ist nicht vorgesehen (siehe „Gewicht der Ermüdung“).

**Geplante Einheiten** (`planned_units`, getrennt von den Aktivitäten): Datum, Sportklasse (ohne „unbekannt“), Untertyp, Titel, Notiz und die Belastung entweder als **Minuten je HF-Zone** oder als **Zielbelastung** mit Dauer und Anteil „intensiv“ (Vorgabe 0 %). Gespeichert werden nur die Eingaben; die Belastung rechnet TrackCraft beim Lesen mit dem Zonensatz, der am geplanten Tag für die Sportklasse gilt: Banister-TRIMP mit der HF in der Zonenmitte (oberste Zone: Mitte zwischen Untergrenze und max. HF), Zonen mit Mitte ab der Anaerob-Grenze zählen anaerob. Ohne Zonensatz werden Zonenminuten aus der Dauer geschätzt (Belastung je Stunde der Sportklasse). Bei Zielbelastung ist „intensiv“ der anaerobe Anteil.

**Status**: geplant, ausgelassen oder erledigt. Erledigt heißt: mit einer Aktivität verknüpft (`activity_id`, eine Aktivität gehört zu höchstens einer Einheit). Wird die Aktivität gelöscht, ist die Einheit wieder geplant. Die Aktivitätsseite schlägt offene Einheiten desselben Tags und derselben Sportklasse zum Verknüpfen vor; „Nicht verknüpfen“ blendet einen Vorschlag für diese Aktivität dauerhaft aus. Nach dem Verknüpfen zeigt sie den Vergleich Plan ↔ Ist (Dauer, Belastung, anaerober Anteil, Zeit in Zonen).

**Ziel**: ein Zieltermin mit Name (`plan_settings`), im Kalender als Fahne, in den Diagrammen der Vorschau als senkrechte Linie; die Seite zeigt die erwartete Form an diesem Tag mit ihrem Zustand.

**Vorschau „Plan“**: vierter Modus neben Ruhe und „wie bisher“, auf der Formseite nur, wenn es ab morgen offene Einheiten gibt. Ab morgen gehen die offenen Einheiten (geplant, nicht verknüpft) als Tagesbelastung in dieselbe Kurve ein, im eingestellten Modell, mit Gewicht der Ermüdung und Foster-Aufschlag; **Tage ohne Einheit zählen als Ruhe**. Die Vorschau reicht mindestens bis zum letzten geplanten Tag bzw. zum Ziel plus 3 Tage, höchstens 90 Tage. Monotonie und Strain gelten damit auch für geplante Wochen.

**Seite „Plan“**: Wochenkalender (Montag bis Sonntag, vier Wochen ab der Vorwoche, blätterbar), geplante Einheiten mit farbigem Rand (blau geplant, grün erledigt, grau ausgelassen), vergangene Aktivitäten ohne Plan gestrichelt, je Woche Belastung (bis heute gemessen, danach geplant), Monotonie der Kalenderwoche (rot über 2,0) und Anteil der Zonen. Klick auf einen Tag legt eine Einheit an, Klick auf eine Einheit öffnet sie (ändern, ausgelassen, duplizieren auf einen anderen Tag, löschen). Darunter die Formvorschau des Plans mit Kacheln: Form heute, Form am Ziel, Fitness am Ziel, geplante Belastung. Alles ist sicherungsrelevant.

**Planungshilfe** (E8 Stufe 2, Schemaversion 16, Entscheidungen und Mockup Uli 23.11.2026, ohne KI): Knopf „Plan vorschlagen“ auf der Seite „Plan“, nur mit Ziel (mindestens 8 Tage voraus; ohne Ziel öffnet er den Zieldialog). Eingaben: Sportklasse, **höchstens Minuten je Wochentag** (unter 30 = Ruhetag), höchster Fitnessanstieg je Woche (3–7, Vorgabe 5), jede vierte Woche Entlastung (3:1, Vorgabe an), Intensität „polarisiert“ (Vorgabe) oder „nur Grundlage“. Die zuletzt übernommenen Eingaben merkt sich TrackCraft (`plan_settings.assistant`). Der Dialog zeigt links die Eingaben, rechts den Vorschlag (Form und Fitness am Ziel, Tapering, Wochenbalken nach Phase, Formvorschau, Einheiten je Woche) und rechnet bei jeder Änderung neu; „N Einheiten übernehmen“ legt sie als normale, änderbare Einheiten an.

- **Regeln:** Ab dem Tapering rückwärts Blöcke zu 7 Tagen, jeder vierte Entlastung (60 % des vorherigen Aufbaublocks). Je Block wird der längste freie Tag ab 90 min (Wochenende bevorzugt) die lange Einheit; polarisiert dazu Intervalltage (Aufbau: 2 ab vier freien Tagen, 1 ab zwei; Entlastung 1 ab drei; Tapering 1 je angefangener Woche, höchstens 2) an den längsten freien Tagen ab 45 min. Lange Einheiten und Intervalle liegen nie neben einem anderen harten Tag; feste Einheiten zählen als hart, wenn sie intensiv (Zone 4–5 bzw. Anteil intensiv) oder ab 90 min lang sind. Ein voller Block mit sieben Trainingstagen verliert den kürzesten freien Tag (mindestens ein Ruhetag). Einheiten unter 30 min entfallen.
- **Einheiten** als Zonenminuten (5-Minuten-Raster): Grundlage 15 % Z1, Rest Z2; locker (Entlastung) 40 % Z1; lange Einheit 10 % Z1; Intervalle 15 min Einfahren (5 Z1, 10 Z2), Wiederholungen 6 min Z4 mit 3 min Z2 (2–6, im Tapering höchstens 4), 10 min Ausfahren Z1. Ohne Zonensatz für die Sportklasse als Zielbelastung (Belastung je Stunde, Intervalle × 1,4 mit Anteil intensiv). Ohne Zonensatz und ohne Belastung je Stunde kein Vorschlag.
- **Optimierung** mit derselben Formkurve (Modell, Gewicht, Foster-Aufschlag) ab dem heutigen Zustand, feste Einheiten eingerechnet: In jedem Aufbaublock gilt der größte Anteil der verfügbaren Zeit (10–100 %, Bisektion), bei dem die Fitness höchstens um den erlaubten Anstieg wächst (anteilig bei kürzeren Blöcken). Für das Tapering probiert TrackCraft 7–14 Tage und einen Umfang von 20–80 % des letzten Aufbaublocks (zum Ende um 40 % fallend, Intervalle mindestens 45 min) und wählt die Lösung mit Form am Ziel bevorzugt +10 bis +20, sonst im Bereich „frisch“, darin mit der höchsten Fitness am Ziel; liegt keine im Bereich, die nächstgelegene. Die Seite warnt, wenn die Form nicht „frisch“ wird und wenn die verfügbare Zeit den Anstieg begrenzt.
- **Ersetzen:** Übernommene Einheiten tragen die Kennung des Vorschlags (`planned_units.suggestion_id`, im Kalender mit Zauberstab). Jede Änderung über die API löscht sie, die Einheit ist dann fest. Ein neuer Vorschlag ersetzt alle offenen, unveränderten Einheiten früherer Vorschläge ab morgen; feste, erledigte und ausgelassene bleiben.

**Wiederkehrende Einheiten und Wochenvorlagen** (E8, Schemaversion 17, Entscheidungen und Mockup Uli 23.11.2026): Beides erzeugt normale, einzeln änderbare Einheiten, verbunden durch eine Serien-Kennung (`planned_units.series_id`, im Kalender mit Pfeilkreis); eine Regel, aus der Termine erst beim Anzeigen entstehen, gibt es nicht.

- **Wiederholen** im Dialog einer Einheit: jede Woche oder alle 2, 3, 4 Wochen bis zu einem Datum (Vorgabe 8 Wochen später), höchstens 26 Einheiten mit der ersten. Bei einer vorhandenen Einheit heißt es „Wiederholen ab hier“ und beginnt eine neue Serie.
- **Serie ändern oder löschen**: Hat eine Einheit offene Einheiten derselben Serie danach, fragt der Dialog „Nur diese“ oder „Diese und folgende“. „Diese und folgende“ übernimmt nur die geänderten Felder (die Belastung als Ganzes), ein neues Datum verschiebt alle um dieselbe Zahl Tage, der Status gilt nur für diese; erledigte und ausgelassene bleiben unverändert. Löschen entfernt diese und die offenen danach.
- **Wochenvorlagen** (`plan_templates`): „Als Vorlage“ in der Wochenspalte speichert die Einheiten einer Kalenderwoche (Wochentag, Sportklasse, Untertyp, Titel, Notiz, Belastung, ohne Status) unter einem Namen; ein vorhandener Name (ohne Groß/Klein) wird nach Hinweis ersetzt. „Vorlage anwenden“ legt sie ab einer Woche auf 1 bis 12 Wochen an; vorhandene Einheiten bleiben, die Vorlage kommt dazu, Tage vor heute entfallen. Über mehrere Wochen wird jede Einheit der Vorlage zu einer Serie. Ein inzwischen gelöschter Untertyp entfällt. Der Dialog zeigt je Woche neue und vorhandene Einheiten.
- Einheiten aus Serien und Vorlagen sind fest; die Planungshilfe ersetzt sie nicht. Alles ist sicherungsrelevant.

Die Aktivitätenliste zeigt die Belastung als sortierbare Spalte mit Summe, das Aktivitätsdetail als Kachel neben der Herzfrequenz. W′-Balance braucht Leistungsdaten und ist nicht vorgesehen (Leistungsdaten siehe eigenes Issue).
