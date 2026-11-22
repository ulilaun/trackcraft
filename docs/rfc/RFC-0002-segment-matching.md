# RFC-0002: Segmentmatching

Status: Neufassung, Piggy 011 (Entwurf zur Freigabe), ersetzt die Fassung Piggy 009. Löst den Widerspruch zwischen ADR-0006 (Start-/Ziel-Puffer) und ADR-0005/RFC-0016/RFC-0014 (vollständige Referenzgeometrie) auf.

## 1. Überblick

Das Matching läuft zweistufig:

1. **Kandidatensuche** (schnell, bewusst großzügig): räumliche Suche mit `rstar`, Puffer um Start und Ziel, Richtungsprüfung.
2. **Endprüfung** (maßgeblich): Vergleich des Trackverlaufs mit der vollständigen Referenzgeometrie des Segments, Start-/Zielquerlinie, Zeitermittlung.

Ein Kandidat ist noch kein Treffer. Gewertet wird nur, was die Endprüfung besteht. Reihenfolge der Entscheidung: ungültig durch Zeit (Abschnitt 5) vor geometrischer Mehrdeutigkeit (Abschnitt 4.6) vor Wertung.

## 2. Parameter

Alle Parameter stehen in der TOML (RFC-0021), nicht in der Datenbank und nicht pro Segment.

| Schlüssel | Vorgabe | Bedeutung |
| :--- | :--- | :--- |
| `[segments.matching] matching_radius_meters` | 20.0 (bis M3c 15.0) | Stufe 1: Radius um Start-/Zielpunkt, gemessen zum Trackverlauf |
| `[segments.matching] max_vector_angle_deg` | 45.0 | Stufe 1: größte Winkelabweichung |
| `[segments.matching] max_offset_m` | 15.0 | Stufe 2: größter ausgeglichener Versatz des Versuchs (4.2) |
| `[segments.matching] tolerance_m` | 20.0 (bis M3c 13.0) | Stufe 2: größter Abstand zur Referenz nach Versatzausgleich |
| `[segments.matching] min_coverage_ratio` | 0.95 | Stufe 2: Mindestanteil der Referenzlänge |
| `[segments.matching] direction_reference_m` | 5.0 | lokale Richtung der Querlinie, höchstens Segmentlänge |
| `[segments.matching] max_backstep_m` | 5.0 | größter erlaubter Rückschritt (inklusive) |
| `[segments.matching] max_inner_gap_m` | 30.0 | größte erlaubte innere Lücke (inklusive), mindestens |
| `[segments.matching] max_inner_gap_ratio` | 0.02 | Lücke höchstens max(`max_inner_gap_m`, Anteil · Segmentlänge); Strecken 0 |
| `[segments.matching] max_interpolation_gap_s` | 15 | größter Zeitabstand zweier Punkte für lineare Interpolation (inklusive) |
| `[segments.matching] coverage_connection_s` | 15 | größte Zeitlücke, über die zwei Trackteile als zusammenhängend gelten (inklusive) |
| `[segments] minimum_length_m` | 20.0 | nur bei Erstellung geprüft (RFC-0016), kein Rechenparameter |

Alle Schlüssel von `[segments.matching]` sind Rechenparameter: Eine Änderung löst eine versionierte Neuberechnung aus (RFC-0007, RFC-0009) und ist sicherungsrelevant (RFC-0012).

## 3. Richtungen

**Trackvektor** an einer Start- oder Zielquerlinie: Richtung der Trackkante, die die Linie kreuzt, vom letzten Punkt vor der Linie (v) zum ersten danach (n) (lokale Ebene, φ Breite, λ Länge im Bogenmaß):

$$\vec{v}_{track} = \begin{pmatrix} (\lambda_{n} - \lambda_{v}) \cdot \cos\left(\frac{\phi_{v} + \phi_{n}}{2}\right) \\ \phi_{n} - \phi_{v} \end{pmatrix}$$

Die frühere geglättete Sehne über drei Punkte k-1, k, k+1 ist verworfen (M3a, 09.11.2026): Liegt das Ziel in einer Kurve, weicht sie zu stark ab. Beim Golden-Segment R2 lag sie bei 53 bis 61°, die kreuzende Kante bei 22 bis 36°; mit der Sehne wären 9 von 32 echten Fahrten verloren gegangen.

**Segmentvektor** an Start und Ziel: aus dem Referenzbezug `direction_reference_m` entlang der Referenz (am Start vorwärts, am Ziel rückwärts), höchstens die Segmentlänge. Fallen die Bezugspunkte geometrisch zusammen, wird die Segmenterstellung mit Hinweis abgelehnt (RFC-0016). Gleiches gilt bei Rundkursen mit unbestimmbarer Richtung.

**Winkelprüfung:**

$$\cos(\theta) = \frac{\vec{v}_{track} \cdot \vec{v}_{segment}}{\|\vec{v}_{track}\| \cdot \|\vec{v}_{segment}\|} \ge \cos(\texttt{max\_vector\_angle\_deg})$$

Bei 45° ist der Grenzwert 1/√2 ≈ 0,7071.

## 4. Ablauf

### 4.1 Stufe 1: Kandidatensuche

Ein Trackabschnitt ist Kandidat, wenn

- der Trackverlauf höchstens `matching_radius_meters` am Startpunkt des Segments vorbeiführt,
- ein späterer Teil des Trackverlaufs höchstens `matching_radius_meters` am Zielpunkt vorbeiführt und
- an beiden Stellen die Winkelprüfung (Abschnitt 3) besteht.

Gemessen wird der Abstand zur Verbindungslinie zweier aufeinanderfolgender Trackpunkte desselben Abschnitts, nicht zu den Punkten selbst. Bei Aufzeichnung mit mehreren Sekunden Abstand liegen die Punkte schneller Fahrten über 30 m auseinander. Mit Abstand zum nächsten Punkt würde ein Kandidat dann übersehen, im Testkorpus bei 12 bis 24 % der Versuche (RFC-0014). Die Distanz wird in WGS84 ohne Höhe gemessen. Stufe 1 entscheidet nie über eine Wertung, sondern nur, welche Aktivitäten und Abschnitte Stufe 2 prüft.

### 4.2 Stufe 2: Referenzprüfung

Der Trackverlauf zwischen Start- und Zielpassage muss die Referenz in Referenzreihenfolge abfahren:

- **Versatzausgleich:** Vor allen weiteren Prüfungen wird der Versuch um seinen mittleren Versatz zur Referenz verschoben. Der Versatz ist der Mittelwert der Vektoren von gleichmäßig verteilten Referenzpunkten zum jeweils nächsten Punkt des Trackverlaufs, iterativ bestimmt (dreimal neu berechnet nach Verschiebung). Ist er länger als `max_offset_m`, wird er auf diese Länge gekürzt. Start- und Zielpassage, Toleranz, Abdeckung und Lücken gelten für den verschobenen Track. Grund: Referenz und Versuch sind beide GPS-Aufzeichnungen und liegen auf derselben Strecke oft über die ganze Länge 8 bis 13 m parallel versetzt (RFC-0014, Golden-Segmente). Zwei gleich verlaufende Wege mit weniger als etwa `max_offset_m` + `tolerance_m` Abstand sind damit nicht unterscheidbar.
- **Toleranz:** Abdeckung zählt nur auf Trackstücken mit Abstand zur Referenz von höchstens `tolerance_m`.
- **Abdeckung:** Maßgeblich ist die Längenquote der Referenz, nicht die Punktzahl. Abgedeckte Intervalle werden nicht doppelt gezählt. Mindestens `min_coverage_ratio` der Referenzlänge muss abgedeckt sein.
- **Rückschritt:** Ein Rückschritt entlang der Referenz von genau `max_backstep_m` macht den Versuch nicht automatisch ungültig, ein größerer schon. Gemessen wird er an der Zuordnung der Trackpunkte zur Referenz (4.7). Weil diese bis zur Toleranz unscharf ist, wird ein Rückwärtsfahren auf gerader Strecke erst ab etwa 2 · `tolerance_m` + `max_backstep_m` (31 m) erkannt. Eine Erkennung über die Bewegungsrichtung verwarf echte Fahrten in Kehren und ist verworfen (M3a, Golden-Segment M1).
- **Innere Lücke:** Eine Lücke in der Abdeckung innerhalb des Versuchs von genau `max_inner_gap_m` ist zulässig, eine größere ungültig. Das gilt auch für eine abgekürzte Kehre.
- **Zusammenhang:** Trackteile mit einer Zeitlücke von höchstens `coverage_connection_s` (genau der Wert zählt noch) gelten als zusammenhängend.
- **Mehrdeutige Geometrie** (Parallelweg, Schleife, Kreuzung, Abkürzung): siehe 4.6.

### 4.3 Start- und Zielquerlinie

Start und Ziel sind Querlinien, die mit der lokalen Richtung (Abschnitt 3) bestimmt werden. Bei Strecken (`kind = route`, RFC-0016) zählt zusätzlich der erste bzw. letzte Punkt einer Aufzeichnung als Passage, wenn er höchstens `matching_radius_meters` von Start bzw. Ziel entfernt liegt und die erste bzw. letzte Kante die Winkelprüfung besteht; die Zeit gilt dann ab bzw. bis zu diesem Punkt. Eine Passage zählt nur bei echtem Seitenwechsel in Segmentrichtung. Ein bloßes Streifen der Linie ist keine Passage. Liegt ein Punkt direkt auf der Linie und der Track setzt auf der anderen Seite fort, ist das eine Passage.

### 4.4 Mehrfache Startpassage

Bei mehreren Startpassagen zählt die letzte. Ein Neustart ist nur bis zur Rückschrittgrenze (`max_backstep_m`) möglich. Ein neuer Versuch beginnt erst nach einer korrekten Startpassage.

### 4.5 Rundkurse, Achter und Lassos

Rundkurse mit gleichen Start-/Zielkoordinaten sind erlaubt. Nähe zum gemeinsamen Start/Ziel reicht nicht, der vollständige Referenzverlauf muss die Endprüfung bestehen. Fahrten über mehrere aufeinanderfolgende Runden mit gemeinsamer Passage ergeben bei n Runden n Versuche. Achter- und Lassosegmente sind wertbar.

### 4.6 Mehrdeutigkeit

Eine mehrdeutige Zuordnung wird bis zum Ziel weiterverfolgt. Ist sie am Ziel nicht aufgelöst, erfolgt keine Wertung, der Grund wird angezeigt (RFC-0016).

### 4.7 Präzisierungen der Umsetzung (M3a)

- **Ebene:** Gerechnet wird in einer lokalen ebenen Projektion um den Segmentstart (Meter Ost/Nord, 111.319,49 m je Grad). Über die wenigen Kilometer eines Segments weicht sie um weniger als 0,1 % von WGS84 ab.
- **Kandidatenpaare:** Zu jeder Zielpassage gehört die letzte Startpassage davor, die nicht vor dem Ziel des vorigen Versuchs liegt. Damit ergeben n Runden mit gemeinsamer Passage n Versuche (4.5), und bei mehreren Startpassagen zählt die letzte (4.4).
- **Start und Ziel nach Versatzausgleich:** Nach dem Ausgleich werden die Passagen für den verschobenen Track neu bestimmt, jeweils die nächste zur ursprünglichen innerhalb von 30 Trackpunkten. Ihre Zeiten gehen in die Dauer ein.
- **Abdeckung und Lücke:** gemessen auf Referenzproben im Abstand von 1 m (Auflösung der Grenze ±1 m). Nur Trackkanten mit Zeitabstand bis `coverage_connection_s` decken ab.
- **Reihenfolge:** Jeder Trackpunkt erhält die früheste Referenzstelle innerhalb der Toleranz, die nicht mehr als `max_backstep_m` hinter dem bisherigen Stand liegt, in einem Fenster nach vorn von Schrittlänge + 4 · `tolerance_m` + 2 · `max_inner_gap_m`. An Kehren, deren Äste näher als die Toleranz beieinander liegen, bleibt der Track so auf dem ersten Ast, bis nur noch der zweite passt (Golden-Segmente R3, M1, M2). Ein Rückschritt liegt vor, wenn ein Punkt nur noch zu Stellen weiter hinten passt.
- **Reihenfolge mit und ohne Versatzausgleich:** Der Versatz ist ein Mittelwert über das ganze Segment. Fährt der Track nur einen Teil auf einem Parallelweg, verschiebt der Ausgleich ihn an Kehren auf den falschen Ast (Golden-Segment M2, Fahrt 04.07.2025). Die Reihenfolge gilt daher, wenn sie mit oder ohne Ausgleich besteht.
- **Schleife, Umweg:** Ein Trackstück außerhalb der Toleranz, das um mehr als 2 · `max_inner_gap_m` länger ist als das Stück Referenz, das es überspringt, macht den Versuch mehrdeutig (4.6, nicht gewertet).
- **Prüfung:** Unit-Tests mit erzeugten Tracks (Grenzen von Lücke, Interpolation, Zusammenhang, Rückschritt, Versatz, Runden, Schleife, Ziel vor einer Kurve) und die zehn Golden-Segmente (RFC-0014): alle 212 erwarteten Versuche, keine weiteren, Dauer höchstens 34 ms vom Referenzskript entfernt.
- **Raster:** Abdeckung und Versatzausgleich suchen die nächste Trackkante über ein Raster (32 m) statt über alle Kanten. Das Raster wählt nur Kandidaten aus, die Abstände werden wie bei der vollständigen Suche berechnet; die Ergebnisse sind gleich (Golden-Segmente unverändert, Unit-Test gegen die vollständige Suche). Eine 43-km-Strecke dauert je Fahrt rund 60 ms statt 8 s.

### 4.8 Strecken (M3d)

Strecken (`kind = route`) laufen über viele Kilometer. Im Wald und im Tal weicht GPS dort abschnittsweise 15–35 m vom Referenzweg ab, und Aufzeichnungen beginnen und enden einige Dutzend Meter vom Kontrollpunkt (Einfahrt, später GPS-Empfang). Mit den Segmentwerten zählte von neun gleich gefahrenen Brandeckrunden nur die Fahrt, aus der die Strecke angelegt wurde. Strecken verwenden daher eigene Werte aus `[segments.route_matching]`, alle übrigen wie `[segments.matching]` (Uli 12.11.2026):

| Parameter | Segment | Strecke |
|---|---|---|
| `tolerance_m` | 20 m | 35 m |
| `max_inner_gap_m` | 30 m, bis 2 % der Länge | 200 m fest |
| `matching_radius_meters` | 20 m | 35 m |
| `recording_edge_m` (Aufzeichnungsrand als Passage) | – | 100 m |
| `max_vector_angle_deg` | 45° | 90° (nur Gegenrichtung ausgeschlossen) |
| `direction_reference_m` | 5 m | 50 m |

Der Winkel ist großzügig, weil man am Ziel oft schräg in die Einfahrt rollt und die Zielrichtung sonst aus den letzten Metern der Quellaufzeichnung stammt; die Richtung sichern Abdeckung (95 %) und Reihenfolge entlang der Strecke. Ergebnis mit den Brandeckrunden: neun von zehn Fahrten gewertet; die zehnte beginnt 167 m entfernt auf einem anderen Weg (rund 700 m Lücke), die Variante „Brandeck – Fürnsal“ zählt nicht.

### 4.9 Segmente mit 20 m Toleranz (M3d)

Mit 13 m Toleranz und 30 m Lücke fielen bei Ulis Segmenten viele echte Durchfahrten weg: im Wald und Tal weicht GPS abschnittsweise 15–25 m ab, und Start- oder Ziellinie werden einige Meter außerhalb von 15 m gekreuzt. Gegen alle 200 Edge-Fahrten (Uli 12.11.2026, Variante A):

| Segment | Länge | 13 m / 30 m | 20 m / 2 % | Obergrenze (35 m / 300 m) |
|---|---|---|---|---|
| Zimmerplatz Trail | 1,4 km | 3 | 6 | 6 |
| Badwald Trail, Heselbach Trail | 1,3–1,5 km | je 2 | je 3 | je 3 |
| Misselewegle | 0,7 km | 5 | 6 | 6 |
| Aischbach – Lindenhof | 1,8 km | 10 | 11 | 11 |
| Heusteig | 2,4 km | 7 | 8 | 9 |
| Reinerzau – Rossberg | 7,3 km | 1 | 5 | 6 |
| Testsegment Tal | 5,2 km | 2 | 9 | 10 |

Brende, Glatt – Birkhof, Bruckbach Stich, Ellbachsee und Hundseck bleiben gleich. Golden-Segmente: zwei Versuche kommen dazu (R1 13.08.2026, Start 17–19 m seitlich; M3 11.08.2024, Ziel 15–22 m seitlich), alle übrigen unverändert. Die Grenztests der Unit-Tests laufen weiter mit festen 13/15/30 m. Das Python-Referenzskript `a2_golden.py` rechnet mit den alten Werten.

Zwei gleich verlaufende Wege mit weniger als etwa `max_offset_m` + `tolerance_m` (35 m) Abstand sind damit nicht unterscheidbar.

## 5. Zeit

- Der Zeitpunkt einer Start-/Zielpassage wird linear zwischen den beiden angrenzenden Messpunkten interpoliert, wenn deren Zeitabstand höchstens `max_interpolation_gap_s` beträgt. Ist der Abstand größer, ist eine Interpolation nicht möglich: der Versuch ist ungültig, wird nicht gewertet und der Grund wird angezeigt.
- Gleicher Zeitstempel bei mehreren Punkten: der erste zählt, weitere werden ignoriert.
- Ein Punkt ohne Zeitstempel innerhalb eines Versuchs macht diesen Versuch ungültig, andere Versuche derselben Aktivität bleiben wertbar.
- Rückläufige Zeit macht den gesamten Versuch ungültig.
- Dauer, Rundung und Sortierung: ADR-0015 und RFC-0009 (Pausen inklusive, Start und Ziel auf ms gerundet, halbe ms aufrunden, Differenz wird gerankt).

## 6. Ergebnis eines Versuchs

Jeder Kandidat endet als gewertet, mehrdeutig (nicht gewertet, Grund sichtbar) oder zeitlich ungültig (nicht gewertet, Grund sichtbar). Geometrische Treffer ohne belastbare Zeit werden nicht gerankt.

Zu jedem gewerteten Versuch werden Ø und max. Herzfrequenz über die Messpunkte zwischen interpolierter Start- und Zielpassage gespeichert (`avg_heart_rate_bpm`, `max_heart_rate_bpm`, RFC-0001). Der Ø ist zeitgewichtet. Ohne HF-Werte im Versuch bleiben beide leer.

## 7. Offene Punkte

1. Die Vorgaben für Radius, Versatz, Toleranz, Abdeckung, innere Lücke und Zeitabstände sind an echten Dateien geprüft (RFC-0014). Laufen braucht mit Versatzausgleich keine eigenen Werte. `max_backstep_m` ist seit M3a an den Golden-Segmenten geprüft (keine echte Fahrt verworfen, Erkennungsgrenze siehe 4.2). Nicht an echten Dateien geprüft sind `direction_reference_m` und die Mehrdeutigkeit (4.6), sie decken die Unit-Tests ab.
2. Golden-Geometrien und vollständige Versuchsgrenzen (RFC-0014 Nr. 14).
