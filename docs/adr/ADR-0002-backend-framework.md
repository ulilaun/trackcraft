# ADR-0002: Backendframework

Status: Akzeptiert, konsolidiert am 2026-10-29 (Piggy 006).

Rust, Axum, Tokio, Serde und SQLx. Ziele: testbare Async-Architektur, Wartbarkeit, gute Ressourcennutzung. Rust-Lernkurve als Aufwand berücksichtigen.

Diese Technologieauswahl ist kein Performancebeleg. Schichten, Jobs, persistente Operationskoordination und zentrale Konfiguration nach RFC-0007.

## FIT-Parser

Entschieden am 01.11.2026: [rustyfit](https://github.com/muktihari/rustyfit) 0.11 (BSD-3-Clause), Nutzung nach RFC-0008.

| Kriterium | rustyfit 0.11 | fitparser 0.11 (Rückfalloption) |
| :--- | :--- | :--- |
| Mehrere Sessions, verkettete Dateien | ja, `decode` je Teildatei | ja |
| Komprimierte Zeitstempel, Big-Endian | ja | ja |
| Developer-Felder ohne `field_description` | übersprungen | brechen die ganze Datei ab |
| Werte | Rohwerte als Ganzzahlen (Semicircles, cm, ms) | skaliert als `f64`, Zeitpunkte in Ortszeit |
| Lizenz | BSD-3-Clause | MIT |
| Pflegezustand | aktiv, Release etwa monatlich, FIT-Profil 21.218 | gepflegt, Release etwa jährlich, FIT-Profil 21.202 |
| 200 Garmin-Edge-Dateien | 3,4 s | 22,6 s |

Beide Parser lieferten für alle 200 Dateien und ihre Big-Endian-, komprimierten und verketteten Varianten dieselben Sessions und Records. Entscheidend sind die Rohwerte, die der Zuordnung in RFC-0008 ohne Rundung entsprechen, die Toleranz gegenüber Developer-Feldern und die Geschwindigkeit auf dem NAS.

Risiko: rustyfit ist jung (seit 2025) und hat einen einzelnen Maintainer. Der Parser wird deshalb nur im Importmodul über eine eigene Schnittstelle (Sessions und Messpunkte nach RFC-0008) verwendet, ein Wechsel zu fitparser betrifft nur diesen Adapter. Die Version wird fest im `Cargo.lock` geführt, ein Update läuft gegen den Testkorpus (RFC-0014). Für jede FIT-Implementierung gilt zusätzlich die FIT Protocol License von Garmin.
