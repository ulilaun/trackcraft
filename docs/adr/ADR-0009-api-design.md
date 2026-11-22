# ADR-0009: API-Design

Status: Akzeptiert, konsolidiert am 2026-10-28.

REST unter `/api/v1`, JSON für strukturierte Fachdaten. Der exakte Vertrag ist [docs/api/openapi.yaml](../api/openapi.yaml) (OpenAPI 3.1, Spec-first), Konventionen und Fehlercodes stehen in [RFC-0003](../rfc/RFC-0003-rest-api-specification.md). Datei-Uploads laufen als `multipart/form-data` mit einem Datei-Part, alles andere als JSON.

Antwortprinzip: `data` für das Ergebnis, `meta` nur bei Listen (Offset-Paging mit `limit`/`offset`/`total`), bei Berechnungsstand und bei Hinweisen. Teiländerungen nur mit PATCH, kein PUT in V1.

Fehlerform: `{"error":{"code":"segment_not_found","message":"Segment not found"}}`.

Sicherung/Restore und Import sind explizite Operationen, keine versteckten Dateikopierfunktionen. Vorschau/Bestätigung müssen den geprüften Stand binden. Kein Export oder Dateidownload in V1; reguläre Trackdaten zur UI bleiben zulässig. Kein App-Login.

Die verbindliche Endpunktübersicht ist der Vertrag, die Gruppenübersicht steht in RFC-0003. Sie ersetzt alte unversionierte Kurzlisten im Projektplan und ADR-0001.
