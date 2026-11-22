# ADR-0015: Segmentranglisten

Status: Bestätigte Regeln, Piggy 004.

Eigene Versuche je Benutzer und Sportklasse cycling/ebike/running/skiing (skiing seit Schemaversion 6; swimming und studio ohne Bestenlisten); unknown bis Zuordnung ausgeschlossen. Untertypen keine zusätzliche Bestenlistentrennung.

Zeit zwischen gültigem Start/Ziel inklusive Pausen/Autopause. Start/Ziel auf nächste ms runden, halbe ms aufrunden; Differenz ranken. Zehntelanzeige gleiches Rundungsprinzip, keine Messgenauigkeitszusage.

Sortierung Dauer, tatsächlicher Startzeitpunkt, unveränderliche Aktivitäts-ID, stabile Versuchsposition. Nicht von neu erzeugten Ergebnis-IDs abhängig. Gleich schnelle Leistung Rekordeinstellung.

Historische PRs chronologisch nach Leistungszeit, Nachimport/Korrektur/Löschung neu berechnen. Erste Bestzeit Basis ohne Verbesserungszählung/Zeitersparnis, nur strikt kürzere Dauer neue Verbesserung.

Monats-/Jahresbestzeit nach lokalem Segmentstart in aktueller Konfigurationszone, initial Europe/Berlin. Aktivitätsstart dafür nicht maßgeblich.

Vollständige Referenz/Abdeckung/Richtung/Zeitqualität in [RFC-0002](../rfc/RFC-0002-segment-matching.md), Rekord-/Statistikdetail in [RFC-0009](../rfc/RFC-0009-ranking-statistics-and-personal-records.md). Lösch-/Ungültigkeitsregeln vorrangig, versionierte gemeinsame Veröffentlichung.
