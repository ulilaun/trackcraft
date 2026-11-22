# RFC-0008: Aktivitätsimport, Outlier-Filter und Pipeline

Status: Neufassung, Piggy 011 (Entwurf zur Freigabe). Führt die bisher in RFC-0003, RFC-0009, RFC-0014, RFC-0017 und ADR-0011 verstreuten Importregeln zusammen. Alle Werte in Klammern sind Vorgaben aus der TOML (RFC-0021).

## 1. Ablauf

1. **Upload und Staging**: Datei empfangen, Limits prüfen, in geschützter temporärer Ablage halten.
2. **Parsen**: FIT in Sessions, GPX in Abschnitte.
3. **Sessionauswahl** (nur FIT): vorausgewählt, einzeln abwählbar.
4. **Pre-Processing je Aktivität**: Sportart und Sportklasse bestimmen, Fahrzeugerkennung, Outlier-Filter, Kennzahlen berechnen.
5. **Start und Gesamtzeit abfragen**, falls eine FIT-Session weder GPS-Punkte noch Zeitangaben enthält.
6. **Duplikatprüfung und Konfliktdialog**.
7. **Speichern**: Original einmal archivieren, Aktivitäten und Abschnitte atomar schreiben. Damit ist der Import abgeschlossen.
8. **Auswertung** (Segmentmatching, Statistik) läuft danach asynchron als persistenter Job (`jobs`, RFC-0001). Der Import ist von der Auswertung getrennt. Ein Auswertungsfehler macht den Import nicht rückgängig und ist ohne Neuimport wiederholbar.

## 2. Upload und Limits

| Limit | Wert | Wirkung |
| :--- | :--- | :--- |
| Dateigröße | 50 MB | Datei wird abgewiesen |
| Punkte je Session | 100.000 | Session ist gesperrt, andere Sessions derselben Datei bleiben importierbar |
| Fahrzeugabschnitt | Abschnitt 5.1 | Session ist gesperrt, andere Sessions derselben Datei bleiben importierbar |
| Upload-Timeout | 1 Stunde | Upload wird abgebrochen |
| GPX ohne Zeitangaben | Abschnitt 3 | Datei wird abgewiesen (`gpx_without_time`) |
| ZIP und andere Formate | – | Datei wird abgewiesen (`unsupported_format`), Archive werden nicht entpackt |

Unter Docker müssen Reverse Proxy und Nginx im Frontend-Container mindestens diese Grenzen zulassen (Body-Größe und Timeout), sonst scheitert der Upload vor dem Backend. Unter Windows nimmt das Backend den Upload direkt an und wendet dieselben Grenzen selbst an. Abgewiesene Uploads werden nicht aufbewahrt. Es bleiben nur die Fehlermeldung und ein Logeintrag, es gibt keinen `rejected`-Ordner.

## 3. Formate

- **FIT**: eine Aktivität je gewählter Session. Gewählte Sessions werden gemeinsam gespeichert oder gemeinsam zurückgenommen. Eine Datei mit gleichem Hash ist eine identische Datei: Fehlende Sessions werden ergänzt, vorhandene sind gesperrt und bleiben unverändert, das Original wird nur einmal referenziert.
- **GPX**: eine Aktivität je Datei. Die Abschnitte bleiben getrennte `track_sections`. Es gibt keine Geometrie-, Höhen- oder Matchingverbindung über Abschnittsgrenzen.
  - Gelesen werden nur `<trk>`/`<trkseg>`/`<trkpt>` mit `<time>` und `<ele>`. Routen (`<rte>`) und Wegpunkte (`<wpt>`) werden ignoriert.
  - Herzfrequenz kommt aus der Garmin-TrackPointExtension (`<extensions>` → `TrackPointExtension` → `hr`, Version 1 und 2, erkannt am lokalen Namen unabhängig vom Präfix). Andere Extensions werden ignoriert.
  - Hat kein `<trkpt>` ein `<time>`, ist die Datei eine geplante Route (z. B. Komoot) und keine Aktivität. Sie wird mit `gpx_without_time` abgewiesen (Abschnitt 2). Punkte ohne `<time>` in einer Datei mit Zeitangaben werden gespeichert (Abschnitt 5).
- Dateien ohne GPS oder mit positionslosen Sensorwerten sind importierbar. Fehlende Werte bleiben unbekannt und werden nie als 0 eingetragen.

### FIT-Zuordnung

Die Kennzahlen (Abschnitt 6) kommen aus der Session-Message, die Messpunkte aus den Record-Messages. Ein Feld mit dem Ungültig-Wert seines FIT-Basistyps (z. B. `0xFF` bei uint8, `0xFFFF` bei uint16) gilt als fehlend. FIT-Zeitpunkte sind Sekunden seit 1989-12-31T00:00:00Z und werden in UTC-Millisekunden umgerechnet.

| Kennzahl | Session-Feld | Einheit | Hinweis |
| :--- | :--- | :--- | :--- |
| Start | `start_time` | FIT-Zeitpunkt | |
| Gesamtzeit | `total_elapsed_time` | ms (Skalierung 1000) | nie `total_timer_time`, das Pausen ausschließt |
| Distanz | `total_distance` | cm (Skalierung 100) | 0 gilt als fehlend |
| Aufstieg | `total_ascent` | m | 0 gilt als fehlend |
| Durchschnittliche Herzfrequenz | `avg_heart_rate` | bpm | |
| Sportart, Untertyp | `sport`, `sub_sport` | Enum | Normalisierung nach Abschnitt 4 |

`total_descent`, Runden, Bahnen, Events und `activity.local_timestamp` werden in V1 nicht ausgewertet.

| Messpunktfeld (RFC-0022) | Record-Feld | Umrechnung |
| :--- | :--- | :--- |
| `t_ms` | `timestamp` | FIT-Zeitpunkt in ms |
| `lat_e7`, `lon_e7` | `position_lat`, `position_long` | Semicircles, Grad = Wert · 180 / 2³¹ |
| `ele_dm` | `enhanced_altitude`, sonst `altitude` | Rohwert / 5 − 500 m |
| `hr` | `heart_rate` | bpm |

Nur für die Fahrzeugerkennung (Abschnitt 5.1) und nicht gespeichert: `enhanced_speed`, sonst `speed` (mm/s, Skalierung 1000), und `distance` (cm, Skalierung 100) aus den Records.

Sessions, Records und Abschnitte:

- `session_index` ist die Reihenfolge der Session-Messages in der Datei, beginnend bei 0.
- Ein Record gehört zur letzten Session, deren `start_time` nicht nach seinem `timestamp` liegt. Records vor der ersten Session gehören zur ersten, bei einer einzigen Session gehören alle Records zu ihr. `start_time + total_elapsed_time` ist keine Grenze, weil Geräte nach dem letzten Timer-Stopp noch Records schreiben. Bei mehreren Sessions werden Records ohne Zeitstempel verworfen.
- Fehlt die Session-Message, gilt die Datei als eine Session ohne Dateiwerte.
- Jede Session ergibt genau einen Abschnitt. Pausen, Auto-Pause und Timer-Stopps teilen nicht, weil die Wertung Pausen mitzählt (RFC-0009), Lücken behandelt das Matching (RFC-0002).
- Der Parser (ADR-0002) muss komprimierte Zeitstempel-Header, beide Byte-Reihenfolgen und verkettete Dateien lesen und Developer-Felder überspringen.

### FIT-Parser

Der Import liest FIT mit rustyfit (ADR-0002):

- `Decoder::decode` wird wiederholt, bis keine weitere Teildatei folgt. Sessions und Records verketteter Teildateien zählen fortlaufend, als wäre es eine Datei. `session_index` läuft über alle Teildateien.
- Session- und Record-Werte werden über `mesgdef::Session` und `mesgdef::Record` als Rohwerte gelesen und nach der Tabelle oben umgerechnet. Der Ungültig-Wert wird gegen den Rohwert geprüft, nicht gegen einen skalierten Wert.
- Die Prüfsummen prüft TrackCraft selbst nach FIT-Protokoll, rustyfit dekodiert ohne Prüfung: Die Datei-Prüfsumme jeder Teildatei umfasst Kopf und Daten. rustyfit kann eine falsche Datei-Prüfsumme nur abweisen oder gar nicht prüfen; TrackCraft soll sie melden und die Datei trotzdem annehmen (siehe unten). Anlass war ein Fehler bis rustyfit 0.11.1: Bei einem 14-Byte-Kopf mit Kopf-Prüfsumme 0 („nicht gesetzt“) blieb der Kopf aus der Datei-Prüfsumme heraus, intakte Garmin-Dateien galten deshalb als beschädigt (bei Ulis Garmin-Dateien 2016–2019 etwa die Hälfte). Behoben in rustyfit 0.11.2 (muktihari/rustyfit#179), TrackCraft verlangt seitdem mindestens 0.11.2.
- Eine falsche Kopf-Prüfsumme oder eine abgebrochene Struktur weist die Datei ab (Abschnitt 2), es gibt keinen Teilimport.
- Passt nur die Datei-Prüfsumme nicht und lässt sich die Datei trotzdem vollständig dekodieren, wird sie angenommen (Uli 14.11.2026). Die Bestätigung meldet dann den Hinweis `file_checksum_ignored` in `meta.notices`, der Audit-Eintrag des Imports trägt `checksum_ignored: true`, das Log eine Warnung.
- Developer-Felder werden gelesen, aber nicht ausgewertet, Felder ohne `field_description` werden übersprungen.

Geprüft an 200 Dateien eines Garmin Edge (Radfahren, je eine Session, 2023 bis 2026): Pausenanteil bis 30 %, bis knapp eine Minute Records nach `start_time + total_elapsed_time`, Höhe nur in `altitude`, bei Indoor `total_distance = 0` und ungültiger `total_ascent`, ohne Brustgurt ungültige `avg_heart_rate`. Mehrere Sessions, Laufen und andere Geräte sind damit nicht belegt (Testkorpus, RFC-0014).

## 4. Sportart und Sportklasse

- `source_sport` ist der normalisierte Sporttyp aus der Datei und Schlüssel für Sport-Mapping und Outlier-Schwellen:
  - **FIT**: der FIT-Name von `sport`, bei gesetztem `sub_sport` außer `generic` ergänzt um `-` und dessen FIT-Namen, z. B. `cycling`, `cycling-road`, `cycling-mountain`, `running-trail`, `e_biking`.
  - **GPX**: der Text von `<type>` in Kleinbuchstaben, jede Folge anderer Zeichen als Buchstaben und Ziffern wird zu `_`, führende und folgende `_` entfallen (`Road Biking` wird `road_biking`). Ohne `<type>` bleibt `source_sport` leer.
  - Der Bindestrich trennt Sport und Untersport, weil FIT-Namen ihn nicht enthalten und er in TOML-Schlüsseln ohne Anführungszeichen erlaubt ist.
- Die Sportklassen selbst (`cycling`, `ebike`, `running`, `skiing`, `swimming`, `studio`, `unknown`) stehen fest im Code, weil Symbole, Farben, Bestenlisten, Rekorde und Statistik an ihnen hängen.
- **Sportart-Zuordnung in der Datenbank:** Die Tabelle `sport_mappings` ordnet einem `source_sport` Sportklasse und optional einen Untertyp zu, die Tabelle `sub_sports` hält den Untertyp-Katalog je Sportklasse (RFC-0001). **Untertypen haben wie die Sportklassen einen festen Schlüssel** (`road`, `mtb`, `indoor`, `street`, `trail`, `walking`, `nordic`, `strength`), den die Oberfläche übersetzt (Lokalisierung, Uli 14.11.2026). Ein eigener Name überschreibt die Übersetzung; eigene Untertypen haben immer einen Namen und einen erzeugten Schlüssel `custom_…`. Aktivitäten und Zuordnungen speichern den Schlüssel mit Fremdschlüssel auf den Katalog. Beides ist in der Oberfläche pflegbar (Einstellungen → Sportarten, RFC-0006) und Teil der Vollsicherung (RFC-0012).
  - Gesucht wird erst der volle Wert; ein Treffer liefert Sportklasse und Untertyp. Sonst gilt der Eintrag des Teils vor dem ersten `-`, dann **nur für die Sportklasse** (der Untertyp bleibt leer). Ohne Treffer gilt `unknown`. Ein Untersport erbt damit die Klasse seines Sports, z. B. `running-hill` die von `running`.
  - Die Outlier-Schwelle bleibt Betriebsparameter der TOML (`[processing.outlier_filter.max_speed_kmh]`, gleiche Suche, sonst `default_max_speed_kmh`).
  - **Vorgaben:** Das Programm bringt eine Liste von Zuordnungen und Katalogeinträgen mit festen Schlüsseln mit (`mapping:<source_sport>`, `catalog:<klasse>:<name>`). Beim Start werden Vorgaben, die noch nie angewendet wurden, eingetragen und in `sport_defaults_applied` vermerkt; ein vorhandener Eintrag des Benutzers bleibt unberührt. Ändert oder löscht der Benutzer einen Eintrag, bringt ein Update ihn nicht zurück. Eine inhaltlich geänderte Vorgabe braucht einen neuen Schlüssel.
  - **Übernahme aus der TOML:** Beim ersten Start mit Schemaversion 7 werden die früheren Abschnitte `[import.sport_class_mapping]`, `[import.sub_sport_mapping]` und `[import.sub_sport_catalog]` einmalig übernommen; sie haben Vorrang vor den Vorgaben, ungültige Einträge werden mit Warnung übersprungen. Danach ignoriert das Programm die Abschnitte und weist im Log darauf hin (RFC-0021).
  - Manuell wird der Untertyp aus dem Katalog der Sportklasse gewählt; ein neuer Untertyp wird dabei im Katalog angelegt (kein Freitext mehr). Wechselt nur die Sportklasse, entfällt ein Untertyp, den die neue Klasse nicht kennt. Nur die Sportklasse trennt Bestenlisten.
  - Ersatztitel beim Import verwenden den eigenen Namen, sonst die deutsche Bezeichnung des eingebauten Untertyps; sie werden als Text gespeichert.
  - Migration 7 wandelt vorhandene Untertyp-Texte um: die deutschen Bezeichnungen der eingebauten Einträge werden zu deren Schlüssel, jeder andere Text wird ein eigener Eintrag seiner Klasse.
  - **Umbenennen** setzt nur den Namen; Zuordnungen und Aktivitäten verweisen auf den Schlüssel und zeigen ihn sofort. `name: null` setzt einen eingebauten Untertyp auf die übersetzte Bezeichnung zurück. Angezeigte Namen sind je Klasse ohne Groß-/Kleinschreibung eindeutig. **Löschen** ist gesperrt, solange eine Aktivität oder Zuordnung ihn verwendet (Code `sub_sport_in_use` mit Anzahlen; Uli 14.11.2026).
- Der Profilname des Geräts (FIT-Message `sport`) wird nicht ausgewertet, er ist frei benannt und gerätespezifisch.
- Eine spätere Korrektur der Sportklasse invalidiert die betroffenen Wertungen und löst Neuberechnung aus. Eine geänderte Zuordnung gilt nur für künftige Importe; vorhandene Aktivitäten behalten Sportklasse und Untertyp, ein „auf vorhandene anwenden“ gibt es nicht (Uli 14.11.2026).

## 5. Outlier-Filter

Der Filter läuft vor dem Speichern, sofern `enabled = true` ist, je Abschnitt getrennt. Er prüft nur Punkte mit Position und Zeitstempel.

1. Die Höchstgeschwindigkeit ist `max_speed_kmh[source_sport]`, sonst `default_max_speed_kmh`.
2. **Erster Bezugspunkt** (Mehrheitsregel, geändert am 08.11.2026): Ein Punkt wird erster Bezugspunkt eines Abschnitts nur, wenn die Geschwindigkeit von ihm zu mehr als der Hälfte der nächsten 5 geprüften Punkte höchstens `max` ist, also zu mindestens 3 von 5. Sonst wird er verworfen und der nächste Punkt geprüft. Gibt es weniger als 5 folgende geprüfte Punkte, gilt die Mehrheit der vorhandenen; ohne Folgepunkt ist der Punkt Bezugspunkt. Eine schlechte Kaltstart-Position fällt damit durch, auch mehrere dicht beieinander, und ein einzelner Sprung kurz nach dem Start reißt keine guten Punkte davor mit. Die frühere Regel „zu jedem der nächsten 5“ verwarf in diesem Fall alle Punkte vor dem Sprung.
3. Die Prüfung läuft sequentiell: `v_i` ist die Geschwindigkeit zwischen dem letzten akzeptierten Punkt und Punkt i (Distanz auf WGS84 ohne Höhe geteilt durch Zeitdifferenz). Ist die Zeitdifferenz 0 oder negativ, gilt die Geschwindigkeit als unendlich, außer die Position ist gleich. Distanzen rechnet TrackCraft überall geodätisch auf dem WGS84-Ellipsoid (Verfahren nach Karney, Bibliothek `geo`), nicht auf einer Kugel.
4. Gilt `v_i > max`, wird Punkt i verworfen und nicht gespeichert. Der nächste Punkt wird wieder gegen den letzten akzeptierten Punkt geprüft. Ein einzelner GPS-Sprung entfernt damit nur den Sprungpunkt.
5. **Gleicher Zeitstempel**: Hat Punkt i denselben Zeitstempel wie der letzte akzeptierte Punkt, wird er verworfen (der erste zählt, wie im Matching, RFC-0002).
6. **Ohne Zeitstempel oder ohne Position**: Der Punkt wird nicht geprüft, nicht verworfen und nicht Bezugspunkt.
7. **Verworfen heißt Position verworfen**: Ein verworfener Punkt verliert nur seine Position. Zeitstempel, Höhe und Herzfrequenz bleiben als positionsloser Messpunkt gespeichert. Ohne Zeitstempel (gleicher Zeitstempel, Punkt 5) entfällt der ganze Punkt.
8. Die Zahl verworfener Punkte steht in `filtered_outliers_count` (verworfene Positionen und wegen gleichem Zeitstempel entfallene Punkte zusammen), die angewandte Schwelle in `max_speed_threshold_kmh` (`NULL`, wenn der Filter aus ist).
9. Die Originaldatei bleibt unverändert. Eine geänderte Schwelle wirkt nur auf künftige Importe, weil verworfene Punkte nicht gespeichert werden.

Höhenwerte werden nicht separat auf Ausreißer gefiltert.

Im Testkorpus (RFC-0014) verwirft der Filter beim Radfahren 249 von 1,56 Mio. Punkten in 58 von 199 Dateien, nie mehr als 2 Punkte am Stück. Die Schwellen bleiben damit unverändert.

### 5.1 Fahrzeugerkennung

Eine Aufzeichnung, die während einer Fahrt im Auto, Bus oder Zug weiterläuft, ist kein GPS-Fehler. Der Outlier-Filter würde die Punkte entfernen, Distanz und Gesamtzeit aus der Datei enthielten die Fahrt aber weiter. TrackCraft kann eine Aktivität in V1 nicht kürzen. Eine solche Session wird deshalb gesperrt und nicht importiert, die Meldung nennt den Abschnitt. Die Aktivität muss vorher außerhalb von TrackCraft gekürzt werden.

Die Prüfung läuft je Session vor dem Speichern, sofern `[processing.vehicle_detection] enabled = true` ist. Die Grenzen werden wie beim Outlier-Filter über `source_sport` gesucht (erst der volle Wert, dann der Sport). Ohne Eintrag, also auch bei `unknown`, gibt es keine Prüfung. Jedes der beiden Kriterien allein sperrt die Session:

1. **Spitze**: Die Geschwindigkeit liegt mindestens `min_duration_s` (30 s) ununterbrochen über `peak_speed_kmh`. Maßgeblich ist die Gerätegeschwindigkeit der Records. Fehlt sie (GPX, FIT ohne Geschwindigkeit), gilt die Geschwindigkeit zwischen aufeinanderfolgenden Punkten vor dem Outlier-Filter. Einzelne GPS-Sprünge erreichen die Dauer nicht.
2. **Schnitt**: Die Durchschnittsgeschwindigkeit über ein Fenster von `window_s` (300 s) liegt über `avg_speed_kmh`. Maßgeblich ist die Distanz der Records (FIT `distance`), sonst die GPS-Distanz über die Punkte nach dem Outlier-Filter. Ein Fenster, in dem zwei aufeinanderfolgende Punkte mehr als `max_pause_s` (60 s) auseinanderliegen, zählt nicht.

| Sport | Spitze | Schnitt über 5 min |
| :--- | :--- | :--- |
| `cycling`, `e_biking` | 90 km/h | 70 km/h |
| `running` | 35 km/h | 20 km/h |

Herleitung aus dem Testkorpus: Die schnellste Radfahrt erreicht 82 km/h Spitze und 62 km/h im 5-Minuten-Schnitt, der schnellste Lauf 13,8 km/h und 11,3 km/h. Kürzere Fenster sind empfindlich für Sprünge im Distanzzähler, die der Edge vereinzelt schreibt. Die Erkennung findet beim Radfahren nur Fahrzeugabschnitte über 70 km/h. Eine Fahrt im Stadtverkehr ist von einer schnellen Abfahrt nicht zu unterscheiden.

Die Meldung (`lock_reason`, RFC-0003) nennt Beginn und Ende des ersten erkannten Abschnitts in lokaler Zeit, die höchste Geschwindigkeit, das Kriterium und die Grenze, z. B. „Zwischen 13:05 und 13:08 bis zu 68 km/h, über der Grenze für Laufen (35 km/h). Vermutlich Fahrzeug, bitte Aktivität vor dem Import kürzen.“ Geänderte Grenzen wirken nur auf künftige Importe.

## 6. Kennzahlen

| Kennzahl | Quellenreihenfolge | Hinweis |
| :--- | :--- | :--- |
| Distanz | Dateiwert (vorhanden und größer 0, 0 gilt als fehlend), sonst GPS-Berechnung auf WGS84 ohne Höhe über die gefilterten Punkte je Abschnitt | sonst unbekannt (`NULL`) |
| Gesamtzeit (inklusive Pausen) | Dateiwert, sonst erster bis letzter GPS-Punkt, sonst manuelle Eingabe | Herkunft wird gespeichert, keine Bewegungszeit |
| Start | Dateiwert, sonst erster Punkt, sonst manuelle Eingabe | jede Aktivität hat einen Start |
| Aufstieg | Dateiwert (größer 0), sonst Hysterese über die Höhen der gespeicherten Messpunkte | Hysteresewert gehört zur Berechnungsversion (RFC-0007), fehlende Höhe ist unbekannt, nicht 0 |
| Durchschnitt | Distanz geteilt durch Gesamtzeit | bei Gesamtzeit 0 oder fehlender Distanz kein Wert, Herkunft wird angezeigt |
| Durchschnittliche Herzfrequenz | Dateiwert (FIT-Session, größer 0), sonst Mittelwert der Herzfrequenzwerte der Messpunkte, falls vorhanden | sonst unbekannt, nur Anzeigewert ohne HF-Analyse, Herkunft in `heart_rate_origin` |
| Titel | GPX: `<name>` des ersten `<trk>`, sonst `<metadata><name>`, getrimmt und nicht leer; FIT hat kein Titelfeld | sonst Ersatztitel aus Bezeichnung und lokalem Start `TT.MM.JJJJ HH:MM` in der Konfigurationszone, z. B. `Rennrad 02.07.2023 10:02`. Bezeichnung ist der Untertyp, sonst die Sportklasse (`Radfahren`, `E-Bike`, `Laufen`), bei `unknown` `Aktivität`. Im Importdialog ist der Titel je Session editierbar (RFC-0003 `SessionDecision.title`), beim Ersetzen gilt die Übernahme aus `carry_over` vorrangig |
| Ort | wird beim Import nicht gesetzt | freies Feld, nur manuell editierbar |

### Höhenhysterese

Die Hysterese läuft über die gespeicherten Messpunkte, also nach dem Outlier-Filter (Abschnitt 5), bei Segmenten über die Referenzpunkte. Sie wird je Abschnitt berechnet und über die Abschnitte summiert, ohne Verbindung über Abschnittsgrenzen (Abschnitt 3). Die Schwelle `s` ist `[processing.elevation] hysteresis_m`, Vorgabe 3 m (RFC-0021).

Die Referenzhöhe `r` ist zunächst die erste verfügbare Höhe des Abschnitts. Für jede weitere Höhe `h` gilt `d = h − r`:

- `d ≥ s` (inklusive): Aufstieg `+= d`, `r = h`.
- `d ≤ −s` (inklusive): Abstieg `+= −d`, `r = h`.
- sonst: keine Änderung.

Punkte ohne Höhe werden übersprungen. Beginn und Ende der Aufzeichnung haben keine Sonderbehandlung, ein Rest unter `s` am Ende zählt nicht. Hat kein Abschnitt mindestens zwei Punkte mit Höhe, sind Aufstieg und Abstieg unbekannt (`NULL`), sonst bekannt, auch wenn sie 0 sind. Aktivitäten verwenden den Aufstieg, Segmente zeigen Aufstieg und Abstieg.

Beispiele mit `s = 3 m`:

| Höhenfolge (m) | Aufstieg | Abstieg | Grund |
| :--- | :--- | :--- | :--- |
| 100 → 101 → 102 → 103 → 104 → 106 | 6 | 0 | bei 103 erreicht (3), bei 106 erneut (3) |
| 100 → 102 → 102,9 → 101 | 0 | 0 | Schwelle nie erreicht |
| 100 → 103 | 3 | 0 | Schwelle genau erreicht zählt |
| 100 → 103 → 101 → 104 | 3 | 0 | der Rückgang um 2 setzt keine neue Referenz |
| 100 → 96 → 100 | 4 | 4 | der Abstieg setzt die Referenz auf 96 |
| 100 → (keine Höhe) → 104 | 4 | 0 | fehlender Punkt wird übersprungen |
| 100 → 105 → 100 → 105 | 10 | 5 | zweimal 5 hinauf, einmal 5 hinab |
| Abschnitt 1: 100, Abschnitt 2: 102 → 105 | 3 | 0 | keine Verbindung von 100 zu 102 |
| 100 (einzige Höhe) | unbekannt | unbekannt | weniger als zwei Höhen |

## 7. Start und Gesamtzeit eingeben

Enthält eine FIT-Session weder GPS-Punkte noch Zeitangaben, fragt der Importdialog Start und Gesamtzeit ab. Eine GPX-Datei ohne Zeitangaben wird abgewiesen (Abschnitt 3). Start und Gesamtzeit sind danach bei jeder Aktivität jederzeit korrigierbar (z. B. falsche Geräteuhr). Messpunkte und Segmentversuche behalten ihre Gerätezeit, die Originalwerte bleiben erhalten, die Herkunft ist „manuell korrigiert“.

Lokale Eingaben gelten in der Zeitzone der TOML und werden als UTC gespeichert. Bei der doppelten Herbstzeit wählt der Benutzer, eine im Frühjahr nicht existierende Zeit rückt eine Stunde nach hinten und wird mit Hinweis angezeigt (02:30 wird 03:30).

## 8. Duplikate und Konfliktdialog

**Erkennung**: Ein identischer Hash bedeutet identische Datei (Abschnitt 3). Sonst gilt eine Aktivität als vermutetes Duplikat, wenn alle vorhandenen Vergleichswerte passen:

| Kriterium | Vorgabe |
| :--- | :--- |
| Start | ±60 s |
| Gesamtzeit | ±5 % |
| Distanz | ±5 % (nur wenn beide Seiten eine Distanz haben) |

Der Vergleich verwendet die Originalwerte, nicht manuell korrigierte Werte. Fehlt ein Vergleichswert, wird er übersprungen.

**Optionen** bei einem vermuteten Duplikat: nicht importieren, zusätzlich importieren, oder die vorhandene Aktivität ersetzen.

**Ersetzen**:
- Die ersetzende Aktivität übernimmt die ID der alten, Inhalt sowie Datei- und Sessionzuordnung werden getauscht.
- Die alte Fassung wird endgültig ersetzt und bleibt nur in bestehenden Sicherungen erhalten. Bestehende Sicherungen werden nicht verändert.
- Vorausgewählt und abwählbar ist die Übernahme von Titel, Beschreibung, Ort und Sportklasse. Der Untertyp ist ebenfalls übernehmbar.
- Jedes Ersetzungsziel kann pro Vorgang nur einmal gewählt werden.
- Die Originaldatei der alten Aktivität wird nur entfernt, wenn keine Aktivität mehr darauf verweist (ADR-0011).

**Sicherheit**: Relevante Konflikte werden vor der Ausführung erneut geprüft. Hat sich seit der Bestätigung etwas Relevantes geändert, wird neu bestätigt, unabhängige Änderungen lösen keine Rückfrage aus. Vor der Bestätigung ändert sich nichts.

**Abbruch und Fehler**: Schließt der Browser bei unbestätigtem Konflikt, findet kein Import statt und der Upload wird bereinigt. Es gibt keine dauerhafte Konfliktwarteschlange. Bereits bestätigte Jobs werden sicher abgeschlossen oder zurückgenommen. Bei Fehler oder Neustart bleiben die alten Datenbank- und Dateistände erhalten.

Gespeichert wird so: Das Original wird vor der Transaktion vom Staging in die Ablage umbenannt (`<sha256>.<format>`, ADR-0011), danach schreibt eine Transaktion `original_files`, Aktivitäten, Abschnitte, Aufträge, Audit-Einträge und den Datenstand. Scheitert die Transaktion, wird ein dabei neu abgelegtes Original wieder entfernt. Ein beim Neustart noch laufender Vorgang wird `rolled_back` (Code `interrupted`), ein neu abgelegtes, nicht referenziertes Original entfällt. Ersetzen löscht die Auswertungen der alten Fassung (Versuche, Hysteresewerte) und stellt die Aktivität neu in die Auswertung.

### 8.1 Mehrfach-Upload

Für die Erstbefüllung und größere Nachträge kann der Importdialog mehrere Dateien auf einmal annehmen. Die API bleibt unverändert, jede Datei ist ein eigener Importvorgang (RFC-0003).

1. Der Browser lädt die gewählten Dateien nacheinander hoch, immer nur ein Upload zur Zeit. Abgewiesene Dateien (Format, Größe, Kopf-Prüfsumme, `gpx_without_time`) erscheinen mit Grund in der Liste.
2. Ein Vorgang ist **ohne Konflikt**, wenn keine wählbare Session Duplikatkandidaten hat und keine Eingabe von Start und Gesamtzeit braucht.
3. Ein Vorgang ohne Konflikt wird direkt nach seiner Prüfung ohne Rückfrage bestätigt (geändert am 08.11.2026, vorher Sammelbestätigung): `import` für alle wählbaren Sessions, `skip` für gesperrte. Gesperrte Sessions allein sind kein Konflikt, sie werden gemeldet und übersprungen. Der Titel bleibt der nach Abschnitt 6, geändert wird er auf der Detailseite (RFC-0006).
4. Vorgänge mit Duplikat oder Zeiteingabe und Vorgänge, deren Bestätigung mit 409 `import_state_changed` scheitert, warten auf eine Einzelentscheidung. Eine Datei wird als Ganzes gespeichert, auch ihre Sessions ohne Konflikt warten dann mit. So landet auch ein Duplikat innerhalb derselben Auswahl, etwa Edge-FIT und Strava-FIT derselben Fahrt, in der Einzelentscheidung: Die zuerst hochgeladene Datei ist dann schon gespeichert, die zweite ist ihr Duplikat.
5. Ein Import aus einem Serververzeichnis gibt es nicht. Unbestätigte Vorgänge räumt das Backend wie bisher auf, es gibt keine dauerhafte Warteschlange.

### 8.2 Aufbewahrung von Vorgängen (C12)

| Was | Aufbewahrung |
| :--- | :--- |
| Unbestätigter Vorgang (`staged`, `awaiting_decision`) samt Staging-Ordner | 24 Stunden ab Upload, danach verwaist und entfernt |
| Abgeschlossener Vorgang (`done`, `failed`, `rolled_back`) | 30 Tage ab Abschluss |
| Abgeschlossener Auftrag in `jobs` (`done`, `failed`) | 30 Tage ab Abschluss |
| Audit-Eintrag (C11, RFC-0011) | ein Jahr |

Aufgeräumt wird beim Start und danach stündlich. Die importierten Aktivitäten und Originale sind davon nicht betroffen. Beim Start bleiben im Staging nur die Ordner offener Vorgänge, alles andere wird entfernt.

## 9. Offene Punkte

1. Entfällt: Staging und Wiederanlauf sind in ADR-0011 und Abschnitt 8 festgelegt (C1, M1b).

## 10. Abruf aus Garmin Connect

Neue Aktivitäten und die gesamte Historie kommen ohne Importdialog direkt aus Garmin Connect, über die **persönliche Garmin-API** (Schlüssel aus Garmin Connect → Einstellungen → Persönlicher API-Schlüssel, Laufzeit 30 Tage bis 1 Jahr). Entscheidungen Uli 14.11.2026: in TrackCraft eingebaut (ein Weg für Gesamt- und laufenden Import), Titel nach der TrackCraft-Regel (Abschnitt 6, nicht der Garmin-Name), vorhandene Aktivitäten werden nicht doppelt angelegt, sondern verknüpft.

- **Einstellungen** in der Datenbank (`garmin_settings`, Migration 8), pflegbar in der Oberfläche: Schlüssel (nur schreibbar, nie in Antworten oder im Log, C7; Teil der Vollsicherung), regelmäßiger Abruf an/aus, Intervall 5–1440 min (Vorgabe 30), Anfangsdatum (frühester Starttag in der Konfigurationszone, leer = gesamte Historie). Ein neuer Schlüssel wird beim Speichern sofort bei Garmin geprüft. Basis-URL und Pause zwischen Downloads stehen in der TOML (RFC-0021).
- **Ablauf eines Laufs:** Token holen (24 h gültig, je Lauf neu), Aktivitätenliste seitenweise lesen (100 je Seite, neueste zuerst) und erst danach jede noch unbekannte Aktivität laden (so kennt die Oberfläche die Gesamtzahl und zeigt den Fortschritt „x von n“) (ZIP mit genau einer FIT, entpackt mit der Importgrenze aus Abschnitt 2) und durch die normale Importstrecke schicken (Staging, Prüfung, Duplikate, Speichern, Auswertungsauftrag). Ein Planer prüft jede Minute, ob ein Lauf fällig ist; „Jetzt abrufen“ startet sofort. Es läuft höchstens ein Lauf.
- **Gesamtimport:** Bis die Liste einmal bis zum Ende bzw. bis zum Anfangsdatum gelesen ist (`backfill_done`), liest jeder Lauf die ganze Liste. Danach endet ein Lauf an der ersten Seite ohne Neues. Ein früheres oder gelöschtes Anfangsdatum setzt `backfill_done` zurück.
- **Garmin-Typ statt Dateisport:** Der Garmin-Typ (`activityType.typeKey`, z. B. `road_biking`, `cross_country_skiing_ws`) ersetzt den `source_sport` der Datei, wenn sie genau eine Session hat (Uli: Garmin kennt auch nachträglich geänderte Typen, alte FIT-Dateien sagen oft nur `generic`). Die Zuordnung (Abschnitt 4) bringt Vorgaben für die Garmin-Typen mit.
- **Entscheidung je Session ohne Rückfrage:** frei → importieren; gleiche Datei schon vorhanden oder Duplikatkandidat (Abschnitt 8) → nicht anlegen, nur die Garmin-ID mit der vorhandenen Aktivität verknüpfen (`linked`); gesperrt (Fahrzeug, Punktgrenze) oder ohne Startzeit → überspringen (`locked`, mit Grund). Titel, Beschreibung und Änderungen vorhandener Aktivitäten bleiben unberührt.
- **Herkunft** in `garmin_activities` (eine Zeile je Garmin-ID, Status `imported`, `linked`, `locked`, `rejected`, `failed`). Jede Garmin-ID wird nur einmal bearbeitet; nur `failed` (Netz, HTTP-Fehler) wird bei den nächsten Läufen erneut versucht, höchstens fünfmal. Löscht der Benutzer eine abgerufene Aktivität, bleibt der Eintrag ohne Verweis stehen, und der Abruf bringt sie nicht zurück. „Erneut versuchen“ in der Oberfläche vergisst gesperrte, abgewiesene und aufgegebene Einträge (etwa nach geänderten Grenzen).
- **Fehler:** Schlüssel abgelaufen oder ungültig (401/403) und Ratenbegrenzung (429) brechen den Lauf ab; der Status nennt den Grund (`garmin_key_invalid` mit „neuen Schlüssel eintragen“, `garmin_rate_limited`). Drei Netzfehler hintereinander brechen ebenfalls ab. Eine bei Garmin gelöschte Aktivität (404) oder ein unlesbares Archiv wird `rejected`.
- **Netz:** Das Backend braucht dafür ausgehendes HTTPS zu `personal.connectapi.garmin.com` (rustls mit den Mozilla-Wurzelzertifikaten, auf allen Plattformen gleich).
- Noch offen: Anzeige neuer Aktivitäten im Dashboard; Gesundheitsdaten.
