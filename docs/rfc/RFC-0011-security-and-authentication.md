# RFC-0011: Sicherheit und Zugriff

Status: Konsolidierte Regeln, 2026-10-29 (Piggy 006).

## Betriebsgrenze

Ein Benutzer/Haushalt/eigener Rechner (Windows-PC oder Docker-Host, RFC-0005). V1 ausschließlich LAN/VPN, kein direkter Internetzugriff auf TrackCraft. Keine Router-Portfreigabe oder öffentlich zugängliche Proxyroute. Jeder Anwendungszugriff erlaubt Lesen, Ändern und Löschen.

Kein App-Login, keine Passwortverwaltung, Login-Sessions oder Rollen in V1. Der feste Benutzer in `users` mit TOML-Profil und Besitzer-/Erstellerreferenzen ist ein Datenobjekt, kein authentifiziertes Konto. Keine Benutzerauswahl; Profil nicht in UI editierbar. Argon2, Authentik, Keycloak, OIDC, Passkeys/MFA sind spätere Optionen, keine Implementierungspflichten für V1.

## Transport, Dienst und Container

HTTPS für alle regulären LAN-/VPN-Clients. Unter Windows terminiert das Backend TLS selbst (rustls, TLS 1.2 und 1.3, Zertifikat aus `[tls]`, RFC-0021), unter Docker der Reverse Proxy. Clientvertrauen, lokale Namensauflösung, Ausstellung/Erneuerung dokumentieren; dauerhafte Zertifikatswarnungs-Ausnahme kein regulärer Betrieb. Der private Schlüssel ist nur für das Dienstkonto lesbar und liegt nicht in der Sicherung; die TOML-Referenzkopie enthält nur die Pfade. Die eigene TrackCraft-CA (RFC-0005) ist per Namensbeschränkung auf die Heimnetz-Namen begrenzt, ihr Schlüssel ist nur für Administratoren lesbar und dem Dienst nicht zugänglich. Wer der CA auf einem Gerät vertraut, vertraut damit nur Zertifikaten für diese Namen.

Windows: Dienstkonto `NT SERVICE\TrackCraft` ohne Administratorrechte, Schreibrecht nur auf Daten- und Sicherungsordner, Konfiguration und Programmordner nur lesbar. Eingehende Firewallregel nur für den TLS-Port, nur `trackcraft.exe`, nur Profil „Privat“. Kein HTTP-Port.

Docker: Backendport nicht direkt veröffentlichen, interne Containerverbindung darf HTTP. Non-root, Least Privilege, soweit möglich readonly Filesystem. Konfiguration readonly, Zugriff auf Daten-/Sicherungs-Volumes nur soweit erforderlich.

## Daten und externe Dienste

Positions-, Aktivitäts- und Gesundheitsdaten lokal; keine Telemetrie oder externe Trainingssynchronisation. Externe Hintergrundkarte ausdrücklich erlaubt, Internet nötig; IP/Kartenausschnitte beim Anbieter können betrachtete Orte verraten. Kein Upload vollständiger Aktivitätstracks dafür. OSM-Standardkarte gemäß Nutzungsrichtlinie, Attribution Pflicht. Ortsvorschlag (RFC-0017): Ohne eigenen Treffer fragt das Backend OpenStreetMap Nominatim mit der auf gut 10 m gerundeten Startkoordinate einer Aktivität; abschaltbar unter Einstellungen → Orte.

PWA keine dauerhafte Offline-Trainings-/API-Kopie auf Clients, nur statische UI-Assets. Flüchtiger Arbeitsspeichercache bleibt möglich.

Sicherung unverschlüsselt einschließlich TOML-Referenzkopie und Benutzerprofil (nur Anzeigename). Sicherungsziel (USB-Datenträger, zweite Platte oder Netzwerkfreigabe) physisch und zugriffsseitig schützen, Verlust-/Lesbarkeitsrisiko dokumentieren. HTTPS bleibt davon unberührt. Profilangaben nur soweit für die UI benötigt ausgeben, Logredaktion ausdrücklich auch dafür vorsehen.

## Eingaben und Operationen

Request-/Typ-/Bereichsvalidierung, keine internen Fehlerdetails an Clients. FIT/GPX-Uploads auf Format, Struktur und Ressourcen prüfen; Limits 50 MB je Datei und 100.000 Punkte je Aktivität (TOML). Abgewiesene Dateien nicht aufbewahren.

- **XML (GPX)**: Eine Datei mit `<!DOCTYPE` wird abgewiesen, GPX braucht keine DTD. Externe Entitäten und Netzwerkzugriffe werden nie aufgelöst, erlaubt sind nur die fünf vordefinierten Entitäten und Zeichenreferenzen. Damit sind XXE und Entitätsexpansion ausgeschlossen.
- **Dateipfade**: Der Dateiname des Uploads (`import_jobs.original_filename`) dient nur der Anzeige und dem Log und wird nie Teil eines Pfads. Originale werden unter einem serverseitig erzeugten Namen aus `sha256` und Format abgelegt (`original_files.relative_path`). Jeder Pfad aus Datenbank oder Sicherung muss relativ sein, darf kein `..` enthalten und muss nach dem Auflösen unter dem Archiv-Basispfad liegen (ADR-0011), sonst bricht die Operation ab.

Keine beliebige Serverpfadauswahl bei Restore; Auswahl auf konfiguriertes Verzeichnis begrenzen. Untrusted Sicherungen vor Anwendung auf Integrität/Kompatibilität und sichere Pfade prüfen. Ältere/nicht vergleichbare Stände verlangen Warnung/Bestätigung; kein ungeprüftes Restore.

Audit für Import, Ersetzen, Löschen, Backup, Restore; V1 hat keine Loginereignisse. Ablage in der Tabelle `audit_log` (RFC-0001), eine Zeile je Ereignis mit Zeitpunkt, Aktion, Objekt, Ergebnis (`ok`, `failed`) und kurzen Details als JSON (z. B. Importvorgang, Session, Dateiname), geschrieben in derselben Transaktion wie die Änderung. Aufbewahrung ein Jahr (C11). Die Tabelle ist Teil der Datenbank und damit der Sicherung. Keine Tokens, Passwörter oder Sessioncookies loggen.

## Browserschutz

Festgelegt am 01.11.2026. Ohne Login muss die Anwendung selbst unerwünschte Requests aus dem Browser abwehren, LAN allein schützt nicht. Bedrohung ist eine fremde Webseite, die ein Benutzer im LAN oder über VPN öffnet:

- **CSRF**: Die Seite löst Requests an TrackCraft aus, z. B. einen `multipart`-Upload per Formular oder ein `DELETE` per `fetch`.
- **DNS-Rebinding**: Die Seite lässt ihren eigenen Namen auf die Adresse des TrackCraft-Rechners zeigen und kann Antworten dann als eigene Herkunft lesen.

Andere Geräte im Netz sind hier nicht Angreifer, die Vertrauensgrenze bleibt das Netz (ADR-0012).

**Erlaubte Herkunft**: `[server] allowed_origins` (RFC-0021) nennt die Adressen, unter denen TrackCraft im Browser aufgerufen wird, z. B. `https://trackcraft.home.arpa`. Nur sie gelten als eigene Herkunft.

**Prüfungen im Backend** für jeden Request unter `/api/v1`, vor dem Routing:

1. **Host**: Der `Host`-Header muss Host und Port einer erlaubten Herkunft entsprechen, sonst 403 `host_not_allowed`. Das verhindert DNS-Rebinding, auch über einen Zugang ohne TLS-Namensprüfung. Ausgenommen sind `GET /health` und `GET /ready` für den Container-Healthcheck. Unter Windows gilt die Prüfung auch für die ausgelieferten Frontend-Dateien.
2. **Fetch-Metadaten**: Ist `Sec-Fetch-Site` gesetzt und weder `same-origin` noch `none`, wird der Request mit 403 `origin_not_allowed` abgewiesen, auch beim Lesen.
3. **Origin**: `POST`, `PATCH` und `DELETE` brauchen einen `Origin`-Header aus `allowed_origins`, sonst 403 `origin_not_allowed`. Ein fehlender `Origin` wird abgewiesen. Browser senden ihn bei diesen Methoden immer, andere Clients gibt es in V1 nicht.
4. **Content-Type**: JSON-Bodys nur als `application/json`, `multipart/form-data` nur beim Upload (`POST /activities/import`), sonst 415 `unsupported_media_type`. Ein HTML-Formular kann damit keinen JSON-Request nachbilden.
5. `GET` und `HEAD` ändern nie etwas.

**Kein CORS**: Backend, Nginx und Reverse Proxy (soweit vorhanden) senden keine `Access-Control-Allow-*`-Header, ein Preflight (`OPTIONS`) erhält keine Freigabe. Fremde Seiten können Antworten nicht lesen.

**Kein CSRF-Token und kein Cookie**: Ohne Sitzung gibt es nichts, was ein Token binden könnte. Die Prüfung von Host und Origin ersetzt es.

**Security-Header**: Sie gelten auf beiden Plattformen gleich und stehen nur hier. Unter Windows setzt das Backend sie auf alle Antworten (Frontend-Dateien, API, Fehler), unter Docker Nginx im Frontend-Container, auch auf weitergeleitete API-Antworten (RFC-0005):

| Header | Wert |
| :--- | :--- |
| `Content-Security-Policy` | `default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data: https:; connect-src 'self'; font-src 'self'; worker-src 'self'; manifest-src 'self'; object-src 'none'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'` |
| `Strict-Transport-Security` | `max-age=31536000` |
| `X-Content-Type-Options` | `nosniff` |
| `X-Frame-Options` | `DENY` |
| `Referrer-Policy` | `strict-origin-when-cross-origin` |
| `Cross-Origin-Opener-Policy` | `same-origin` |
| `Permissions-Policy` | `camera=(), microphone=(), geolocation=(), payment=(), usb=()` |

Das Backend setzt auf API-Antworten zusätzlich `Cache-Control: no-store` (keine dauerhafte API-Kopie im Client), `Cross-Origin-Resource-Policy: same-origin` und `X-Content-Type-Options: nosniff`, damit der Schutz unter Docker nicht allein an Nginx hängt.

Begründungen:

- **`img-src https:`** statt nur des Kartenhosts: Die Tile-URL steht in der TOML, die nur das Backend liest, Nginx liefert feste Header. Damit beide Plattformen dieselbe CSP haben, gilt das auch unter Windows. Fremde Bilder führen keinen Code aus, `script-src` und `connect-src` bleiben auf `'self'`. Eine Beschränkung auf den Kartenhost ist eine spätere Option.
- **`style-src 'self'`**: Leaflet und Recharts setzen Stile per Skript, das erlaubt die CSP. Braucht eine Bibliothek trotzdem Inline-Stile, wird die Ausnahme hier mit Grund ergänzt. `'unsafe-inline'` und `'unsafe-eval'` für Skripte sind ausgeschlossen.
- **`Referrer-Policy`**: Beim Laden der Kacheln sieht der Kartenanbieter nur die Herkunft (den Namen der TrackCraft-Adresse), keine Seitenpfade. Die OSM-Nutzungsrichtlinie verlangt einen gültigen Referer, deshalb nicht `no-referrer`.
- **HSTS** ohne `includeSubDomains` und `preload`, weil der Name im Heimnetz liegt.
