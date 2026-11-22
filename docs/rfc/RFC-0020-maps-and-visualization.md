# RFC-0020: Karten und Visualisierung

Status: Konsolidierte V1-Regeln, 2026-10-29 (Piggy 006).

Leaflet ist die einzige Kartenbibliothek (Diagramme: Recharts, ADR-0004). Standardkarte ist die OSM-Standardkarte (`tile.openstreetmap.org`), Nutzung gemäß OSM-Nutzungsrichtlinie, Attribution „© OpenStreetMap-Mitwirkende“ Pflicht. Keine parallele MapLibre-Festlegung.

V1 nur eine externe Standard-Hintergrundkarte. Tile-URL und Attribution in TOML, Initialwert OSM-Standard. Satellit/Topo, lokale/offline Tiles und Heatmap nur spätere Optionen.

## Datenschutz und Verfügbarkeit

Internet erforderlich; IP und Kartenausschnitte beim Anbieter können betrachtete Orte verraten. Kein Upload kompletter Trainingsdateien/Tracks dafür. Ohne Internet oder bei Störung/Sperrung keine Hintergrundkarte: Track auf neutralem Hintergrund mit Hinweis, Segmenterstellung gesperrt; sonstige lokale Kernfunktionen bei erreichbarer NAS nicht blockieren.

PWA keine dauerhafte Offline-Trainingskopie. Ein Kartendienst ist kein Ersatz für die Verbindung zum TrackCraft-Rechner.

## Darstellung

Aktivitätsroute, Start/Ziel und bekannte Distanz, Segmentroute/Start/Ziel und Versuche, Höhenprofil aus geeigneten Daten. GPX-Tracks/Tracksegmente als getrennte Abschnitte ohne künstliche Verbindungslinien. Ohne GPS Hinweis statt erfundener Route.

Visualisierung fehlender/ungültiger Punkte und detaillierter Höhen-/Abstandskennzahlen benötigt noch den finalen Geo-/Datenvertrag.
