# RFC-0015: Releases und Versionierung

Status: Konsolidierter Entwurf, 2026-10-29 (Piggy 007).

SemVer MAJOR.MINOR.PATCH, Tags v1.0.0 usw., Stable/Beta. Branchmodell `main` + `feature/*`, Releases als Tags auf `main`.

## Build und Verteilung

Festgelegt am 08.11.2026.

- **Nur CI baut Releases.** Ein Tag `vX.Y.Z` auf `main` startet den Release-Workflow in GitHub Actions; ein Tag `vX.Y.Z-beta.N` erzeugt eine Vorabversion (Beta). Von Hand gebaute Programme und Images sind keine Releases.
- **Ablauf**: alle Prüfungen aus RFC-0004 auf Windows und Linux, Frontend einmal bauen, daraus das Windows-ZIP, das Linux-Paket und das Frontend-Image; Backend für `x86_64-pc-windows-msvc`, `x86_64-unknown-linux-gnu` und `linux/amd64`. Scheitert ein Schritt, entsteht kein Release.
- **Artefakte** im GitHub Release des Tags:
  - `trackcraft-<version>-windows-x86_64.zip` (Inhalt RFC-0005),
  - `trackcraft-<version>-linux-x86_64.tar.gz` (Inhalt RFC-0005 „Linux“),
  - `trackcraft-<version>-docker.zip` mit `compose.yaml` (Images mit genauer Version), `nginx.conf`, Beispiel-TOML für Container und `BETRIEB.md`,
  - `SHA256SUMS.txt` mit den Prüfsummen aller Pakete,
  - Release Notes: Änderungen, neue oder geänderte TOML-Schlüssel, Schemaversion, ob ein Update über `setup.cmd` Besonderheiten hat, bekannte Einschränkungen.
- **Images** nach `ghcr.io/ulilaun/trackcraft-frontend` und `-backend` mit genauer Version, Stable zusätzlich `stable` (RFC-0005).
- **Version im Programm**: Version, Commit und Builddatum werden beim Build eingebettet und über `GET /version`, `trackcraft.exe service status` und die Systemseite angezeigt. Der Quellcode-Link der Fußzeile (AGPL § 13) zeigt auf den Release-Tag. Solange das Repository privat ist, ist er nur für Ulrich erreichbar; das genügt, weil nur er die Anwendung nutzt.
- **Keine Codesignatur** in V1. Echtheit prüft der Download aus dem eigenen Repository und `SHA256SUMS.txt`.
- **Kein Update-Check**: TrackCraft fragt keine Releases ab und lädt nichts nach. Einzige externe Verbindung bleibt die Karte (ADR-0008).

## Migration und Kompatibilität

Migrationen beim Start prüfen/ausführen. Vor jeder Migration lokale Vollsicherung, Kopie ins Sicherungsziel falls erreichbar (RFC-0012); scheitert die lokale Sicherung, bricht der Start ab. Startreihenfolge (C5, 08.11.2026), jeder Fehler beendet den Start mit Meldung:

1. TOML lesen und vollständig prüfen (RFC-0021).
2. Sperrdatei `trackcraft.lock` in `database_dir` exklusiv belegen, sonst Abbruch „läuft bereits“.
3. Datenordner und Migrationsordner prüfen (vorhanden, beschreibbar), Staging aufräumen (ADR-0011).
4. Datenbank öffnen und `user_version` lesen. Neuer als die eigene Schemaversion: Abbruch ohne Änderung.
5. Steht eine Migration an und enthält die Datenbank schon Daten: Sicherung vor Migration lokal in `migration_dir`, danach Kopie ins Ziel, falls erreichbar (RFC-0012). Eine neu angelegte, leere Datenbank braucht keine.
6. Migrationen nacheinander, jede in einer Transaktion.
7. Benutzer und `data_state` beim ersten Start anlegen (Spitzname aus `[user] display_name`, sonst „Ich“); danach führt die Datenbank.
8. Erst jetzt den Port öffnen. Vorher beantwortet TrackCraft keinen Request und startet keinen Job.

Upgradebeispiele 1.0->1.1 und 1.1->1.2 sind noch kein nachgewiesener Kompatibilitätsvertrag. Unterstützte Schema-/Backup-/Configversionen je Release dokumentieren.

Schema-, fachliche Datenstands- und Berechnungsversion strikt trennen. Neue Rechenparameter lösen versionierte Hintergrundberechnung aus, nicht automatisch eine fachliche Änderung an Originalen.

## Rollback

Kompatibles vorheriges Programm bzw. Image und zugehörigen gültigen Sicherungsstand nach dem sicheren Restoreverfahren aus RFC-0012 verwenden. Keine pauschale Zusage, dass ein altes Image gegen ein bereits migriertes Schema laufen kann. Ein Programm, das eine neuere Schemaversion als die eigene vorfindet, startet nicht und nennt beide Versionen; es ändert dabei nichts an den Daten.

Ablauf unter Windows: Dienst stoppen, Programmordner aus dem aufbewahrten alten ZIP ersetzen. War schon migriert, die Datenordner leeren bzw. beiseitelegen, Dienst starten und die Sicherung vor der Migration (lokal im Migrationsordner oder im Ziel) über die Seite Sicherung wiederherstellen. Unter Docker entsprechend mit der alten Imageversion. Restore ersetzt vollständig und braucht bei älterem/nicht vergleichbarem Stand Warnung/Bestätigung.

## Checkliste

Tests/Lint/Build/Coverage, Migrationen, Backup/Restore, Dokumentation/RFC/Release Notes, bekannte Einschränkungen und Konfigurationsschema. Version über `/api/v1/version`, DTO noch offen.

Updatebenachrichtigung, automatischer Check, Selbstaktualisierung, MSI-Installer, Codesignatur und Rollbackassistent nur spätere Optionen; externe Abfragen benötigen gesonderte Einordnung.
