# RFC-0019: Export und Portabilität

Status: Für V1 neu gefasst, 2026-11-26 (#58). Ersetzt die Abgrenzung vom 2026-10-29 (Piggy 005), die Export für V1
ausschloss.

## Ziel

Wer seine Daten in TrackCraft steckt, bekommt sie vollständig wieder heraus: unveränderte Originaldateien, die
Fachdaten maschinenlesbar und die Spuren in einem Format, das andere Programme lesen. Der Export ist kein
Sicherungsersatz (RFC-0012) und wird in V1 nicht wieder eingelesen.

## Umfang V1

| Was | Wo | Endpunkt |
|---|---|---|
| Gesamtexport als ZIP | Einstellungen → Export | `GET /export` |
| Originaldatei einer Aktivität | Aktivitätsdetail | `GET /activities/{id}/original` |
| GPX einer Aktivität aus den aufbereiteten Punkten | Aktivitätsdetail | `GET /activities/{id}/gpx` |
| CSV der gefilterten Aktivitätenliste | Aktivitätenliste | keiner, entsteht im Browser aus `GET /activities` |

Alle drei Endpunkte sind lesend (GET), laufen durch den Browserschutz (RFC-0011) und antworten mit
`Content-Disposition: attachment`. Sie ändern nichts und schreiben kein Audit.

## Gesamtexport

**Entstehung:** Der Server baut das ZIP beim Herunterladen im Strom zusammen (keine Zwischendatei, kein Platzbedarf
auf dem Server, kein Hintergrundauftrag). Den Fortschritt zeigt der Browser. Bricht die Verbindung ab, endet der
Export ohne Rückstände und wird neu gestartet. Mehrere Exporte gleichzeitig sind erlaubt; sie behindern Import,
Sicherung und Neuberechnung nicht.

**Dateiname:** `trackcraft-export-JJJJ-MM-TT.zip` (Ortsdatum).

**Aufbau:**

```
originals/<relativer Archivpfad>   Originaldateien unverändert (FIT/GPX), wie im Archiv abgelegt
gpx/<JJJJ-MM-TT-HHMM>-<titel>.gpx  je Aktivität mit Position eine GPX aus den aufbereiteten Punkten
trackcraft-export.json             Fachdaten, als letzter Eintrag
```

Namen in `gpx/` bestehen aus Ortszeit des Starts und dem Titel (nur Buchstaben, Ziffern, `-`, höchstens 60 Zeichen);
bei Gleichheit folgt `-2`, `-3` usw. Das JSON nennt je Aktivität die Pfade ihrer Dateien im ZIP.

**Konsistenz:** Die Fachdaten werden zu Beginn gelesen, danach folgen die Dateien. Eine Originaldatei, die in der
Zwischenzeit gelöscht wurde oder im Archiv fehlt, wird übersprungen und in `missing_originals` aufgeführt; der Export
bricht deswegen nicht ab.

### Fachdaten-JSON (`trackcraft-export`, Formatversion 1)

Die Einträge verwenden dieselben Darstellungen wie die API (OpenAPI), damit es nur eine Beschreibung der Felder gibt.

| Feld | Inhalt |
|---|---|
| `format`, `format_version` | `"trackcraft-export"`, `1` |
| `created_at`, `app_version`, `schema_version`, `timezone` | Zeitpunkt (UTC), Programmversion, Datenbankschema, konfigurierte Zeitzone |
| `profile` | wie `GET /profile` |
| `activities` | wie `GET /activities` (alle, nach Startzeit), dazu `original` (`path`, `format`, `sha256`, `size_bytes`) und `gpx` (Pfad oder `null`) |
| `segments` | wie `GET /segments` (Segmente und Strecken), dazu `reference` mit der vollständigen Referenzgeometrie (`lat`, `lon`, `ele_m`, `dist_m`) |
| `segment_efforts` | alle Versuche des veröffentlichten Berechnungsstands (Segment, Aktivität, Position, Status, Zeiten, HF); Ränge und Bestzeiten sind daraus abgeleitet |
| `seasons` | wie `GET /seasons` |
| `hr_zone_sets`, `form_settings` | wie `GET /hr-zones` bzw. `GET /form/settings` |
| `plan` | `units` (alle Einheiten), `goals`, `templates` wie in `GET /plan` bzw. `GET /plan/templates` |
| `sport_mappings`, `sub_sports` | wie `GET /sport-mappings` bzw. `GET /sub-sports` |
| `place_settings` | wie `GET /places/settings` |
| `garmin` | Einstellungen des Garmin-Abrufs (`enabled`, `start_date`, `interval_minutes`, `configured`), **ohne Schlüssel** |
| `backup_settings` | `daily_time`, `keep` |
| `missing_originals` | Archivpfade, die beim Export fehlten |

Nicht enthalten: Geheimnisse (Garmin-Schlüssel, Zertifikate, Sitzungen; vgl. #10, RFC-0013 „Was ins Log darf“),
TOML-Konfiguration, Audit-Protokoll, Vorgänge, Importaufträge, Rohdaten der Berechnung (Messpunkte stecken in den
Originalen und GPX-Dateien).

Spätere Formatversionen dürfen Felder ergänzen; Entfernen oder Umdeuten erhöht `format_version`.

## Einzelne Aktivität

- **Original:** die Datei aus dem Archiv, Byte für Byte. Name `<JJJJ-MM-TT-HHMM>-<titel>.fit` bzw. `.gpx`.
  Mehrere Aktivitäten aus derselben Datei (Multisport) liefern dieselbe Datei. Fehlt sie im Archiv: `404 original_missing`.
- **GPX:** GPX 1.1 aus den aufbereiteten Punkten (nach Ausreißerfilter, RFC-0008), je Abschnitt ein `<trkseg>`,
  mit Höhe, Zeit und Herzfrequenz (Garmin `TrackPointExtension v1`); Titel als `<name>`, Beschreibung als `<desc>`,
  Sportklasse als `<type>`. Punkte ohne Position fehlen. Aktivität ohne Position: `409 activity_without_position`.

## CSV der Aktivitätenliste

Knopf in der Aktivitätenliste. Exportiert genau die aktuell gefilterte und sortierte Liste über alle Seiten, mit den
Spalten der Liste plus Titel, Ort, Untertyp und Startzeit. Kopfzeile und Zahlen folgen der eingestellten Sprache bzw.
dem Format der Oberfläche: deutsches Format mit `;` als Trenner und Dezimalkomma, sonst `,` und Punkt. UTF-8 mit BOM,
Zeilenende CRLF, Felder mit Trenner, Anführungszeichen oder Zeilenumbruch in `"…"`. Dateiname
`trackcraft-aktivitaeten-JJJJ-MM-TT.csv`.

## Spätere Optionen

Import eines Exportarchivs (Umzug in eine andere Installation), Export einzelner Segmente/Strecken als GPX,
CSV/JSON weiterer Listen.
