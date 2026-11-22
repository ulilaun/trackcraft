# ADR-0001: Gesamtarchitektur

Status: Akzeptierte Grundentscheidungen, Piggy 006.

Eigene FIT-/GPX-Analyse auf einem eigenen Rechner im Heimnetz (Referenz: Windows-Desktop-PC, alternativ Docker-Host, ADR-0007), Einzelbenutzerbetrieb mit einem expliziten Benutzerobjekt in DB. Keine Telemetrie/Trainingssynchronisation.

- Rust/Axum/Tokio/Serde/SQLx, SQLite.
- React/TypeScript/Vite/TanStack Query/React Router/Tailwind CSS/Lucide Icons/Recharts.
- Leaflet/OSM-Standardkarte, Tile-URL und Attribution aus TOML.
- geo/geo-types/rstar, gpx/quick-xml/chrono, rustyfit als FIT-Parser (ADR-0002).
- Windows: ein Dienst `trackcraft.exe`, der Frontend und API ausliefert und HTTPS selbst terminiert. Docker: zwei Appcontainer React/Nginx und Backend, lokaler Reverse Proxy terminiert HTTPS.

LAN/VPN ohne App-Login, jeder Zugriff volle Rechte, kein öffentlicher Internetbetrieb. Hintergrundkarte ausdrückliche externe Ausnahme, IP/Ausschnitte beim Anbieter, keine vollständigen Trainingsdateien hochladen.

Ein fester Benutzer mit DB-ID, Profil in der Datenbank (Spitzname anfangs aus `[user]` der TOML, danach Kontoseite), Besitzer/Erstellerreferenzen. Keine Mehrbenutzer-/Loginfunktion. Zeitpunkte nur UTC, Konfigurationszone für Anzeige/Eingaben/Kalender und Importersatz.

FIT je gewählter Session mit gemeinsamer Originalreferenz, fehlende Sessions nachimportierbar; GPX je Datei mit Abschnitten. Start und Gesamtzeit Pflicht, notfalls beim Import abgefragt; keine Bewegungszeit. Importabschluss getrennt von Auswertung. Vollständige Segmentreferenz; [Matching](../rfc/RFC-0002-segment-matching.md) und [Datenmodell](../rfc/RFC-0001-data-schema.md) führen Details.

Equipment und Wartung sind nicht Teil von V1.0, sondern Post-MVP (v1.1): [RFC-0010](../rfc/RFC-0010-equipment-and-maintenance-management.md), [RFC-0100](../rfc/RFC-0100-mvp-scope-definition.md).

TOML einzige Konfigurationsquelle, Standard `%ProgramData%\TrackCraft\config\trackcraft.toml` (Windows) bzw. `/config/trackcraft.toml` (Container), readonly/strict/restart; nur `TRACKCRAFT_CONFIG` Pfadwahl. Globale UI erst später. API `/api/v1`, vollständige OpenAPI noch ausarbeiten.

Eine Vollsicherung in konfiguriertes Verzeichnis (USB, zweite Platte oder Netzwerkfreigabe); Backup/Restore mit Benutzeridentität/aktuellen TOML-Profilfeldern, sicheren Datei-/DB-Operationen und Revisionen. Keine Exportfunktion/Offline-Trainingskopie/Streak. Trainingsbelastung/Fitness sehr viel später.

Technik-/Fachrestpunkte nicht durch ADR implizit entschieden; keine Behauptung implementierter Software.
