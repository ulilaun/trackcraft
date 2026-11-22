# ADR-0006: Segmentmatching

Status: Bestätigte Fachregeln, Piggy 009; überarbeitet im Konsolidierungsreview (zweistufiges Matching, alle Parameter aus der TOML).

geo/geo-types/rstar, räumliche Kandidatensuche.

## Ablauf und Parameter

1. **Zweistufiges Matching**: Stufe 1 ist eine großzügige Kandidatensuche (Puffer um Start- und Zielkontrollpunkt plus Richtungsprüfung über eine geglättete Sehne aus 3 GPS-Punkten). Stufe 2 ist die maßgebliche Endprüfung gegen die vollständige Referenzgeometrie des Segments (Toleranz, Abdeckung, Start-/Zielquerlinie). Ein Kandidat ist noch kein Treffer.
2. **Parameter aus der TOML**: Alle Matching-Werte stehen in `trackcraft.toml` unter `[segments.matching]`, nicht in der Datenbank und nicht pro Segment. Vorgaben: Radius 15 m (zum Trackverlauf), Winkel 45°, Versatzausgleich bis 15 m, Toleranz 13 m, Abdeckung 95 %, Richtungsbezug 5 m, Rückschritt 5 m, innere Lücke 30 m, Interpolation und Abdeckungsverbindung je 15 s. Versatz, Toleranz, innere Lücke und Zeitabstände sind an echten Dateien geprüft (RFC-0014). Änderungen lösen eine versionierte Neuberechnung aus.
3. **Outlier-Filter**: Vor dem Matching durchläuft die Aktivität den GPS-Outlier-Filter mit sportspezifischen Höchstgeschwindigkeiten aus `trackcraft.toml` (`[processing.outlier_filter]`). Eine Session mit Fahrzeugabschnitt wird gar nicht erst importiert (`[processing.vehicle_detection]`, RFC-0008).
4. **Asynchrone Auswertung**: Bei Erstellung eines Segments (gegen die Historie), nach dem Import einer Aktivität und bei Änderung der Rechenparameter werden persistente Jobs in die Tabelle `jobs` eingereiht. Änderungen von Name oder Beschreibung eines Segments lösen kein Matching aus.

Detailregeln siehe [RFC-0002](../rfc/RFC-0002-segment-matching.md) und [RFC-0008](../rfc/RFC-0008-activity-import-and-processing-pipeline.md).
