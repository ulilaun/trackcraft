# RFC-0021: Zentrale Anwendungskonfiguration

Status: Neufassung, Piggy 011 (Entwurf zur Freigabe). Vollständiger Schlüsselkatalog mit Beispieldatei, abgeleitet aus ADR-0001, 0006, 0007, 0011, 0014, RFC-0003, 0005, 0007, 0008, 0012, 0013, 0016, 0017 und 0020.

## 1. Grundsätze

- **Technische Bezeichner** sind ab V1 unveränderlich, auch wenn sich der Produktname ändert: Konfigurationsdatei `trackcraft.toml`, Umgebungsvariable `TRACKCRAFT_CONFIG`, Testvariable `TRACKCRAFT_TEST_CORPUS`, Container `trackcraft-frontend` und `trackcraft-backend`, Netz `trackcraft-network`, Windows-Dienst `TrackCraft` mit Datenordner `%ProgramData%\TrackCraft` (RFC-0005), Sicherungsformat `trackcraft-backup` (RFC-0012). Der angezeigte Name kommt aus der Markendatei des Frontends (RFC-0006).
- Die TOML-Datei ist die einzige Anwendungskonfiguration, schreibgeschützt. Standardpfad unter Windows `%ProgramData%\TrackCraft\config\trackcraft.toml`, im Container `/config/trackcraft.toml` (RFC-0005). `TRACKCRAFT_CONFIG` wählt nur den Dateipfad, es gibt keine Einzelwert-Umgebungsvariablen.
- Die Datei wird beim Start streng geprüft: Fehlende oder ungültige Datei, unbekannte Schlüssel, fehlende Pflichtwerte und ungültige Wertebereiche oder Kombinationen verhindern den Start. Nur dokumentierte optionale Werte haben Vorgaben.
- Änderungen wirken nach einem Neustart, es gibt kein Reload.
- Alle Werte der Matching-Parameter, Hysterese und Duplikaterkennung stehen hier und nirgends sonst (nicht in der Datenbank, nicht pro Segment).

## 2. Schlüsselkatalog

Spalte „Wirkung“: **Neuberechnung** löst eine versionierte Neuberechnung aus und ist sicherungsrelevant. **Import** wirkt nur auf künftige Importe. **Betrieb** hat keine fachliche Wirkung und ist nicht sicherungsrelevant. „Frontend“ bedeutet, dass der Wert in der öffentlichen Teilmenge (Abschnitt 4) enthalten ist.

| Schlüssel | Pflicht / Vorgabe | Wirkung | Frontend |
| :--- | :--- | :--- | :--- |
| `[app] timezone` | optional, Vorgabe `Europe/Berlin` | Neuberechnung (Kalenderzuordnung) | ja |
| `[user] display_name` | optional (Setup trägt ihn ein) | nur Spitzname des ersten Benutzers beim ersten Start; danach führt die Datenbank, Änderung auf der Kontoseite | ja |
| `[server] allowed_origins` | Pflicht (Liste, mindestens ein Eintrag, z. B. `["https://trackcraft.home.arpa"]`) | Betrieb | nein |
| `[server] listen` | optional, Vorgabe `0.0.0.0:8080` (Adresse und Port; Windows-Beispiel `0.0.0.0:443`) | Betrieb | nein |
| `[server] static_dir` | optional (Ordner des Frontend-Builds; gesetzt: Backend liefert das Frontend aus und setzt die Security-Header, Windows. Leer: nur API, Docker mit Nginx) | Betrieb | nein |
| `[tls] cert_file` | optional (PEM-Zertifikatskette; gesetzt: Backend spricht auf `listen` nur HTTPS) | Betrieb | nein |
| `[tls] key_file` | optional (PEM-Schlüssel, nur zusammen mit `cert_file`) | Betrieb | nein |
| `[paths] database_dir` | Pflicht (Windows `C:\ProgramData\TrackCraft\data\database`, Container `/data/database`) | Betrieb | nein |
| `[paths] activities_dir` | Pflicht (Windows `…\data\activities`, Container `/data/activities`) | Betrieb | nein |
| `[paths] logs_dir` | Pflicht (Windows `…\data\logs`, Container `/data/logs`) | Betrieb | nein |
| `[backup] target_dir` | Pflicht (Sicherungsziel: USB, zweite Platte oder Netzwerkfreigabe; unter Windows Laufwerkspfad oder UNC-Pfad, im Container separat eingebunden) | Betrieb | nein |
| `[backup] migration_dir` | Pflicht (Windows `…\data\backups`, Container `/data/backups`) | Betrieb | nein |
| `[backup] daily_time` | optional, Vorgabe `03:00` (in `timezone`); eine auf der Seite Daten gesetzte Uhrzeit hat Vorrang | Betrieb | nein |
| `[logging] level` | optional, Vorgabe `info` (`error`, `warn`, `info`, `debug`, `trace`) | Betrieb | nein |
| `[monitoring] low_disk_free_gb` | optional, Vorgabe `1.0` | Betrieb | nein |
| `[maps] tile_url` | optional, Vorgabe OSM-Standard | Betrieb | ja |
| `[maps] attribution` | optional, Vorgabe OSM-Hinweis | Betrieb | ja |
| `[maps] track_max_points` | optional, Vorgabe `2000` (2 bis 100000; Messpunkte je Abschnitt für Karte und Diagramm der Detailseite, gleichmäßig ausgedünnt, C2) | Anzeige | ja |
| `[import] max_file_size_mb` | optional, Vorgabe `50` | Betrieb | ja |
| `[import] max_points_per_session` | optional, Vorgabe `100000` | Betrieb | ja |
| `[import] upload_timeout_minutes` | optional, Vorgabe `60` | Betrieb | nein |
| `[import.duplicates] start_tolerance_s` | optional, Vorgabe `60` | Import | nein |
| `[import.duplicates] duration_tolerance_ratio` | optional, Vorgabe `0.05` | Import | nein |
| `[import.duplicates] distance_tolerance_ratio` | optional, Vorgabe `0.05` | Import | nein |
| `[import.sport_class_mapping]`, `[import.sub_sport_catalog]`, `[import.sub_sport_mapping]` | **veraltet**: beim ersten Start mit Schemaversion 7 einmalig in die Datenbank übernommen, danach ignoriert mit Hinweis im Log; ohne Prüfung beim Laden. Die Zuordnung steht in der Datenbank (RFC-0008 Abschnitt 4) | – | – |
| `[processing.elevation] hysteresis_m` | optional, Vorgabe `3.0` | Neuberechnung (Aufstieg aus Messpunkten; Segmenthöhen werden abgeleitet) | nein |
| `[processing.outlier_filter] enabled` | optional, Vorgabe `true` | Import | nein |
| `[processing.outlier_filter] default_max_speed_kmh` | optional, Vorgabe `100.0` | Import | nein |
| `[processing.outlier_filter.max_speed_kmh]` | optional (Tabelle `source_sport` auf km/h, Untersport erbt vom Sport) | Import | nein |
| `[processing.vehicle_detection] enabled` | optional, Vorgabe `true` | Import | nein |
| `[processing.vehicle_detection] min_duration_s` | optional, Vorgabe `30` | Import | nein |
| `[processing.vehicle_detection] window_s` | optional, Vorgabe `300` | Import | nein |
| `[processing.vehicle_detection] max_pause_s` | optional, Vorgabe `60` | Import | nein |
| `[processing.vehicle_detection.peak_speed_kmh]` | optional (Tabelle `source_sport` auf km/h, Untersport erbt vom Sport, ohne Eintrag keine Prüfung) | Import | nein |
| `[processing.vehicle_detection.avg_speed_kmh]` | optional (wie `peak_speed_kmh`) | Import | nein |
| `[segments] minimum_length_m` | optional, Vorgabe `20.0` | nur bei Segmenterstellung, kein Gate rückwirkend | ja |
| `[segments] route_edge_m` | optional, Vorgabe `1000.0` | nur bei Streckenerstellung: größter Abstand von Start/Ziel zum Rand des Abschnitts (RFC-0016) | ja |
| `[segments] route_loop_radius_m` | optional, Vorgabe `750.0` | automatische Kategorie von Strecken: Start und Ziel höchstens so weit auseinander = Rundkurs bzw. Hin & zurück; beim Start neu eingeordnet (RFC-0016) | ja |
| `[segments.matching] matching_radius_meters` | optional, Vorgabe `20.0` | Neuberechnung | nein |
| `[segments.matching] max_vector_angle_deg` | optional, Vorgabe `45.0` | Neuberechnung | nein |
| `[segments.matching] max_offset_m` | optional, Vorgabe `15.0` | Neuberechnung | nein |
| `[segments.matching] tolerance_m` | optional, Vorgabe `20.0` | Neuberechnung | nein |
| `[segments.matching] min_coverage_ratio` | optional, Vorgabe `0.95` | Neuberechnung | nein |
| `[segments.matching] direction_reference_m` | optional, Vorgabe `5.0` | Neuberechnung | nein |
| `[segments.matching] max_backstep_m` | optional, Vorgabe `5.0` | Neuberechnung | nein |
| `[segments.matching] max_inner_gap_m` | optional, Vorgabe `30.0` | Neuberechnung | nein |
| `[segments.matching] max_inner_gap_ratio` | optional, Vorgabe `0.02` (Lücke höchstens max(`max_inner_gap_m`, Anteil · Länge)) | Neuberechnung | nein |
| `[segments.matching] max_interpolation_gap_s` | optional, Vorgabe `15` | Neuberechnung | nein |
| `[segments.matching] coverage_connection_s` | optional, Vorgabe `15` | Neuberechnung | nein |
| `[segments.route_matching] tolerance_m` | optional, Vorgabe `35.0` | Neuberechnung | nein |
| `[segments.route_matching] max_inner_gap_m` | optional, Vorgabe `200.0` | Neuberechnung | nein |
| `[segments.route_matching] matching_radius_meters` | optional, Vorgabe `35.0` | Neuberechnung | nein |
| `[segments.route_matching] recording_edge_m` | optional, Vorgabe `100.0` | Neuberechnung | nein |
| `[segments.route_matching] max_vector_angle_deg` | optional, Vorgabe `90.0` | Neuberechnung | nein |
| `[segments.route_matching] direction_reference_m` | optional, Vorgabe `50.0` | Neuberechnung | nein |
| `[garmin] api_base_url` | optional, Vorgabe `https://personal.connectapi.garmin.com` (nur `https://`, `http://` nur für 127.0.0.1/localhost in Tests) | Betrieb | nein |
| `[garmin] request_delay_ms` | optional, Vorgabe `1000`, höchstens `60000` (Pause vor jedem Download) | Betrieb | nein |
| `[secrets] key_file` | optional, absoluter Pfad, Vorgabe `secrets.key` im Ordner der Konfigurationsdatei. Schlüssel für gespeicherte Geheimnisse (RFC-0011, #10); fehlt die Datei, legt das Setup bzw. der Start sie an. Im Container `/data/secrets.key`, weil `/config` schreibgeschützt eingehängt ist | Neustart | nein |
| `[places] nominatim_url` | optional, Vorgabe `https://nominatim.openstreetmap.org` (nur `https://`, `http://` nur für 127.0.0.1/localhost in Tests); Rückwärtssuche des Ortsvorschlags (RFC-0017), etwa für eine eigene Nominatim-Instanz | Betrieb | nein |
| `[demo] enabled` | optional, Vorgabe `false`. `true`: Hinweisband „Demo mit erfundenen Daten“; beim Start werden alle Daten um ganze Tage verschoben, sodass die letzte Aktivität gestern war. Nur mit einer Demo-Datenbank (`trackcraft demo mark`, Migration 20), sonst bricht der Start ab. `trackcraft demo start` schreibt eine eigene Konfiguration mit `true` | Neustart | nein |

Feste Werte in V1 (nicht konfigurierbar): genau eine Vollsicherung. Die Zahl aufbewahrter Sicherungsstände (Vorgabe fünf) wird auf der Seite Daten eingestellt, nicht in der TOML.

Schlüssel, Anfangsdatum und Intervall des Garmin-Abrufs stehen nicht in der TOML, sondern in der Datenbank und werden in der Oberfläche gepflegt (RFC-0008 Abschnitt 10). Ebenso, ob der Ortsvorschlag OpenStreetMap fragen darf (Einstellungen → Orte).

## 3. Prüfungen beim Start

- `timezone` ist ein gültiger IANA-Name. Die Zeitzonendaten sind im Programm enthalten (Windows liefert keine IANA-Datenbank), ihre Version steht in `GET /system`. `display_name` ist, wenn angegeben, nicht leer.
- Jeder Eintrag in `allowed_origins` ist eine Herkunft der Form `https://host` oder `https://host:port`, in Kleinbuchstaben, ohne Pfad, Query und abschließenden Schrägstrich, und kommt nur einmal vor (Browserschutz, RFC-0011).
- `listen` ist eine gültige Socketadresse. `cert_file` und `key_file` sind beide gesetzt oder beide leer, die Dateien sind lesbar und passen zusammen. Ist `static_dir` gesetzt, ist `[tls]` Pflicht: Wer das Frontend direkt ausliefert, spricht HTTPS (RFC-0011). Ohne `[tls]` lauscht das Backend nur auf HTTP für den internen Containerbetrieb.
- Alle Pfade sind absolut nach den Regeln der Plattform (unter Windows mit Laufwerk oder als UNC-Pfad, kein verbundener Laufwerksbuchstabe für Netzwerkfreigaben). `target_dir` ist von `database_dir`, `activities_dir`, `logs_dir` und `migration_dir` verschieden; verglichen wird nach Auflösen und unter Windows ohne Unterscheidung der Groß-/Kleinschreibung. `database_dir` liegt nicht auf einer Netzwerkfreigabe und auf demselben Laufwerk bzw. Dateisystem wie `activities_dir` (Staging, ADR-0011).
- Alle Grenzen und Toleranzen sind größer 0. `min_coverage_ratio` liegt in (0, 1]. `max_vector_angle_deg` liegt in (0, 90]. Die Verhältnisse in `import.duplicates` liegen in (0, 1].
- Jede Geschwindigkeit im Outlier-Filter und in der Fahrzeugerkennung ist größer 0. `window_s` ist größer als `min_duration_s`.
- `daily_time` hat das Format `HH:MM`.
- Die Berechnungsparameter (Wirkung „Neuberechnung“) bilden einen Parameter-Hash, der mit `calculation_versions` verglichen wird. Weicht er vom veröffentlichten Stand ab, wird eine Neuberechnung angestoßen (RFC-0007, RFC-0009).

## 4. Öffentliche Teilmenge für das Frontend

Das Frontend erhält nur die Werte mit „Frontend“ in der Tabelle: Zeitzone, Kartenquelle, Attribution und Punktzahl der Detailseite, Importlimits (zur Vorabprüfung), Mindestlänge für Segmente, Untertypkatalog sowie Anzeigename. Pfade, Sicherungs- und Betriebswerte werden nicht ausgeliefert. Die Antwort von `GET /config` ist im Vertrag (`ConfigResponse`) festgelegt.

## 5. Beispieldatei

Beispiel für das Referenzgerät unter Windows. Windows-Pfade stehen in einfachen Anführungszeichen (TOML-Literalstring), damit `\` kein Escape ist. Für Docker entfallen `listen`, `static_dir` und `[tls]`, und die Pfade lauten wie in RFC-0005 (`/data/...`, Sicherungsziel z. B. `/backup-target`).

```toml
[app]
timezone = "Europe/Berlin"

[user]
display_name = "Beispielname"

[server]
allowed_origins = ["https://trackcraft.home.arpa"]   # Adressen, unter denen TrackCraft im Browser aufgerufen wird
listen          = "0.0.0.0:443"
static_dir      = 'C:\Program Files\TrackCraft\web'

[tls]
cert_file = 'C:\ProgramData\TrackCraft\config\tls\server.pem'       # von setup/cert verwaltet (eigene CA, RFC-0005)
key_file  = 'C:\ProgramData\TrackCraft\config\tls\server-key.pem'

[paths]
database_dir   = 'C:\ProgramData\TrackCraft\data\database'
activities_dir = 'C:\ProgramData\TrackCraft\data\activities'
logs_dir       = 'C:\ProgramData\TrackCraft\data\logs'

[backup]
target_dir    = 'E:\TrackCraft-Sicherung'                     # USB-Laufwerk; Netzwerkfreigabe als UNC-Pfad, z. B. '\\ds214\backup\trackcraft'
migration_dir = 'C:\ProgramData\TrackCraft\data\backups'   # nur lokale Migrationssicherung, nur die letzte
daily_time    = "03:00"

[logging]
level = "info"

[monitoring]
low_disk_free_gb = 1.0

[maps]
tile_url    = "https://tile.openstreetmap.org/{z}/{x}/{y}.png"
attribution = "© OpenStreetMap-Mitwirkende"
track_max_points = 2000   # Messpunkte für Karte und Diagramm der Detailseite

[import]
max_file_size_mb       = 50
max_points_per_session = 100000
upload_timeout_minutes = 60

[import.duplicates]
start_tolerance_s        = 60
duration_tolerance_ratio = 0.05
distance_tolerance_ratio = 0.05

# Die Sportart-Zuordnung steht seit Schemaversion 7 in der Datenbank (Einstellungen -> Sportarten).

[processing.elevation]
hysteresis_m = 3.0

[processing.outlier_filter]
enabled               = true
default_max_speed_kmh = 100.0

[processing.outlier_filter.max_speed_kmh]
running       = 35.0
running-trail = 30.0
cycling       = 110.0
e_biking      = 100.0
hiking        = 25.0
swimming      = 12.0
alpine_skiing = 130.0

[processing.vehicle_detection]
enabled        = true
min_duration_s = 30
window_s       = 300
max_pause_s    = 60

[processing.vehicle_detection.peak_speed_kmh]
running  = 35.0
cycling  = 90.0
e_biking = 90.0

[processing.vehicle_detection.avg_speed_kmh]
running  = 20.0
cycling  = 70.0
e_biking = 70.0

[segments]
minimum_length_m = 20.0
route_edge_m     = 1000.0
route_loop_radius_m = 750.0

[segments.matching]
matching_radius_meters  = 20.0
max_vector_angle_deg    = 45.0
max_offset_m            = 15.0
tolerance_m             = 20.0
min_coverage_ratio      = 0.95
direction_reference_m   = 5.0
max_backstep_m          = 5.0
max_inner_gap_m         = 30.0
max_inner_gap_ratio     = 0.02
max_interpolation_gap_s = 15
coverage_connection_s   = 15

# Strecken: großzügigere Werte, alle übrigen wie [segments.matching] (RFC-0002 Abschnitt 4.8)
[segments.route_matching]
matching_radius_meters = 35.0
tolerance_m            = 35.0
max_inner_gap_m        = 200.0
recording_edge_m       = 100.0
max_vector_angle_deg   = 90.0
direction_reference_m  = 50.0
```

## 6. Offene Punkte

1. Aufbewahrungsdauer und Rotation der Logs (30 Tage und Dateinamen sind bisher nur ein Betriebsentwurf, RFC-0013).
2. Entfällt: Staging liegt fest in `database_dir/staging` (C1, ADR-0011).
