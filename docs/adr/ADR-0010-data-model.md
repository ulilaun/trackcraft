# ADR-0010: Datenmodell

Status: Akzeptierte Semantik, Piggy 009; Strukturen im Konsolidierungsreview überarbeitet.

SQLite relationales Schema für Einzelbenutzer-Betrieb. IDs sind UUIDs (TEXT, UUIDv7, im Backend erzeugt), Zeitpunkte UTC in Millisekunden.

## Modellstrukturen

- `users`: der feste Benutzer mit Pflicht-Anzeigename. Ein Geburtstag ist in V1 nicht vorgesehen.
- `original_files`: archivierte Originaldatei (Pfad, SHA-256), von mehreren Aktivitäten (FIT-Sessions) gemeinsam nutzbar.
- `activities`: Metadaten, Sportklasse, Start/Gesamtzeit/Distanz mit Herkunft, Aufstieg aus der Datei, Auswertungsstatus (`processing_status`), gefilterte GPS-Outlier (`filtered_outliers_count`) und angewandter Speed-Schwellenwert (`max_speed_threshold_kmh`).
- `track_sections`: Messpunkte als ein BLOB je Aufzeichnungsabschnitt, Format in [RFC-0022](../rfc/RFC-0022-track-point-binary-format.md).
- `segments`: vollständige Referenzgeometrie (BLOB), Start-/Zielkoordinaten, Sportklasse. Keine Vektoren, Radien, Winkel oder Höhen; diese stammen aus Referenz und TOML.
- `segment_efforts`: Versuche je Berechnungsversion mit Status, Dauer in ms und Abweichungen an Start/Ziel. Rang, Bestzeitenart und Zeitersparnis werden nicht gespeichert, sondern abgeleitet.
- `activity_calculations`: Aufstieg nach Hysterese je Aktivität und Berechnungsversion; gilt, wenn die Datei keinen Aufstieg liefert.
- `calculation_versions`: versionierte, gemeinsam veröffentlichte Auswertungsstände. Datenänderungen werden im veröffentlichten Stand nachgetragen (RFC-0007).
- `jobs`: persistente asynchrone Warteschlange (Segment-Historie, Auswertung neuer Aktivitäten, Neuberechnung).
- `import_jobs`: Importvorgänge und ihr Status.
- `data_state`: Datenstandsidentität für Sicherung und Restore.

Vollständiges DDL-Schema siehe [RFC-0001](../rfc/RFC-0001-data-schema.md).
