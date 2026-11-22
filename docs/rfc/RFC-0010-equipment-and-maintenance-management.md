# RFC-0010: Equipment, Komponenten und Wartung

Status: Post-MVP (v1.1+), aus V1.0 gestrichen (RFC-0100), Piggy 011. Wiederhergestellt aus den bis Piggy 010 verstreuten Regeln in RFC-0003, RFC-0009, RFC-0014 (Nr. 23–29) und RFC-0017. Nicht Teil des V1.0-Vertrags.

## 1. Abgrenzung

V1.0 enthält kein Equipment: keine Tabellen, Endpunkte, UI, Wartungswarnungen, Zuordnung an Aktivitäten und keine Equipmentdaten in der Sicherung. Dieses Dokument ist die Grundlage für Release v1.1 und ersetzt die verstreuten Einzelregeln.

Equipment hat den festen Benutzer als Besitzer. Kein Besitzerwechsel.

## 2. Equipment und Komponenten

- Equipment kann maximal eine Komponentenebene haben. Keine Zyklen, höchstens ein zeitgleicher Parent.
- Parent, Einbau, Ausbau, Wechsel, Deaktivierung und Reaktivierung sind zeitlich. Zeitpunkte sind höchstens jetzt und rückdatierbar, nie in der Zukunft. Grenze: Aktivitätsstart ≥ Zeitpunkt.
- Rückdatierte Konflikte erscheinen als bestätigungspflichtige Vorschau (Rückfrage).
- Deaktivierung und Reaktivierung wirken kaskadierend auf Parent und Komponenten. Eine eingebaute Komponente ist nur bei aktivem Parent reaktivierbar.
- Der Typ ist Freitext, es gibt keine typabhängige Hierarchie. Mehrere aktive Ketten zählen parallel. Nachimporte wirken historisch.

## 3. Zuordnung und Nutzung

- Eine Aktivität hat 0..n direkte Equipments, Import ohne Zuordnung ist erlaubt. Neu wählbar sind nur aktive Equipments, bestehende Zuordnungen bleiben erhalten. Ein zum Start eingebautes Unterteil ist nicht direkt wählbar.
- Automatische Komponenten ergeben sich aus Aktivitätsstart und Einbauhistorie.
- Volle Nutzung je beteiligtem Equipment. Dasselbe Equipment zählt pro Aktivität einmal, die Gesamtstatistik der Aktivität ebenfalls nur einmal.
- Die ganze Aktivität zählt nach ihrem Start, ohne Aufteilung. Eine Änderung während der Aktivität wirkt nicht auf sie. Korrektur, Löschung und Nachimport lösen Neuberechnung aus.
- Beim Ersetzen einer Aktivität (Import) sind direkte Equipments, auch inzwischen inaktive (ohne Reaktivierung), als Metadaten übernehmbar. Die automatischen Komponenten folgen dem neuen Start.
- Einbau- und Aktivzeiträume bleiben erhalten.

## 4. Wartungsregeln

- Eine Regel hat eine eigene Identität (nicht nur eine Freitextart), einen frei vergebenen Namen, eine feste Einheit `km` oder `hours`, einen änderbaren Wert (Änderung setzt nichts zurück) und einen Status aktiv/inaktiv. Eine andere Einheit erfordert eine neue Regel.
- Der Ausgangswert (Lebenszeit-Basis) wird nur beim Anlegen gesetzt. Für ein neues Equipment ist er 0 (Vorgabe), bei bekannter anderer Basis änderbar. Der Bezugszeitpunkt ist vorbelegt mit jetzt.
- Deaktivierung ist endgültig: nicht reaktivierbar, Historie bleibt, keine Warnung mehr. Eine Regel wird mit ihrem Equipment deaktiviert. Parallele Regeln sind unabhängig.
- Stundenregeln nutzen die Gesamtzeit, auch für Aktivitäten ohne GPS.

## 5. Wartungslogbuch

- Ein Logbuch je Equipment. Ein Eintrag hat optional einen nach dem Anlegen festen Verweis auf höchstens eine Regel, die er zurücksetzt. Ohne Verweis erfolgt kein Reset. Kein Reset von Parent oder Geschwistern.
- Der km/h-Stand wird gespeichert, ist vorbelegt und änderbar. Kein Kostenfeld.

## 6. Berechnung

- Maßgeblich ist der jüngste Bezug. Bei gleichem Zeitpunkt gilt die Wartung zuerst. Eine Aktivität mit Start gleich Bezug zählt danach, eine Aktivität vor dem Bezug davor.
- Fehlende Nutzung in der Wartung zählt als 0, ohne Unsicherheitsstatus allein wegen Messlücken. Ampel mit den Schwellen 90 % und 100 %, ungerundet. Warnung „Wartung fällig“ (RFC-0013). Deaktivierte Regeln lösen keine Fälligkeitswarnung aus.

## 7. Löschen

Ungenutztes Equipment ist löschbar samt Unterteilen, Regeln, Basen und Wartungen. Genutztes Equipment (direkt oder automatisch) wird abgewiesen. Ein genutztes Unterteil an einem ungenutzten Parent bleibt eigenständig.

## 8. API (noch kein Vertrag)

```http
GET    /api/v1/equipment
POST   /api/v1/equipment
PUT    /api/v1/equipment/{id}
PATCH  /api/v1/equipment/{id}
DELETE /api/v1/equipment/{id}
GET    /api/v1/equipment/{id}/maintenance
POST   /api/v1/equipment/{id}/maintenance
PATCH  /api/v1/equipment/{id}/maintenance/{maintenance_id}
DELETE /api/v1/equipment/{id}/maintenance/{maintenance_id}
GET    /api/v1/equipment/{id}/stats
```

## 9. Abnahmefälle (ehemals RFC-0014 Nr. 23–29)

23. Equipment 0..n volle Nutzung, gleiche Beteiligung einmal, Gesamtaktivität einmal. Aktiv/inaktiv/neue direkte Auswahl, bestehende Historie.
24. Eine Komponentenebene/keine Zyklen/höchstens ein zeitgleicher Parent. Einbau/Ausbau/Wechsel/Reaktivierung rückdatierbar, nie Zukunft, Grenze Start ≥ Zeitpunkt, rückdatierte Konflikte per Rückfrage, Deaktivierungs-/Reaktivierungskaskade Parent/Komponenten, eingebaute Komponente nur bei aktivem Parent reaktivierbar, Nachimporte historisch; mehrere aktive Ketten zählen parallel.
25. Komponenten volle Nutzung nach Aktivitätsstart, kein Split bei Änderung unterwegs. Stundenregeln nach Gesamtzeit, auch für Aktivitäten ohne GPS.
26. Regeln frei benannt, feste Einheit/änderbarer Wert ohne Reset, anderer Einheit neue Regel. Deaktivierung endgültig/nicht reaktivierbar, Historie/keine Warnung, mit Equipment deaktiviert; parallele Regeln unabhängig; Vorgabe Bezugszeitpunkt jetzt.
27. Wartungslogbuch: Eintrag mit festem Verweis auf höchstens eine Regel setzt nur diese zurück, ohne Verweis kein Reset, km/h-Stand vorbelegt/gespeichert/änderbar, kein Kostenfeld; kein Parent-/Geschwisterreset. Lebenszeit-Ausgangswert nur beim Anlegen. Typ Freitext, keine typabhängige Hierarchie, eingebautes Unterteil nicht direkt zuordenbar. Neu=0 Default, bekannte andere Basis; jüngster Bezug, Zeitgleichheit Wartung.
28. Ganze Aktivität nach Start: vor Bezug davor, gleich/danach danach. Kein Split, Korrektur/Löschung/Nachimport neu berechnen. Fehlende Wartungsnutzung 0, normale 90-/100-%-Ampel ungerundet.
29. Ungenutztes Equipment samt Unterteilen/Regeln/Basen/Wartungen löschbar; direkt oder automatisch genutztes geschützt; genutztes Unterteil an ungenutztem Parent bleibt eigenständig.

## 10. Offene Punkte

1. Operationen für Regeln, Basen, Hierarchie und Aktivhistorie sowie konkrete Fehlermeldungen.
2. DTOs und Datenbankschema für v1.1.
3. Bedeutung und Darstellung der Ampel-Stufen 90 % und 100 %.
