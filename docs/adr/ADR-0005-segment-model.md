# ADR-0005: Segmentmodell

Status: Akzeptierte Regeln, Piggy 006.

Vollständige geordnete Referenzgeometrie statt Start-/Zielpaar. Ersteller verweist auf festen Benutzer. Eine Sportklasse cycling/ebike/running/skiing (skiing seit Schemaversion 6), unabhängig von Aktivitätsuntertyp; aus der Aktivität vorausgewählt und änderbar, bei unknown Pflichtauswahl. Name/Beschreibung editierbar, Referenz/Richtung/Sportklasse fest.

Erstellen nur aus fertig importierter Aktivität, genau ein Aufzeichnungsabschnitt. Referenz wird kopiert, kein Bezug zu Originaldatei oder Quellaktivität; die Quellaktivität liefert über das Matching den ersten Versuch. Ohne verfügbare Karte keine Segmenterstellung. Kartenvorschau/gekoppelte Start-/Zielregler auf vorhandenen GPS-Punkten, konkrete Runde eindeutig. Mindestens zwei gültige GPS-Punkte, Mindestlänge initial 20 m einschließlich konfigurierbar, nur bei Erstellung; Zeit/Höhe optional. Rundkurse erlaubt.

Neue Segmente automatisch gegen Historie prüfen, Fortschritt/unvollständigen Verlauf zeigen. Keine Favoriten/Folgen/Schwierigkeit/freies Zeichnen in V1.

Detailquellen [Segmentbedienung](../rfc/RFC-0016-segment-user-experience.md) und [Matching](../rfc/RFC-0002-segment-matching.md), Schema [RFC-0001](../rfc/RFC-0001-data-schema.md). Keine erfundene endgültige Geo-Implementierung.
