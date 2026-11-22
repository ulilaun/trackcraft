# RFC-0004: Repository und Entwicklungsstruktur

Status: Konsolidierter Entwurf, 2026-10-29 (Piggy 007).

TrackCraft ist ein Monorepo für FIT-/GPX-Analyse:

```text
trackcraft/
  backend/              # Rust, migrations, src, tests, config
  frontend/             # React, src, public, tests
  infrastructure/       # docker/ (Dockerfiles, Nginx), windows/ (Dienst, Einrichtung)
  scripts/
  docs/                 # RFC, API, Architektur
  data/                 # Laufzeitdaten, keine zugesagte Git-Ablage
  .github/workflows/
  docker-compose.yml
  README.md
  LICENSE               # AGPL-3.0-or-later
```

**Lizenz**: TrackCraft steht unter der GNU Affero General Public License v3.0 oder später (`AGPL-3.0-or-later`), Text in `LICENSE`. Abhängigkeiten müssen damit verträglich sein (MIT, BSD, Apache-2.0, ISC, MPL-2.0, LGPL, GPL-3.0); eine Abhängigkeit mit anderer Lizenz braucht vorher eine Prüfung. Stand der Spezifikation: React, Vite, TanStack Query, React Router, Tailwind, Lucide, Recharts (MIT), Leaflet (BSD-2-Clause), Axum, Tokio (MIT), rustyfit (BSD-3-Clause), SQLite (gemeinfrei). Beispieldateien des Garmin FIT SDK im öffentlichen Testkorpus bleiben unter ihrer eigenen Lizenz und werden nicht Teil des ausgelieferten Programms (RFC-0014). Kartenkacheln und -daten von OpenStreetMap unterliegen der ODbL und verlangen die Namensnennung (RFC-0020).

Dies ist die geplante Implementierungsstruktur. Die Spezifikation liegt bereits im Repository: ADRs unter `docs/adr`, RFCs unter `docs/rfc`, der Vertrag unter `docs/api`. Navigations- und Nachweisdokumente (README, Projektplan, offene Punkte, Konsolidierung, Prüfprotokoll) liegen im Stammordner.

Backendmodule: api, config, db, domain, geo, import, jobs, repositories, segment, services; app.rs/main.rs. Domain umfasst auch den festen Benutzer. Frontend: app, pages, components, features, api, hooks, store, types, utils. Struktur darf keine zweite Fachlogik im API-Layer erzeugen.

Migrationen unter backend/migrations, konkrete Reihenfolge nach finalem Schema. Illustrative alte Namen `0001_initial`, `0002_equipment`, `0003_segments` waren kein freigegebener Migrationsstand.

Zwei Plattformen (RFC-0005): Windows mit `trackcraft.exe` als Dienst, Frontend-Build daneben im Ordner `web`; Docker mit frontend.Dockerfile/backend.Dockerfile, zwei Appcontainer. Das Backend ist ein Binary für beide; Windows-spezifischer Code (Dienststeuerung, Ereignisprotokoll) liegt in einem eigenen Modul `platform` hinter `cfg(windows)`, sonst gibt es keine Plattformzweige in der Fachlogik. Konfigurierbare initiale Datenbereiche database, activities, backups, logs.

## Konfiguration

Nur TOML, Datei standardmäßig `%ProgramData%\TrackCraft\config\trackcraft.toml` (Windows) bzw. `/config/trackcraft.toml` (Container, read-only mount), nur `TRACKCRAFT_CONFIG` zur Pfadauswahl. Vollständige Beispieldatei nach finalem Schema noch zu erstellen. Keine `.env`-Einzelwerte als zweite Anwendungsquelle.

## Entwicklung und CI

Rust Toolchain/Cargo (unter Windows MSVC-Ziel mit Visual Studio Build Tools), Node LTS, Git, Editor; Docker nur für die Docker-Plattform. Entwickelt wird auf Windows. rust-analyzer/TOML/ESLint/Prettier empfohlen. Abhängigkeiten müssen unter Windows ohne zusätzliche Systembibliotheken bauen (z. B. rustls statt OpenSSL, SQLite gebündelt).

Prüfungen: cargo fmt, clippy, test, Integration/Coverage auf Windows und Linux; Frontend lint, Vitest/Testing Library, build; Docker-Images bauen. Workflows unter `.github/workflows`: `ci` für jeden Pull Request und jeden Push auf `main` mit allen Prüfungen einschließlich Backend (Windows) und Docker-Images (ein neuer Push auf denselben PR bricht den alten Lauf ab), `release` für Tags `v*` (RFC-0015), `linux-package` für das Linux-Paket (vom Release aufgerufen, bei Pull Requests als Testpaket und von Hand). Die Betriebsanleitung `BETRIEB.md` je Plattform liegt unter `infrastructure/windows` und `infrastructure/docker` und wird ins jeweilige Release-ZIP gepackt. `THIRD-PARTY-LICENSES.txt` erzeugt der Build aus den Abhängigkeiten (Rust und npm). Exakte CI-Skripte sind noch keine vorliegenden Implementierungsdateien.

Branchmodell: `main` + `feature/*`, Releases als Tags auf `main`. Kein `develop`-Branch.
