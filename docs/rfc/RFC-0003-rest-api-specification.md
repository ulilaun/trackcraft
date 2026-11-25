# RFC-0003: REST-Schnittstelle

Status: Neufassung, Piggy 011 (Entwurf zur Freigabe). Der exakte Vertrag (Pfade, Parameter, DTOs) steht in [docs/api/openapi.yaml](../api/openapi.yaml) (OpenAPI 3.1) und ist verbindlich. Dieses Dokument legt die Konventionen, die Fehlercodes und die Abläufe dahinter fest.

## 1. Grundsätze

- REST unter `/api/v1`, JSON für strukturierte Daten. Binär nur beim Datei-Upload (`multipart/form-data`, ein Datei-Part `file`).
- Kein App-Login, keine Session, keine Rollen (ADR-0012). Es gibt daher kein 401. 403 steht nur für die Prüfung von Host und Herkunft (RFC-0011).
- Kein Export, kein Download von Originalen oder Sicherungen, keine Konvertierung (ADR-0009). Trackdaten für Karte und Diagramm sind reguläre Leseoperationen.
- Der Benutzer ist fest. Aktivitäten und Segmente tragen ihn implizit, der Client wählt keinen Benutzer. Das Profil kommt nur aus der TOML und wird über `GET /config` gelesen, es gibt keine Schreib-API dafür.
- Equipment und Wartung sind nicht Teil von V1.0 (RFC-0010).
- Es gibt keinen Export-, Login- und Profilendpunkt, und keine Pfade für ausgeschlossene Funktionen.

## 2. Antwortform

- Erfolg: `{"data": ..., "meta": ...}`. `meta` steht nur bei Listen (`total`, `limit`, `offset`), bei Antworten mit Berechnungsstand (`calculation`) und bei Hinweisen (`notices`). Löschen liefert 204 ohne Body.
- Fehler: `{"error": {"code": "...", "message": "...", "details": {...}}}`. `code` ist stabil und snake_case, `message` eine kurze englische Beschreibung ohne interne Details, `details` optional (bei Validierungsfehlern `details.fields` mit `field`, `code`, `message`). Die Oberfläche übersetzt anhand des Codes.

## 3. Datentypen

| Thema | Regel |
| :--- | :--- |
| IDs | UUID als Zeichenkette (UUIDv7 vom Backend erzeugt) |
| Zeitpunkte | RFC 3339 in UTC mit Millisekunden und `Z`, z. B. `2026-10-22T08:15:03.250Z`. Ausnahme: In Track-Antworten sind Zeiten ganze Millisekunden seit 1970 (`t_ms`), um klein zu bleiben |
| Lokale Eingaben | Nur als `LocalTimeInput` (`local` ohne Zone, optional `occurrence`), gültig in der Zeitzone der Konfiguration. Der Server rechnet um. Nicht existierende Frühjahrszeit: eine Stunde später, Hinweis `local_time_shifted` in `meta.notices`. Doppelte Herbstzeit ohne `occurrence`: 400 `local_time_ambiguous` |
| Dauern | ganze Millisekunden, Feldname endet auf `_ms` |
| Strecken, Höhen | Meter, Feldnamen enden auf `_m` |
| Geschwindigkeit | `avg_speed_kmh`, vom Server berechnet (Distanz durch Gesamtzeit), `null` bei fehlender Distanz oder Gesamtzeit 0 |
| Unbekannt | `null`, nie 0 (RFC-0009). Summen in Statistiken zählen nur vorhandene Werte und nennen die Zahl der Aktivitäten ohne Wert |
| Rundung | Die API liefert ungerundete Werte. Rundung für die Anzeige übernimmt das Frontend. Auch Aktivitätsrekorde (`/stats`) kommen ungerundet; verglichen werden sie gerundet wie angezeigt (RFC-0009) |
| Sportklasse | `cycling`, `ebike`, `running`, `skiing`, `swimming`, `studio`, `unknown`. Segmente, Ranglisten und Rekorde kennen nur `cycling`, `ebike`, `running`, `skiing` |

## 4. Listen

- Offset-Paging: `limit` (Vorgabe 50, höchstens 200) und `offset` (Vorgabe 0). `meta.total` enthält die Gesamtzahl.
- Sortierung mit `sort` (Feldname, Minus davor für absteigend), erlaubte Werte je Endpunkt im Vertrag. Gleichstände sortiert das Backend stabil nach ID.
- Filter sind Query-Parameter je Endpunkt (z. B. `sport_class`, `sub_sport`, `from`, `to`, `q`). `from` und `to` sind lokale Kalendertage in der Konfigurationszone. Unbekannte Parameter werden mit 400 abgewiesen.

## 5. Ändern

Nur PATCH mit den zu ändernden Feldern. `null` löscht nur nullbare Felder. PUT wird in V1 nicht verwendet, weil es keine Ressource gibt, die vollständig ersetzt wird. Unbekannte Felder und Felder, die nicht änderbar sind (Track, Distanz, Segmentreferenz), führen zu 400 `validation_failed`. Gleichzeitige Änderungen werden nicht per Versionsprüfung abgesichert, weil es einen Benutzer gibt. Ausnahmen mit Zustandsbindung sind Import und Restore (Abschnitt 8 und 9).

## 6. HTTP-Status

| Status | Verwendung |
| :--- | :--- |
| 200 | Lesen und Ändern erfolgreich |
| 201 | Angelegt (Importvorgang, Segment) |
| 202 | Angenommen, läuft im Hintergrund (Import speichern, Sicherung, Restore, Wiederholung) |
| 204 | Gelöscht oder abgebrochen |
| 400 | Ungültige Anfrage oder Werte |
| 403 | Host oder Herkunft nicht erlaubt (RFC-0011), bei jedem Endpunkt möglich |
| 404 | Objekt nicht gefunden |
| 408 | Upload dauerte länger als `import_limits.upload_timeout_minutes` |
| 409 | Zustandskonflikt |
| 413 | Datei zu groß |
| 415 | Weder FIT noch GPX, oder unzulässiger `Content-Type` |
| 500 | Interner Fehler |
| 503 | Nicht bereit oder Schreiben kurz gesperrt, mit `Retry-After` |

## 7. Fehlercodes

| Code | Status | Bedeutung |
| :--- | :--- | :--- |
| `validation_failed` | 400 | Allgemeine Validierung, Felder in `details.fields` |
| `invalid_file` | 400 | Datei beschädigt oder nicht lesbar |
| `invalid_import_decision` | 400 | Session unbekannt oder gesperrt, Ersetzungsziel kein Kandidat oder doppelt, Aktion unpassend |
| `gpx_without_time` | 400 | GPX-Datei ohne Zeitangaben, also eine geplante Route und keine Aktivität (RFC-0008) |
| `time_input_required` | 400 | Start und Gesamtzeit fehlen bei einer Session ohne GPS und Zeitangaben |
| `local_time_ambiguous` | 400 | Doppelte Herbstzeit ohne `occurrence` |
| `invalid_point_selection` | 400 | Start-/Zielindex ungültig, kein gültiger Positionspunkt, Start nicht vor Ziel, oder Abschnitt nicht eindeutig |
| `segment_too_short` | 400 | Unter der Mindestlänge (RFC-0016) |
| `season_overlap` | 409 | Zeitraum überschneidet sich mit einer anderen Saison, `details.season` nennt sie (RFC-0009) |
| `season_name_taken` | 409 | Eine andere Saison hat diesen Namen |
| `route_already_matched` | 409 | Aktivität hat schon einen Versuch auf einer Strecke (RFC-0016) |
| `segment_direction_undeterminable` | 400 | Richtung an Start oder Ziel nicht bestimmbar (RFC-0016) |
| `backup_invalid` | 400 | Sicherung besteht die Vorprüfung nicht (Integrität, Format, Pfade) |
| `host_not_allowed` | 403 | `Host` entspricht keiner Adresse aus `allowed_origins` (RFC-0011) |
| `origin_not_allowed` | 403 | `Origin` fehlt oder ist nicht erlaubt, oder `Sec-Fetch-Site` meldet eine fremde Seite (RFC-0011) |
| `activity_not_found`, `segment_not_found`, `import_job_not_found`, `job_not_found`, `season_not_found`, `backup_not_found`, `operation_not_found` | 404 | Objekt nicht vorhanden |
| `not_found` | 404 | Unbekannter API-Pfad (kein Objekt-Code passend) |
| `method_not_allowed` | 405 | Methode für diesen Pfad nicht vorgesehen |
| `import_state_changed` | 409 | Relevanter Stand hat sich seit der Anzeige geändert, neu laden und erneut bestätigen |
| `import_not_confirmable` | 409 | Importvorgang ist nicht in `awaiting_decision` |
| `import_not_cancellable` | 409 | Importvorgang läuft bereits |
| `operation_running` | 409 | Sicherung oder Restore läuft schon |
| `backup_target_unavailable` | 409 | Sicherungsziel nicht erreichbar oder nicht beschreibbar |
| `restore_preview_stale` | 409 | Aktueller Stand hat sich seit der Vorschau geändert, neu prüfen |
| `job_not_retryable` | 409 | Auftrag ist nicht fehlgeschlagen |
| `upload_timeout` | 408 | Upload nach dem Timeout abgebrochen, nichts bleibt liegen (RFC-0008) |
| `file_too_large` | 413 | Über dem Limit (`import_limits.max_file_size_mb`) |
| `unsupported_format` | 415 | Weder FIT noch GPX |
| `unsupported_media_type` | 415 | Body nicht als `application/json`, oder `multipart/form-data` außerhalb des Uploads (RFC-0011) |
| `not_ready` | 503 | Backend startet oder migriert noch |
| `write_locked` | 503 | Schreibzugriffe kurz gesperrt (Sicherung, Restore) |
| `internal_error` | 500 | Unerwarteter Fehler, Details nur im Log |

Dieser Katalog ist die Quelle der Fehlercodes, der Vertrag verweist darauf. Weitere Codes sind zulässig, wenn sie hier ergänzt werden.

## 8. Import

1. `POST /activities/import` mit der Datei. Das Backend prüft die Limits (50 MB, 100.000 Punkte je Session, 1 h Timeout, RFC-0008), parst und liefert einen Importvorgang mit Sessions samt Duplikatkandidaten und einem `state_token`. Gespeichert wird nichts. Wird die Datei abgewiesen, entsteht kein Importvorgang und nichts bleibt liegen.
2. Eine Session ist `selectable`, `locked_existing` (identische Session schon importiert, bei GPX die identische Aktivität) `locked_limit` (Punktlimit) oder `locked_vehicle` (Fahrzeugabschnitt erkannt, RFC-0008 Abschnitt 5.1, `lock_reason` nennt Zeitraum, Geschwindigkeit und Grenze). Gesperrte Sessions sind nicht wählbar. Ist keine Session wählbar, etwa bei einer GPX-Datei mit Fahrzeugabschnitt, kann der Vorgang nur abgebrochen werden.
3. `POST /import/jobs/{id}/confirm` mit `state_token` und je Session einer Entscheidung: `import` (zusätzlich speichern), `replace` (mit `replace_activity_id`, optional `carry_over` für Titel, Beschreibung, Ort, Sportklasse, Untertyp) oder `skip`. Bei `time_input_required` sind `start_time` (lokal) und `total_time_ms` Pflicht. Optional setzt `title` den Titel der neuen Aktivität (sonst Titel nach RFC-0008), bei `replace` mit `carry_over.title` gilt der übernommene Titel. Ein Ersetzungsziel darf je Vorgang nur einmal vorkommen.
4. Vor der Ausführung prüft das Backend Kandidaten und Metadaten erneut. Bei entscheidungsrelevanter Änderung antwortet es mit 409 `import_state_changed`, bei unabhängigen Änderungen führt es aus. Danach 202 mit Status `running`, der Client fragt `GET /import/jobs/{id}` ab, bis `done`, `failed` oder `rolled_back` erreicht ist.
5. Die gewählten Sessions werden gemeinsam gespeichert oder gemeinsam zurückgenommen. `done` bedeutet: gespeichert. Die Auswertung (Matching, Statistik) läuft danach als Aufträge (`/jobs`) und ist wiederholbar, ohne die Datei erneut zu importieren (`POST /jobs/{id}/retry`).
6. Mehrfach-Upload ist eine Funktion des Dialogs: je Datei ein eigener Vorgang, nacheinander hochgeladen, Vorgänge ohne Konflikt bestätigt der Dialog ohne Rückfrage (RFC-0008 Abschnitt 8.1). Es gibt keinen Endpunkt für mehrere Dateien.
7. `DELETE /import/jobs/{id}` bricht einen unbestätigten Vorgang ab und bereinigt den Upload. Schließt der Browser ohne Bestätigung, räumt das Backend verwaiste Vorgänge nach 24 Stunden auf (RFC-0008 Abschnitt 8.2). Es gibt keine dauerhafte Konfliktwarteschlange.

## 9. Sicherung und Restore

- `GET /backups/status` zeigt den letzten Versuch samt Ergebnis, die letzte erfolgreiche Sicherung samt Revision, ausstehende Änderungen, den laufenden Vorgang und den Zustand des Ziels. Erfolg, keine Änderungen, Ziel nicht erreichbar und Fehler sind getrennte Ergebnisse (RFC-0012).
- `POST /backups` startet „Jetzt sichern“ (202, Vorgang). Ohne Änderung oder ohne Ziel endet der Vorgang als übersprungen, nicht als Erfolg. Läuft schon ein Vorgang, 409 `operation_running`.
- `GET /backups` listet die Stände im konfigurierten Ziel. `meta.target.state` unterscheidet erreichbar, nicht erreichbar und nicht beschreibbar, eine leere Liste bei erreichbarem Ziel heißt „keine Sicherung“. `id` ist eine undurchsichtige Referenz, kein Pfad.
- `POST /restores/preview` prüft einen Stand und liefert Vergleich (`older`, `same`, `newer`, `not_comparable`), Zählwerte, Warnungen und einen `preview_token`. Bei `older` und `not_comparable` ist ausdrückliche Zustimmung nötig.
- `POST /restores` mit `preview_token` und `acknowledge_warnings` führt den vollständigen fachlichen Ersatz aus (kein Merge), 202 mit Vorgang. Hat sich der aktuelle Stand seit der Vorschau geändert, 409 `restore_preview_stale`. Das Backend sperrt beim Umschalten Schreibzugriffe (503 `write_locked`).
- `GET /operations/{id}` zeigt Phase und Ergebnis von Sicherung und Restore.
- Kein Upload, kein Download, keine freie Pfadwahl.

## 10. Berechnungsstand

Auswertungen (Versuche, Bestzeiten, Statistik) sind versioniert (`calculation_versions`, RFC-0001). Antworten, die davon abhängen, tragen `meta.calculation` mit Status `none`, `current`, `running`, `stale` oder `failed` sowie ID und Zeitpunkt des veröffentlichten Stands. Während einer Neuberechnung zeigt die API den letzten vollständigen Stand als `stale`. Nachträge nach Datenänderungen ändern den Status nicht (RFC-0007). Gelöschte oder ungültige Beiträge werden nie als gültig ausgewiesen (RFC-0009).

## 11. Trackdaten

- `GET /activities/{id}/track` liefert je Abschnitt Arrays je Feld (`index`, `t_ms`, `lat`, `lon`, `ele_m`, `hr`, `dist_m`). Fehlende Werte sind `null`. `section` wählt einen Abschnitt, `max_points` dünnt gleichmäßig nach Index aus und behält ersten und letzten Punkt. `index` nennt die Position im Abschnitt und ist Grundlage für `start_index`/`end_index` beim Segment.
- `GET /segments/{id}/track` liefert die Referenz ohne Zeit und Herzfrequenz.
- `dist_m` ist die kumulierte WGS84-Strecke ohne Höhe, `null` bei Punkten ohne Position.
- Das Binärformat der Datenbank (RFC-0022) wird nie ausgeliefert.

## 12. Übersicht der Endpunkte

Die verbindliche Liste mit Parametern und DTOs steht im Vertrag, hier die Gruppen:

| Gruppe | Endpunkte |
| :--- | :--- |
| System | `GET /health`, `/ready`, `/version`, `/system`, `/config` |
| Aktivitäten | `GET /activities`, `GET/PATCH/DELETE /activities/{id}`, `GET /activities/{id}/deletion-preview`, `/track`, `/efforts` |
| Import | `POST /activities/import`, `GET /import/jobs`, `GET/DELETE /import/jobs/{id}`, `POST /import/jobs/{id}/confirm` |
| Segmente | `GET /segments`, `POST /segments/from-activity`, `GET/PATCH/DELETE /segments/{id}`, `/deletion-preview`, `/track`, `/leaderboard`, `/attempts` |
| Statistik | `GET /stats/dashboard`, `/stats` (Auswahl aus Zeitraum, Sportklasse, Untertyp), `/stats/periods` (Kurzsummen je Saison und Jahr) |
| Sicherung | `GET /backups/status`, `GET/POST /backups`, `GET /operations/{id}`, `POST /restores/preview`, `POST /restores` |
| Aufträge | `GET /jobs`, `GET /jobs/{id}`, `POST /jobs/{id}/retry` |

## 13. Prüfung des Vertrags

`docs/api/openapi.yaml` wurde syntaktisch geparst (ohne doppelte Schlüssel), alle Referenzen aufgelöst, Operations-IDs und Pfadparameter geprüft, und die eingebetteten Beispiele wurden gegen ihre Schemas validiert, ergänzt um Negativtests. Der offizielle OpenAPI-Validator war in der Arbeitsumgebung nicht installierbar und ist beim Aufbau des Repositorys nachzuholen (RFC-0014). Das Backend wird gegen den Vertrag getestet (Antworten gegen die Schemas).

## 14. Offene Punkte

1. Offizielle Validierung des Vertrags und Vertragstests im Backend (RFC-0014).
2. Ob der Vertrag zusätzlich Beispielantworten für alle Endpunkte erhält (bisher für die zentralen DTOs).
3. Entfällt (M1b): Die geprüften Sessions stehen in `import_jobs.sessions_json`, die Datei bleibt bis zum Abschluss in `staging/<vorgang-id>`. Der `state_token` ist ein Hash über Vorgang, geprüfte Sessions, Sperren, vorhandene Aktivitäten und die IDs der Duplikatkandidaten. Beim Bestätigen wird die Datei erneut gelesen; weicht das Ergebnis ab (z. B. nach geänderter Konfiguration), antwortet das Backend mit `import_state_changed`. Die Bestätigung antwortet erst nach abgeschlossener Speicherung, `status` ist dann schon `done` oder `failed`; das Speichern läuft in einem eigenen Task und wird auch bei abgebrochener Verbindung zu Ende geführt.
4. Erledigt (#57): Tabelle `operations` (RFC-0001), im Vertrag `Operation` unter `GET /operations/{id}`; `POST /backups` legt den Vorgang als `queued` an, ein zweiter gleichzeitiger antwortet 409 `operation_running`.
5. Rate- und Größenlimits der Proxys unter Docker (Reverse Proxy, Nginx) und des Backends unter Windows passend zu den Upload-Grenzen (RFC-0005, RFC-0008).
