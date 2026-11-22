# RFC-0014: Tests und Qualitätssicherung

Status: Prüfanforderungen, Piggy 008. Keine implementierten Anwendungstests behauptet.

## Ebenen

Unit Domain/Geo/Services/Matching, Integration SQLite/Dateien/API/Jobs, E2E FIT/GPX bis Anzeige/Backup/Restore.

Backend fmt/clippy/test, Frontend lint/Vitest/Testing Library/build. Coverage ersetzt keine Golden Tests.

## Coverage

Festgelegt am 08.11.2026.

**Zählweise**: Zeilenabdeckung, also der Anteil ausgeführter Codezeilen an allen ausführbaren Zeilen eines Bereichs. Zweigabdeckung wird, soweit das Werkzeug sie liefert, nur angezeigt und ist kein Ziel. Ein Ziel ist erreicht, wenn der auf eine Nachkommastelle abgeschnittene Wert mindestens dem Zielwert entspricht (89,96 % zählt als 89,9 % und verfehlt 90 %).

**Backend**: `cargo-llvm-cov` mit der stabilen Toolchain. Gemessen wird im CI-Job unter Linux, damit es eine einzige, reproduzierbare Zahl gibt; der Windows-Job führt dieselben Tests ohne Messung aus. Es zählen alle Tests, die in CI laufen: Unit- und Integrationstests, öffentlicher Korpus und die öffentlichen Golden-Fälle. Der private Korpus (`TRACKCRAFT_TEST_CORPUS`) läuft nur lokal und zählt nicht mit.

| Bereich | Module (RFC-0004) | Ziel |
| :--- | :--- | :--- |
| Domain | `domain`, `geo`, `segment` | mindestens 90 % |
| Services | `services`, `import`, `jobs` | mindestens 80 % |
| Gesamt | ganzes Backend-Crate ohne Ausnahmen unten | mindestens 75 % |

Nicht gezählt: Testcode und Testgenerator, `main.rs`, das Modul `platform` (Windows-Dienst und Ereignisprotokoll, `cfg(windows)`; abgedeckt durch die Windows-Testfälle unter „Freigabe und Grenzen“). Weitere Ausnahmen nur mit Begründung in einer einzigen Liste im Repository (`backend/coverage-exclusions.txt`), nicht verstreut im Code.

**Frontend**: Vitest mit V8-Coverage, Zeilenabdeckung für die Logikordner `src/utils`, `src/api` und `src/hooks` (Formate, Zeitzonen, Fehlercodes, API-Zugriff), Ziel mindestens 70 %. Komponenten und Seiten haben kein Ziel; sie deckt die Abnahme über die Oberfläche ab.

**Wirkung in CI**: Bis einschließlich M5 nur Bericht. Jeder Pull Request zeigt die Werte je Bereich und die Änderung gegenüber `main` in der Zusammenfassung des Jobs, die Rohdaten (lcov) liegen als Artefakt bei. Ab M6 (v1.0) ist es ein Gate: Unterschreitet ein Bereich sein Ziel, schlägt `ci` fehl und der Release-Workflow (RFC-0015) bricht ab. Die Ziele werden nicht gesenkt, um einen PR durchzubringen; eine Änderung der Ziele ist eine Änderung dieses RFC.

## Fachliche Abnahmefälle

1. Ein fester Benutzer, stabile DB-ID, keine Login-/Benutzerauswahl. Profil Pflicht-Anzeigename, leer Startfehler, kein Geburtstag. Ohne `timezone` gilt `Europe/Berlin`. Unveränderter Abgleich keine neue Revision, tatsächliche Änderung sicherungsrelevant.
2. Besitzer Aktivitäten, Ersteller Segmente, Rekorde über Aktivität; Übernahme vorhandener Daten und Erhalt der Identität bei Umbenennung/Restore.
3. FIT/GPX inklusive fehlendem GPS/Distanz und positionslosen Sensorwerten. Ohne GPS-Punkte und Zeitangaben Abfrage von Start und Gesamtzeit im Importdialog. Korrektur von Start/Gesamtzeit: Messpunkte und Versuchszeiten unverändert, Kalender neu, Duplikatvergleich mit Originalwerten. Limits 50 MB/100.000 Punkte (Session gesperrt), Timeout 1 h, keine Aufbewahrung abgewiesener Dateien. Keine erfundenen Nullwerte. FIT-Zuordnung nach RFC-0008: `total_elapsed_time` statt `total_timer_time`, Ungültig-Werte und 0 als fehlend, `altitude` ohne `enhanced_altitude`, Records nach Sessionende, Zuordnung bei mehreren Sessions, Datei ohne Session-Message, ein Abschnitt trotz Timer-Stopps. FIT-Parser (ADR-0002): verkettete Dateien, Big-Endian, komprimierte Zeitstempel-Header, Developer-Felder mit und ohne `field_description`, falsche Prüfsumme und abgebrochene Datei werden abgewiesen. Parser-Update nur nach unverändertem Ergebnis über den Testkorpus. GPX nur `<trk>`, `<rte>`/`<wpt>` ignoriert, HF aus TrackPointExtension v1 und v2 mit beliebigem Präfix, GPX ganz ohne `<time>` abgewiesen. Outlier-Filter: schlechter erster Punkt wird verworfen und reißt keine guten Punkte mit, gleicher Zeitstempel verwirft den späteren Punkt, Punkt ohne Zeitstempel bleibt ungeprüft, verworfener Punkt behält Zeit, Höhe und HF ohne Position.
4. FIT-Sessionliste alle vorausgewählt/einzeln abwählbar, gewählte gemeinsam speichern/rücknehmen. Identische FIT fehlende Sessions ergänzen, vorhandene gesperrt/unverändert, Original einmal referenzieren.
5. GPX-Abschnitte erhalten, keine Geometrie-/Höhen-/Matchingverbindungen darüber.
6. Hash/Konkurrenz/Reservierungen, Duplikattoleranzen/fehlende Vergleichswerte, drei Optionen. Ein Ersetzungsziel pro Import einmal. Relevante Änderung während Dialog neue Bestätigung, unabhängige keine Rückfrage.
7. Ersetzen behält Aktivitäts-ID, Inhalt/Datei-/Sessionzuordnung ausgetauscht. Metadatenwahl. Fehler/Neustart alte DB-/Dateistände erhalten; späterer Auswertungsfehler ohne Rücknahme.
8. Browserabbruch unbestätigt kein Import, Verwaistenbereinigung; bestätigte Jobs sicher. Keine dauerhafte Konfliktwarteschlange. Titel aus dem Dialog (`title`) wird gespeichert, ohne ihn Titel nach RFC-0008, bei Ersetzen mit `carry_over.title` der alte. ZIP wird mit `unsupported_format` abgewiesen. Mehrfach-Upload: Dateien nacheinander, Vorgänge ohne Konflikt werden ohne Rückfrage gespeichert, Duplikat innerhalb derselben Auswahl (z. B. Edge-FIT und Strava-FIT) landet in der Einzelentscheidung, abgewiesene Dateien mit Grund in der Liste.
9. Segmenterstellung nur aus fertig importierter Aktivität, Referenz kopiert ohne Quellbezug, Quellaktivität ergibt ersten Versuch; Karte/Regler/vorhandene GPS-Punkte/ein Abschnitt/eindeutige Runde; ohne Karte gesperrt; Sportklasse vorausgewählt/änderbar. Genau 20 m erlaubt, darunter nicht; Konfigurationsänderung keine rückwirkende Wirkung. Zwei gültige Punkte, Zeit/Höhe optional. Ganze Aktivität als Strecke: Start und Ziel am ersten und letzten gültigen GPS-Punkt, kind route; Aufzeichnungsrand nahe Start/Ziel zählt bei Strecken als Passage; Artwechsel wertet die Historie neu aus, Name/Beschreibung nicht; Filter route_id nur mit gültigem Versuch. HF je Versuch zeitgewichtet, ohne HF leer.
10. Rundkurse/gleiche Koordinaten, aufeinanderfolgende Runden mit gemeinsamer Passage (n Runden n Versuche), Richtung 5-m-Bezug höchstens Segmentlänge, unbestimmbare Richtung Ablehnung. Start-/Zielquerlinie Richtung/Toleranz, echter Seitenwechsel, Streifen keine Passage, Punkt direkt auf Linie mit Fortsetzung auf anderer Seite. Mehrfache Startpassage: letzte zählt, Neustart nur bis Rückschrittgrenze, Achter-/Lassosegment wertbar.
11. Interpolation 15 s einschließlich; Abdeckungsverbindung eigener 15-s-Wert. Grenztests jeweils separat.
12. Vollständige Referenz-Endprüfung trotz vereinfachter Kandidatensuche; Längenquote statt Punktzahl, Intervalle nicht doppelt, Referenzreihenfolge, Parallelweg/Schleife/Kreuzung/Abkürzung. Kandidatensuche gegen den Trackverlauf: Start im Radius zwischen zwei Punkten, die beide außerhalb liegen, ergibt einen Kandidaten.
13. Rückschritt genau 5 m nicht automatisch ungültig, mehr ungültig (auf gerader Strecke messbar erst ab etwa 31 m, RFC-0002 4.2); innere Lücke genau 30 m zulässig, mehr ungültig (auch abgekürzte Kehre); Versatz bis genau 15 m ausgeglichen, größerer auf 15 m gekürzt; Toleranz 13 m nach Versatzausgleich und Abdeckung 95 % (geprüft mit echten Dateien, siehe Auswertung A1 und Golden-Segmente); neuer Versuch erst nach korrekter Startpassage.
14. Mehrdeutige Zuordnung weiterverfolgen, am Ziel unaufgelöst keine Wertung. Golden-Segmente aus echten Dateien siehe Abschnitt „Golden-Segmente“, Sonderformen (Rundkurs, Runden, Achter, Lasso, Parallelweg, Mehrdeutigkeit) aus dem Generator.
15. Pausen inklusive, ms-Rundung Start/Ziel/Differenz, Zehntelanzeige/halbe Werte. Gleicher Zeitstempel: erster Punkt zählt, weitere ignoriert; Punkt ohne Zeitstempel im Versuch macht ihn ungültig, andere Versuche wertbar; rückläufige Zeit ganzer Versuch ungültig.
16. UTC-only, explizite Zone vor Konfigurationsersatz, Herbst-Doppelzeit Auswahl, nicht existierende Frühjahrszeit eine Stunde später mit Hinweis (02:30 wird 03:30), UTC-Zeitpunkte bei Zonenwechsel unverändert.
17. Aktivitäts-/Segmentstart getrennte Perioden, Mitternacht/Sommerzeit/ISO-Wochenjahr; jede Aktivität hat einen Start.
18. WGS84 ohne Höhenanteil, Dateiwert (vorhanden und größer 0; 0 gilt als fehlend) vor GPS-Distanz. Gesamtzeit inklusive Pausen/Unterbrechungen: Dateiwert vor erstem/letztem GPS-Punkt vor manueller Eingabe, Herkunft gespeichert; keine Bewegungszeit.
19. Ein Durchschnitt Distanz/Gesamtzeit, null Dauer kein Wert, Herkunft angezeigt.
20. Höhen Dateiwert (größer 0) vor Hysterese, fehlende Höhe nicht 0. Hysterese mit Schwelle `hysteresis_m` inklusive über gespeicherte Punkte, Aufstieg und Abstieg, je Abschnitt summiert, unter zwei Höhen unbekannt; Beispiele aus RFC-0008, Beginn/Ende/Lücken. Keine Ausreißerfilterung. Geänderte Schwelle erzeugt neue Gesamtversion, Statistik und Rekorde bis zur Veröffentlichung veraltet.
21. Abgeleitete chronologische PR-Folge nach Nachimport/Korrektur/Löschen ohne gespeicherte Bestzeitenart, Abfragezeit über alle Segmente gemessen; Erste Bestzeit keine Verbesserung, Gleichstand Einstellung, stabile Tie-Breaks ohne neue Ergebnis-IDs. Zusammenfassungen ISO-Woche/Monat/Jahr je Sportklasse und gesamt, keine Trends.
22. Aktivitätsrekorde je Sportklasse: Distanz, Aufstieg, Gesamtzeit, Durchschnitt; jeder vorhandene Wert zählt, kein Mindestumfang, manuelle Gesamtzeit zählt, Vergleich gerundet wie angezeigt (10 m/1 m/1 s/0,1 km/h, Laufen Pace auf 1 s/km), Gleichstand: frühere Aktivität behält Rekord, unknown ausgeschlossen; keine Streak.
23–28. Entfallen in V1.0 (Equipment und Wartung, Post-MVP v1.1). Die Abnahmefälle stehen in RFC-0010.
29. Löschkaskaden Aktivitäten/Segmente, Original letzte Referenz, keine Wiederbelebung.
30. TOML einzige Quelle/readonly/strict/restart, Katalog und öffentliche Teilmenge, Versionierung/Abhängigkeiten; Erstellungsmindestlänge kein rückwirkendes Gate.
31. Versionierte Berechnung/atomare Veröffentlichung, letzter vollständiger Stand veraltet, Fehler/ohne Vorbestand, konkurrierendes Löschen. Nachtrag je Datenänderung nach RFC-0007, auch während laufender Gesamtversion.
32. Eine Sicherungsart in konfiguriertes Ziel (USB, zweite Platte oder Netzwerkfreigabe), bedingt/manuell/heutiges Nachholen, 03:00 in Anwendungszeitzone, Sicherung vor Migration lokal plus Kopie (lokal fehlgeschlagen kein Start), keine Doppelstarts, fünf validierte Stände, Überspringen kein Erfolg. Profiländerung und neuberechnungsrelevante TOML-Änderung lösen Sicherungsbedarf aus.
33. Konsistenter Zwischenstand/WAL/Originale, kurze Schreibsperre dann Übertragung ins Ziel ohne, Änderungen während Kopie, Fehler/Platzmangel/Retention/kein rekursiver Inhalt.
34. Restore leer/gefüllt, ältere/fremde Linien Warnung/Zustimmung an Revision gebunden, Vollersatz/kein Merge, Alt/Neu vollständig nach Absturz, Profil aktuelle TOML und Benutzer-ID aus Backup.
35. LAN/VPN/HTTPS/Ports/Upload/XML/Pfad/Origin, lokale Zertifikate; keine Login-Testpflicht. GPX mit DOCTYPE oder externer Entität abgewiesen, Uploadname mit `../` oder absolutem Pfad ohne Wirkung auf die Ablage, Pfad mit `..` aus Sicherung abgewiesen. Browserschutz (RFC-0011): fremder `Host` 403 außer Healthcheck, ändernder Request ohne oder mit fremdem `Origin` 403, `Sec-Fetch-Site: cross-site` und `same-site` 403 auch beim Lesen, Formular-POST (`text/plain`, `application/x-www-form-urlencoded`) 415, `multipart` außerhalb des Uploads 415, kein `Access-Control-Allow-*` auch nicht beim Preflight, Security-Header auf HTML-, API- und Fehlerantworten, `Cache-Control: no-store` auf API-Antworten. Windows: nur HTTPS auf `listen`, kein HTTP-Port, Security-Header vom Backend auf Frontend-Dateien, API und Fehlern, `static_dir` ohne `[tls]` Startfehler, Firewallregel nur Profil „Privat“, Dienst ohne Administratorrechte. Docker: Frontend-Port nur auf `127.0.0.1`, Reverse Proxy reicht `Host` unverändert weiter. Die UI läuft unter der CSP ohne Verstoß (Konsole), Karte mit Kacheln inklusive.
36. PWA ohne dauerhaften Trainingscache, OSM-Karte aus TOML mit Attribution, Kartenausfall: Track auf neutralem Hintergrund, Segmenterstellung gesperrt, sonst keine Blockade lokaler Kernfunktionen. Sport-Mapping mit Rückfall vom Untersport auf den Sport (FIT `sport-sub_sport`, GPX `<type>` normalisiert), Untertyp beim Import nur über den exakten Eintrag der Sportart-Zuordnung, Zuordnung aus der Datenbank mit einmaliger TOML-Übernahme und Vorgaben, die ein Update nicht zurückbringt, Untertyp als Katalogschlüssel mit Fremdschlüssel, Migration 7 wandelt Texte in Schlüssel, Umbenennen nur über den Namen, Löschsperre bei Verwendung, Untertypübernahme beim Ersetzen (entfällt bei unpassender Klasse). Titel aus GPX-Namen, sonst Ersatztitel mit lokalem Start. Keine ausgeschlossenen Export-/Socialfunktionen.

## Testkorpus

Festgelegt am 01.11.2026. Grundlage für Import-, Parser- und Matchingtests sowie für A1 (Schwellen und Parameter) und A2 (Golden-Geometrien).

### Ablage

Echte Dateien enthalten Positionen, z. B. den Startort zu Hause. Sie liegen deshalb nur lokal und nie in Git, auch nicht im privaten Repository.

| Teil | Ort | Inhalt | Läuft in |
| :--- | :--- | :--- | :--- |
| Öffentlich | `backend/tests/corpus/public/` | Beispieldateien des Garmin FIT SDK (nach dessen Lizenz) und erzeugte Dateien | CI und lokal |
| Privat | lokales Verzeichnis, Pfad über die Testvariable `TRACKCRAFT_TEST_CORPUS` | echte Dateien der eigenen Geräte und Apps | nur lokal |
| Manifest | `backend/tests/corpus/manifest.toml` | je Datei SHA-256, Quelle, Sonderfälle und erwartete Werte | CI und lokal |

- Erzeugte Dateien entstehen durch einen Generator im Testcode aus einer Beschreibung, nicht als Binärdatei von Hand. Er nutzt den Encoder von rustyfit (ADR-0002) bzw. schreibt GPX als Text.
- Das Manifest enthält für private Dateien nur Kennzahlen (Sessions, Start, Gesamtzeit, Distanz, Aufstieg, Punktzahl, verworfene Punkte, Sportklasse), keine Koordinaten. Es darf deshalb im Repository liegen.
- Das Manifest erzeugt der Korpustest selbst, wenn zusätzlich `TRACKCRAFT_UPDATE_MANIFEST` gesetzt ist (nur Test, keine Anwendungskonfiguration); jede Änderung daran ist bewusst und im Diff geprüft. Ohne diese Variable vergleicht der Test jede Datei mit dem Manifest und meldet neue Dateien, die noch fehlen. Erster Stand am 08.11.2026 (M1a): 215 Dateien.
- Fehlt `TRACKCRAFT_TEST_CORPUS`, werden die privaten Tests übersprungen und als übersprungen gemeldet. Vor einem Release und vor jedem Parser-Update laufen sie lokal vollständig (ADR-0002, RFC-0015).
- `TRACKCRAFT_TEST_CORPUS` gilt nur für Tests und ist keine Anwendungskonfiguration (RFC-0021).

### Quellen

| Quelle | Format | Liefert | Stand |
| :--- | :--- | :--- | :--- |
| Garmin Edge 530 | FIT | Rad (Straße, Mountainbike, Indoor), je eine Session, Abtastung 1 bis 3 s | 200 Dateien, 2023 bis 2026 |
| Garmin-Uhr (FIT-Produkt 2337) | FIT | Laufen (`running`, ohne Untersport) mit GPS, HF und Höhe, Abtastung 4 bis 7 s | 8 Dateien, 2023 bis 2024 |
| Strava-Export | FIT, GPX | „Originaldatei“ und GPX derselben Edge-Fahrten | 2 Fahrten, je FIT und GPX |
| SportTracks.mobi-Export | GPX | dieselben Edge-Fahrten mit eigenem Titel und Untertyp | 3 Dateien |
| Export aus Komoot oder Garmin Connect | GPX | geplante Route ohne Zeit | beizusteuern |
| Garmin FIT SDK | FIT | Multisport, Developer-Felder, Schwimmen, niedriger Akku | öffentlich |
| Generator | FIT, GPX | Sonder- und Fehlerfälle | im Repository |

Im privaten Verzeichnis bleibt die Ordnung nach Quelle erhalten (`edge/`, `uhr/`, `strava/`, `sporttracks/`). Wahoo und andere Hersteller sind nicht verfügbar, ihre Eigenheiten deckt nur der Generator ab, soweit sie bekannt sind.

### Pflichtfälle

| Fall | Quelle |
| :--- | :--- |
| Rad mit GPS, HF und Höhe nur in `altitude` | Edge |
| Indoor ohne GPS, `total_distance = 0` | Edge (1 Datei) |
| ohne Herzfrequenz | Edge (3 Dateien) |
| Ausreißer in der ersten Minute (Kaltstart) | Edge (3 Dateien), Generator |
| Fahrzeugabschnitt am Ende (Spitze und Schnitt) | Uhr (Lauf vom 07.01.2024), Generator |
| Fahrzeugabschnitt nur über den Schnitt erkennbar, GPX ohne Gerätegeschwindigkeit | Generator |
| Sprung im Distanzzähler ohne Fahrzeug | Edge (Fahrt vom 10.03.2024) |
| Laufen mit Smart-Recording (Abtastung über 1 s) | Uhr (8 Dateien) |
| Records ohne Position am Anfang (GPS noch ohne Fix) | Uhr (4 Dateien), Edge |
| Laufen mit Untersport (z. B. Trail) | Generator, Uhr falls vorhanden |
| Multisport, mehrere Sessions in einer Datei | FIT SDK, Generator |
| Records vor der ersten Session, Datei ohne Session-Message | Generator |
| verkettete Datei, Big-Endian, komprimierte Zeitstempel-Header | Generator |
| Developer-Felder mit und ohne `field_description` | FIT SDK, Generator |
| falsche Prüfsumme, abgebrochene Datei | Generator |
| GPX mit `<type>`, Namen, Höhe und HF-Erweiterung (`gpxtpx:hr`) | Strava |
| GPX ohne HF, Untertyp als `<type>` | SportTracks |
| GPX mit mehreren `<trk>`/`<trkseg>` | Generator, App-Export falls vorhanden |
| GPX ohne Zeitangaben (abgewiesen mit `gpx_without_time`) | Komoot-Route (beizusteuern), Generator |
| GPX ohne `<type>`, ohne Namen | Generator |
| GPX mit DOCTYPE oder externer Entität | Generator |
| Session über 100.000 Punkte, Datei über 50 MB | Generator |
| identische Datei (gleicher Hash) | Edge-Datei zweimal |
| vermutetes Duplikat bei gleichem Inhalt, anderem Hash | Edge-FIT und Strava-FIT |
| vermutetes Duplikat über Formate | Edge-FIT mit Strava- und SportTracks-GPX derselben Fahrt |

Eine Datei kann mehrere Fälle abdecken. Ein Pflichtfall ohne Datei ist ein offener Testmangel und wird im Manifest als fehlend geführt.

### Befunde aus den Beispieldateien

- **Sprünge**: Eine erste Auswertung zählt Sprünge über 110 km/h zwischen GPS-Punkten nach der ersten Minute in 57 der 200 Edge-Dateien und in 1 der 8 Läufe. Geprüft in A1, siehe unten.
- **Strava-„Original“**: Die FIT-Datei von Strava ist nicht bytegleich mit der Edge-Datei (andere Größe, einige Messages entfernt). Session und Records sind identisch. Der Hash erkennt sie nicht als identische Datei, nur die Duplikatprüfung (RFC-0008) fängt sie ab.
- **`source_sport` aus GPX**: Strava schreibt `mountain_biking` und `cycling`, SportTracks den eigenen Untertyp (`Mountain`, `Road`). `mountain_biking`, `mountain` und `road` stehen nicht im Beispiel-Mapping (RFC-0021) und ergäben `unknown`. Eingang für B3 und B4.
- **SportTracks-GPX** enthält keine Herzfrequenz und lässt Records ohne Position weg (8.363 Punkte bei 8.376 Records, davon 8.364 mit Position). Ein Import aus SportTracks hat deshalb keinen HF-Durchschnitt.

### Auswertung A1: Schwellen und Matching-Parameter

Ausgewertet am 06.11.2026 mit dem privaten Korpus (199 Edge-Fahrten mit GPS, 8 Läufe). Die Auswertung lief mit einem eigenen Leseskript außerhalb des Repositorys, sie ersetzt keine Anwendungstests.

**Outlier-Filter** (RFC-0008 Abschnitt 5), sequentiell wie spezifiziert:

- Rad, 110 km/h: 249 von 1,56 Mio. Punkten verworfen, in 58 Dateien, höchstens 2 Punkte am Stück, 3 davon in der ersten Minute. Das sind einzelne Sprünge, der Filter wirkt wie beabsichtigt.
- Lauf, 35 km/h: nur der Lauf vom 07.01.2024. Dort hat die Uhr nach dem Lauf etwa drei Minuten einer Fahrzeugfahrt mit bis zu 68 km/h aufgezeichnet, der Filter verwirft 134 Punkte am Stück. Die Dateiwerte (8,65 km statt rund 5,9 km) enthalten die Fahrt. Daraus folgt die Fahrzeugerkennung (RFC-0008 Abschnitt 5.1).
- Höchste Geschwindigkeiten: Rad 82 km/h Spitze und 62 km/h im 5-Minuten-Schnitt, Lauf (ohne den 07.01.2024) 13,8 km/h und 11,3 km/h.
- Keine Records mit gleichem, rückläufigem oder fehlendem Zeitstempel (B1 bleibt mit dem Generator zu prüfen).

**Abtastung**: Auch der Edge zeichnet in Bewegung nicht jede Sekunde auf (68 % der Intervalle 1 s, bis 10 s). Anteil der Bewegungszeit in Intervallen über:

| Lücke | Rad | Lauf |
| :--- | :--- | :--- |
| 5 s | 21,3 % | 83,8 % |
| 10 s | 0,1 % | 6,5 % |
| 15 s | 0 % | 0,3 % |

Mit 5 s für `max_interpolation_gap_s` wären 23 % der Rad- und 91 % der Laufversuche zeitlich ungültig, mit 15 s keiner.

**Matching** an wiederholt gefahrenen Strecken: Aus den Fahrten wurden Referenzen von 300 m, 1 km und 3 km geschnitten (250 Segmente) und alle anderen Fahrten derselben Strecke in Richtung als Versuch geprüft (3.938 Versuche, davon 22 Läufe). Beide Spuren sind GPS-Aufzeichnungen, die Referenz streut also mit. Die Spuren liegen im Median etwa 2 m auseinander, an den 5 % schlechtesten Stellen 4 bis 5 m.

| Rad: Abdeckung mindestens 95 % | 300 m | 1 km | 3 km |
| :--- | :--- | :--- | :--- |
| Toleranz 5 m | 65 % | 48 % | 46 % |
| Toleranz 10 m | 92 % | 82 % | 78 % |
| Toleranz 15 m | 98 % | 94 % | 93 % |

- Ein Teil der Fehlschläge sind echte Abweichungen (Umwege, andere Spur), die Quoten sind eher zu niedrig. 15 m wurde wegen paralleler Wege nicht gewählt.
- Die innere Lücke kostet bei 10 m Toleranz und 5 m Lücke weitere 3 bis 18 Prozentpunkte, bei 20 m weniger.
- Bei 12 bis 24 % der Versuche liegt kein Trackpunkt innerhalb von 15 m um Start oder Ziel, obwohl der Track vorbeiführt. Daher misst die Kandidatensuche zum Trackverlauf.
- Laufen: Das GPS der Uhr streut mit 4 bis 9 m im Median deutlich mehr, bei 10 m Toleranz bestehen nur 10 der 22 Versuche. Die Stichprobe reicht nicht für eigene Werte (A2).
- Korrigiert durch A2: Die Golden-Segmente zeigten, dass Versuche vor allem durch einen gleichbleibenden Versatz zur Referenz scheitern. Seitdem gelten Versatzausgleich bis 15 m, Toleranz 13 m und innere Lücke 30 m (knappster gefahrener Versuch 29 m).
- Nicht geprüft: `max_backstep_m`, `direction_reference_m`.

### Golden-Segmente (A2)

Festgelegt am 06.11.2026. Zehn Segmente aus drei Aktivitäten (Straße 05.07.2026, MTB 04.09.2026, Lauf 14.01.2024), die Einmündungen R4 und R5 mit der Straßenfahrt vom 19.07.2026 als Gegenstück.

- **Ablage**: Die Geometrien enthalten Koordinaten und liegen im privaten Korpus unter `golden/segments/<id>.gpx`. Die Erwartungen ohne Koordinaten stehen in `backend/tests/corpus/golden.toml`: je Segment Zweck, Quelle mit Abschnitt, und je Aktivität des Korpus mit derselben Sportklasse jeder erwartete Versuch mit Ergebnis und Dauer.
- **Prüfung**: Der Test legt die Segmente an und wertet den Korpus aus. Erwartet werden genau die genannten Versuche, keine weiteren. Ergebnis exakt, Dauer bis zur ersten Bestätigung durch die Anwendung mit 1 s Toleranz, danach exakt. Eine Abweichung ist ein Fehler oder eine bewusste Regeländerung, die die Erwartung im selben Pull Request anpasst.
- **Herkunft der Erwartungen**: Vorgerechnet mit einem vereinfachten Nachbau von RFC-0002 (ohne Rückschritt und Mehrdeutigkeit), jeder nicht gewertete Versuch auf der Karte von Ulrich geprüft. Ergebnis: Alle gefahrenen Versuche sind gültig, außer einem Lauf mit 25 s Aufzeichnungslücke.

| Segment | Zweck | Länge | Versuche | gewertet |
| :--- | :--- | :--- | :--- | :--- |
| R1 Schlussanstieg | viele Versuche, Ranking | 995 m | 33 | 33 |
| R2 Flach Straße | Referenz mit GPS-Versatz | 990 m | 32 | 32 |
| R3 Wendepunkt | Gegenrichtung zählt nie | 495 m | 27 | 27 |
| R4 Einmündung | Track mündet im Segment ein: kein Versuch | 1.099 m | 27 | 27 |
| R5 Zufahrt von der Seite | andere Route, ganzes Segment: Versuch | 999 m | 35 | 35 |
| M1 Kehren | zwei Kehren | 490 m | 11 | 11 |
| M2 Anstieg Wald | Wald-GPS, Versatz bis 13 m | 1.951 m | 11 | 11 |
| M3 Flach MTB | Grundfall MTB | 999 m | 24 | 24 |
| L1 Anstieg Lauf | Uhr, Smart-Recording, Kehren | 996 m | 6 | 6 |
| L2 Flach Lauf | Aufzeichnungslücke 25 s: kein Treffer | 999 m | 6 | 5 |

Befund: Ohne Versatzausgleich hätten 26 der 212 gefahrenen Versuche nicht gezählt. Die Versuche lagen über die ganze Länge 8 bis 13 m neben der Referenz, weil schon die Referenz eine GPS-Aufzeichnung mit Versatz ist. Nötige Toleranz ohne Ausgleich: Straße 15,5 m, MTB 22,5 m, Lauf 35 m. Nach Ausgleich: Straße 6,5 m, MTB 12,6 m, Lauf 11,1 m.

Echte Rundkurse, Runden, Achter, Lassos und Parallelwege kommen im Korpus nicht vor und bleiben beim Generator.

## Freigabe und Grenzen

Build/Lint/Tests, Migration/Kompatibilität und Restore prüfen. Referenzgerät ist Ulrichs Windows-11-Desktop-PC (RFC-0005); Abnahme, Restoretest und Zeitmessung dort, keine Performanceziele, Zeiten messen und dokumentieren. CI baut und testet auf Windows (`x86_64-pc-windows-msvc`) und Linux (`linux/amd64`, Docker-Images); Backendtests, Golden-Segmente und der öffentliche Korpus laufen auf beiden. Plattformfälle: Pfade mit Laufwerk und UNC, Leerzeichen im Pfad, Groß-/Kleinschreibung beim Vergleich der Pfade, durch Virenscanner kurz gesperrte Datei, Restore mit zuvor geöffneten Dateien, Sicherung unter Windows erstellt und unter Docker wiederhergestellt und umgekehrt, Zeitzone ohne IANA-Daten des Systems, Dienststart ohne Anmeldung, Herunterfahren während Import und Sicherung. Verteilung (RFC-0005, RFC-0015): `setup.cmd` aus dem Download-Ordner auf frischem Windows mit Vorgaben und mit eigenem Installationsordner, wiederholt ohne Änderung an vorhandener TOML, als Update in den bisherigen Ordner, Installationsordner im Benutzerordner abgewiesen; Zertifikat: Kette und Namen von einem Client akzeptiert, fremder Name abgewiesen (Namensbeschränkung), Neuausstellung bei geänderten Namen und unter 30 Tagen Restlaufzeit, Namen außerhalb der CA nur mit `--new-ca`; `setup` ohne Adminrechte mit klarer Meldung, `check-config` mit gültiger und ungültiger Datei ohne Schreibzugriff, Update von der Vorversion nach dem dokumentierten Ablauf mit Sicherung vor Migration, Start eines älteren Programms gegen neueres Schema verweigert ohne Datenänderung, `uninstall` lässt Konfiguration und Daten stehen, Release-ZIP enthält alle Dateien und passt zu `SHA256SUMS.txt`. Abnahme Desktop voll, Tablet/Smartphone Anzeige/einfache Bearbeitung/Import, Chromium-Browser und Safari iOS, Light/Dark nach System, keine Barrierefreiheitsprüfung. Restorezeit kein Gate.

Dieses Definitionspaket enthält keinen Anwendungscode und keine ausführbaren Golden-/Absturz-/API-/Import-/Restoretests. Dokumentprüfungen separat im Prüfprotokoll.
