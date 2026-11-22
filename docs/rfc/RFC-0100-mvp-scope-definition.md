# RFC-0100: V1-Umfang (MVP Scope Definition)

Status: Bestätigte Scopeentscheidungen, Piggy 009; Matrix im Konsolidierungsreview an die überarbeiteten Fachregeln angeglichen.

## MVP-Scope Matrix (v1.0 vs Post-MVP)

| Modul / Komponente | Version 1.0 (MVP) | Post-MVP (v1.1+) |
| :--- | :--- | :--- |
| **Segment Matching** | Zweistufig: Kandidatensuche (Puffer, Richtung) und Endprüfung gegen die vollständige Referenz, alle Parameter aus der TOML, asynchrone Auswertung (RFC-0002) | Flexible/dynamische Radien |
| **Import Pipeline** | FIT / GPX Parser, Mehrfach-Upload mit Speichern ohne Rückfrage bei Dateien ohne Konflikt, Sessionauswahl, Duplikaterkennung, Outlier-Filter (TOML Schwellenwerte), Fahrzeugerkennung sperrt die Session, Kennzahlen (RFC-0008) | Drittanbieter-Sync APIs, Aktivität kürzen, Ort aus den Startkoordinaten bestimmen (Reverse-Geocoding; Datenschutz nach RFC-0011 offen: externer Dienst sieht den Startpunkt, Alternative lokale Ortsliste) |
| **PWA & Offline** | Installierbar, App-Shell und statische Assets offline, keine Offline-Trainingsdaten, kein Login | Offline-Karten-Preloading |
| **Karte** | Eine externe Standardkarte (OSM), Tile-URL und Attribution aus der TOML (RFC-0020) | Satellit/Topo, lokale Tiles, Heatmap |
| **Backup/Restore** | Eine Vollsicherung in ein konfiguriertes Ziel, Restore im laufenden Backend (RFC-0012) | nicht festgelegt |
| **Datenschutz** | Grundlegender Lokaldaten-Schutz | Privatsphäre-Zonen (Anonymisierungs-Radien) |
| **Statistik & PRs** | Bestzeiten, Rankings, Aggregationen, Aktivitätsrekorde | CP-Leistungskurven, FTP-Schätzung |
| **Equipment (`RFC-0010`)** | *Aus v1.0 gestrichen* | Komplettes Equipment- & Wartungs-Tracking |
| **Oberfläche (`RFC-0006`)** | Kachel-Design, Aktivitätenliste mit Summenzeile, Detail mit zwei Kurven, Segmentseite mit Bestzeiten-Entwicklung, Strecken als Segmentart, Statistik je Jahr | Kalender, manuelle Erfassung ohne Datei, konfigurierbare Spalten, frei anordenbare Karten, Vergleich zweier Aktivitäten, Marken in Karte und Diagramm, Kartenfärbung und Linienfarbe, HF-Zonen, Runden, freier Statistik-Zeitraum |
