# TrackCraft: Spezifikation

Überblick über Funktionen, Installation und Entwicklung: [README im Hauptordner](../README.md).

Die Fachdokumente (ADRs, RFCs, Schnittstellenvertrag) beschreiben, wie TrackCraft arbeitet, und werden mit dem Code fortgeschrieben. Offene Punkte und Ideen stehen als [GitHub-Issues](https://github.com/ulilaun/trackcraft/issues).

Das Produkt heißt **TrackCraft** (RFC-0006). „Piggy“ war der Arbeitstitel, er steht nur noch in älteren Versionsangaben wie „Piggy 012“.

## Einstieg

1. [V1-Umfang](rfc/RFC-0100-mvp-scope-definition.md): Pflichtumfang v1.0 und Post-MVP-Schnitt (v1.1).
2. [Frontend & UI-Design](rfc/RFC-0006-frontend-architecture-and-pwa.md): UI-Design-System, Komponenten, Seitenstruktur.

## Verbindlichkeit

Ein ADR hält die Entscheidung, der zugehörige RFC das verbindliche Detail. Bei einem Widerspruch gilt der jüngere Stand, der Widerspruch ist zu beheben. 15 ADRs und 23 RFCs, dazu der Schnittstellenvertrag [openapi.yaml](api/openapi.yaml).

## Architekturentscheidungen (ADR)

- [ADR-0001: Gesamtarchitektur](adr/ADR-0001-architecture.md)
- [ADR-0002: Backendframework](adr/ADR-0002-backend-framework.md)
- [ADR-0003: Datenbank](adr/ADR-0003-database.md)
- [ADR-0004: Frontend](adr/ADR-0004-frontend.md)
- [ADR-0005: Segmentmodell](adr/ADR-0005-segment-model.md)
- [ADR-0006: Segmentmatching](adr/ADR-0006-segment-matching.md)
- [ADR-0007: Deployment](adr/ADR-0007-deployment.md)
- [ADR-0008: Sicherheit](adr/ADR-0008-security.md)
- [ADR-0009: API-Design](adr/ADR-0009-api-design.md)
- [ADR-0010: Datenmodell](adr/ADR-0010-data-model.md)
- [ADR-0011: Speicherlayout](adr/ADR-0011-storage-layout.md)
- [ADR-0012: Authentifizierung und Zugriff](adr/ADR-0012-authentication-and-access.md)
- [ADR-0013: Observability und Logging](adr/ADR-0013-observability-and-logging.md)
- [ADR-0014: Backup und Wiederherstellung](adr/ADR-0014-backup-strategy.md)
- [ADR-0015: Segmentranglisten](adr/ADR-0015-segment-ranking-rules.md)

## Schnittstellenvertrag

- [OpenAPI 3.1: docs/api/openapi.yaml](api/openapi.yaml): Pfade, Parameter und DTOs (Spec-first, verbindlich).

## Spezifikationen (RFC)

- [RFC-0001: Datenmodell und Benutzer](rfc/RFC-0001-data-schema.md)
- [RFC-0002: Segmentmatching](rfc/RFC-0002-segment-matching.md)
- [RFC-0003: REST-Schnittstelle (Konventionen, Fehlercodes, Abläufe)](rfc/RFC-0003-rest-api-specification.md)
- [RFC-0004: Repository und Entwicklungsstruktur](rfc/RFC-0004-repository-and-monorepo-structure.md)
- [RFC-0005: Deployment und Infrastruktur](rfc/RFC-0005-deployment-and-infrastructure.md)
- [RFC-0006: Frontend-Architektur, UI-Design und PWA](rfc/RFC-0006-frontend-architecture-and-pwa.md)
- [RFC-0007: Backendarchitektur und Anwendungsschichten](rfc/RFC-0007-backend-architecture-and-application-layers.md)
- [RFC-0008: Aktivitätsimport, Outlier-Filter und Pipeline](rfc/RFC-0008-activity-import-and-processing-pipeline.md)
- [RFC-0009: Ranglisten, Statistiken und Rekorde](rfc/RFC-0009-ranking-statistics-and-personal-records.md)
- [RFC-0010: Equipment, Komponenten und Wartung](rfc/RFC-0010-equipment-and-maintenance-management.md)
- [RFC-0011: Sicherheit und Zugriff](rfc/RFC-0011-security-and-authentication.md)
- [RFC-0012: Sicherung und Wiederherstellung](rfc/RFC-0012-backup-and-recovery-strategy.md)
- [RFC-0013: Logging und Monitoring](rfc/RFC-0013-logging-and-monitoring.md)
- [RFC-0014: Tests und Qualitätssicherung](rfc/RFC-0014-testing-strategy-and-quality-assurance.md)
- [RFC-0015: Releases und Versionierung](rfc/RFC-0015-release-and-versioning-strategy.md)
- [RFC-0016: Segmenterstellung und Bedienung](rfc/RFC-0016-segment-user-experience.md)
- [RFC-0017: Aktivitätenverwaltung](rfc/RFC-0017-activity-management.md)
- [RFC-0018: Dashboard und Startseite](rfc/RFC-0018-dashboard-and-home-screen.md)
- [RFC-0019: Export und Portabilität](rfc/RFC-0019-data-export-and-portability.md)
- [RFC-0020: Karten und Visualisierung](rfc/RFC-0020-maps-and-visualization.md)
- [RFC-0021: Zentrale Anwendungskonfiguration](rfc/RFC-0021-configuration.md)
- [RFC-0022: Binärformat der Messpunkte](rfc/RFC-0022-track-point-binary-format.md)
- [RFC-0100: V1-Umfang (MVP Scope Definition)](rfc/RFC-0100-mvp-scope-definition.md)
