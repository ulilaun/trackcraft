# RFC-0016: Segmenterstellung und Bedienung

Status: Konsolidierte Regeln, Piggy 006.

## Erstellen aus Aktivität

Segmente entstehen ausschließlich aus einer fertig importierten Aktivität; ein Erstellen aus Segment-GPX gibt es nicht. Die Referenzdaten werden kopiert, das Segment hat keinen Bezug zur Originaldatei und keinen Verweis auf die Quellaktivität. Die Quellaktivität liefert über das normale Matching den ersten Versuch.

Genau einen Aufzeichnungsabschnitt auswählen, darin Startpunkt und späteren Zielpunkt. Aufgezeichneter Verlauf dazwischen in Aufzeichnungsrichtung wird vollständige Referenz. Kein freies Zeichnen, keine Umkehrfunktion beschlossen.

Nur bei verfügbarer Karte; lädt die Karte nicht, ist die Segmenterstellung mit Hinweis gesperrt. Karte mit zwei gekoppelten Schiebereglern entlang Aufzeichnung, Start/Ziel auf vorhandene gültige GPS-Punkte begrenzen. Auswahl hervorheben, Rest abblenden. Bei überlagerten Runden konkrete Passage über Position in Aufzeichnung eindeutig auswählen. Ganze Strecke möglich, falls innerhalb eines Abschnitts.

## Strecken

Eine **Strecke** ist ein Segment über eine ganze Tour, mit dem man wiederholt gefahrene oder gelaufene Runden vergleicht. Technisch ist sie ein Segment mit `kind = 'route'` (RFC-0001): gleiches Matching, gleiche Rangliste, gleiche PRs.

- **Anlegen**: Das Aktivitätsdetail hat zwei getrennte Knöpfe „Segment erstellen“ und „Strecke erstellen“, jeder öffnet seinen eigenen Dialog ohne Umschalter (gemeinsamer Aufbau aus Karte, Höhenprofil und gekoppelten Schiebereglern). Der Strecken-Dialog legt `kind = 'route'` fest. Start und Ziel stehen anfangs 500 m vom ersten bzw. letzten gültigen GPS-Punkt des Abschnitts entfernt und lassen sich höchstens `[segments] route_edge_m` (Vorgabe 1000 m) entlang der Aufzeichnung vom Rand verschieben, etwa um Garage oder Anfahrt abzuschneiden (Uli, 09.11.2026). Das Höhenprofil zeigt dabei eine gebrochene Achse: erster und letzter Kilometer breit, die Mitte als schmale Lücke mit Angabe der ausgelassenen Strecke. Bei mehreren Abschnitten wird der Abschnitt gewählt. Die übrigen Regeln dieses RFC gelten unverändert.
- **Keine doppelte Strecke**: Hat die Aktivität im veröffentlichten Stand irgendeinen Versuch auf einer Strecke, auch einen ungewerteten, ist „Strecke erstellen“ gesperrt; daneben steht, zu welcher Strecke sie passt, mit Link. Die API antwortet in diesem Fall mit `route_already_matched`. Segmente lassen sich weiter erstellen.
- **Art ändern**: Name, Beschreibung und Art (Segment oder Strecke) sind editierbar. Name und Beschreibung lösen keine Neuberechnung aus. Ein Wechsel der Art wertet die Historie des Segments neu aus, weil Strecken den Aufzeichnungsrand als Passage zählen (RFC-0002 4.3, Uli 09.11.2026).
- **Start und Ziel einer Strecke**: Beginnt eine Aufzeichnung höchstens `[segments.route_matching] recording_edge_m` (100 m) vom Start entfernt, gilt ihr erster Punkt als Startpassage; endet sie höchstens so weit vom Ziel, der letzte Punkt als Zielpassage, jeweils nicht gegen die Segmentrichtung. Strecken haben eigene, großzügigere Matching-Werte (RFC-0002 Abschnitt 4.8). Sonst würde schon die Quellaktivität keinen Versuch ergeben, und Runden ab der Haustür hingen vom Zufall ab, auf welcher Seite der Linie der erste GPS-Punkt liegt.
- **Anzeige**: Segmente und Strecken stehen als eigene Einträge in der Kopfzeile und haben je eine eigene Liste (RFC-0006). Die Segmentseite zeigt die Art.
- **Name beim Import** (Uli 22.11.2026): Ergibt die Auswertung einer neu angelegten Aktivität einen gültigen Versuch auf einer Strecke, bekommt sie deren Namen als Titel und, wenn die Strecke einen hat, deren Ort. Das gilt nur, solange die Aktivität nie geändert wurde (`revision = 1`); Ersetzen, Sportklassenwechsel und jede Bearbeitung erhöhen die Revision, die Übernahme selbst auch. Bestehende Aktivitäten benennt ein neues Segment oder eine neue Strecke nicht um. Passt die Aktivität zu mehreren Strecken, gilt die längste.
- **Filter**: Die Aktivitätenliste filtert nach einer Strecke (`route_id`) und zeigt alle Aktivitäten mit mindestens einem gültigen Versuch darauf. Sortiert nach Zeit ist das der Streckenvergleich. Eine Auswahlliste der Strecken in der Aktivitätenliste gibt es nicht (wird lang, ohne Suche); der Filter entsteht über „Alle Fahrten“ auf der Streckenseite und steht dann als entfernbarer Filter in der Liste (Uli 12.11.2026).
- **Ort**: Strecken (und Segmente) haben einen optionalen Ort als freien Text, weil Rundkurse meist am selben Ort beginnen und enden. Die Suche der Liste findet auch den Ort.
- Ein direkter Vergleich zweier Aktivitäten (Differenzen, Kurven übereinander) folgt in V2.

## Erstellen-Dialog (M3d)

- Unter dem Gesamtprofil zeigt ein vergrößertes Profil die Auswahl plus je ein Viertel Rand (mindestens 50 m), sobald die Auswahl weniger als die Hälfte der Aufzeichnung umfasst. Es folgt der Auswahl; Klick setzt den näheren Punkt (Uli, 12.11.2026).
- Die Knöpfe ◀ ▶ je Punkt wiederholen bei Dauerdruck (nach 400 ms alle 70 ms, nach 20 Schritten in Fünferschritten).
- Ziehen auf der Karte: Start und Ziel folgen der Aufzeichnung von ihrer bisherigen Lage aus vor oder zurück (schrittweise zum nächsten Punkt innerhalb von 150 m entlang der Linie) und springen nicht auf eine parallele Rückfahrt oder spätere Runde. Der Marker steht dabei immer auf der Linie.
- Strecken: kein Feld Steigung, dafür Ort.

## Kategorien

Jedes Segment und jede Strecke hat eine Kategorie mit eigenem Symbol, die in der Liste angezeigt und gefiltert wird (Uli 12.11.2026). Sie wird beim Anlegen aus der Referenz bestimmt und lässt sich von Hand überschreiben; „automatisch“ stellt sie zurück. Ein Artwechsel bestimmt sie neu.

| Art | Kategorie | Regel |
|---|---|---|
| Segment | Anstieg (`climb`) | Ø-Steigung aus Start- und Zielhöhe ≥ +2 % |
| Segment | Abfahrt (`descent`) | Ø-Steigung ≤ −2 % |
| Segment | Wellig (`rolling`) | dazwischen, auch flach |
| Strecke | Rundkurs (`loop`) | Start und Ziel höchstens `[segments] route_loop_radius_m` (Vorgabe 750 m) und höchstens ein Drittel der Länge auseinander |
| Strecke | Hin & zurück (`out_and_back`) | wie Rundkurs, und mindestens 70 % der zweiten Hälfte liegen höchstens 35 m neben der ersten |
| Strecke | Strecke (`point_to_point`) | Start und Ziel weiter auseinander |

Ohne Höhen bzw. Positionen bleibt die Kategorie leer. Bei Strecken liegen Start und Ziel am Aufzeichnungsrand und schwanken um einige hundert Meter; deshalb 750 statt früher 200 m (Uli, 14.11.2026). Beim Start werden die automatischen Kategorien aller Strecken mit dem aktuellen Radius neu bestimmt; von Hand gesetzte Kategorien bleiben.

## Ähnliche Segmente

Der Erstellen-Dialog weist auf ein bestehendes Segment gleicher Art und Sportklasse hin, mit Link. Anlegen bleibt möglich.

- **Segment**: Start und Ziel jeweils höchstens 30 m entfernt und Länge um höchstens 10 % anders.
- **Strecke**: Vergleich über den Verlauf, weil Start und Ziel am Aufzeichnungsrand von Fahrt zu Fahrt um einige hundert Meter schwanken. Ähnlich, wenn mindestens 90 % der Auswahl höchstens `[segments.route_matching] tolerance_m` (35 m) neben der bestehenden Strecke liegen und umgekehrt. Das Backend prüft das (`POST /segments/similar`), der Dialog fragt nach kurzer Pause beim Verschieben neu an.

## Mindestdaten

Mindestens zwei gültige GPS-Punkte und konfigurierte Mindestlänge, initial 20 m einschließlich. Länge aus WGS84-Oberflächengeometrie ohne Höhenanteil. Zeitstempel/Höhe optional. Keine Verbindung über Aufzeichnungsgrenzen.

`[segments].minimum_length_m` nur bei Erstellung prüfen. Spätere Änderung lässt bestehende Segmente/Wertungen unverändert, keine automatische Invalidierung.

Rundkurse erlaubt: Start/Ziel unterschiedliche Positionen in Aufzeichnung, gleiche Koordinaten möglich. Vollständiger Referenzverlauf muss Matchingregeln erfüllen, Nähe zum gemeinsamen Start/Ziel reicht nicht.

Lokale Richtung für Start-/Zielquerlinie aus konfigurierbarem 5-m-Referenzbezug. Es gilt der aktuelle Bezugswert, höchstens die Segmentlänge. Geometrisch zusammenfallende Bezugspunkte: Erstellung mit Hinweis ablehnen.

Ersteller automatisch einziger Benutzer. Eine Sportklasse cycling/ebike/running/skiing erforderlich: aus der Aktivität vorausgewählt und änderbar, bei unknown-, swimming- oder studio-Aktivität Pflichtauswahl.

Bedienung im Dialog (Mockup M3c, Uli 09.11.2026): Ziehen am Band zwischen den Reglern verschiebt Start und Ziel gemeinsam bei gleicher Länge (nur Segment); ◀ ▶ und Pfeiltasten bewegen um einen Punkt (Shift 10, Bild auf/ab 100); Klick in Profil oder Schiene setzt den näheren Regler; Start und Ziel lassen sich auf der Karte entlang der Linie ziehen, bei mehreren Durchfahrten gewinnt die in der Aufzeichnung nächstgelegene; liegt Start oder Ziel an einer mehrfach durchfahrenen Stelle, nennt ein Hinweis die gewählte Durchfahrt. Länge, Ø-Steigung, Auf- und Abstieg und die Zeit in dieser Fahrt werden laufend angezeigt. Ein neues Segment ist anfangs mit einem Ausschnitt in der Mitte vorbelegt, eine Strecke mit dem Aktivitätstitel als Namen.

## Lebenszyklus und Anzeige

Neue Segmente automatisch gegen vorhandene Aktivitäten auswerten; Fortschritt/Abschluss zeigen, Historie bis dahin unvollständig. Name/Beschreibung/Art editierbar; Referenz/Richtung/Sportklasse fest, Änderung erfordert neues Segment.

Liste Suche/Filter, Detail Name/Länge/verfügbare positive/negative Höhen/Sportklasse/Versuche/Karte. Eigene Bestzeiten/zweite-dritte Leistung/Verlauf. Gestaltung der Segmentliste und -seite mit Bestzeiten-Entwicklung und Rücksprung zur Herkunft: RFC-0006 Abschnitt 2. Jeder Versuch zeigt Ø und max. HF (RFC-0002 Abschnitt 6). Ranking ms, Anzeige Zehntel. Erste Bestzeit von PR-Verbesserung unterscheiden.

Geometrische Treffer ohne belastbare Zeit nicht ranken, Grund zeigen. Mehrdeutige Passage bis Ziel nicht werten. Neuberechnung alter Stand veraltet, neue Ergebnisse gemeinsam; ohne Vorbestand laufend, Fehler sichtbar.

Löschen entfernt Segmentpunkte/Versuche/PRs, nicht Aktivitäten/Originale. Löschen der Quellaktivität lässt das Segment bestehen. Konsequenz vor Ausführung zeigen, Auswertung aktualisieren. Jobs keine Wiederbelebung.

Keine Favoriten/Folgen/Schwierigkeit, globalen Ranglisten oder Social-Funktionen in V1. Matchingdetail [RFC-0002](RFC-0002-segment-matching.md).
