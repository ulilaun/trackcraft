<p align="center"><img src="docs/assets/trackcraft-icon.svg" width="96" alt="TrackCraft"></p>

# TrackCraft

<p align="center"><b>Eigene Segmente und Bestenlisten wie bei Strava – rückwirkend über alle Aktivitäten, mit Garmin-Import, ohne Cloud und ohne Abo.</b></p>

<p align="center"><a href="README.en.md">English</a> · <a href="#ausprobieren">In zwei Minuten ausprobieren</a> · <a href="#installation">Installation</a> · <a href="#häufige-fragen">Häufige Fragen</a></p>

**Deine Trainingsdaten auf deinem eigenen Rechner.** TrackCraft ist eine selbst gehostete Trainingsplattform für FIT- und GPX-Dateien: Aktivitäten importieren, auf der Karte ansehen, eigene Segmente und Strecken anlegen, Bestzeiten über Jahre und Saisons verfolgen, Belastung und Form beobachten und das Training bis zum nächsten Ziel planen. Ohne Cloud, ohne Konto, im eigenen Heimnetz.

Stand: Version 0.9 (Vorabversion auf dem Weg zu v1.0). Die Oberfläche gibt es auf Deutsch und Englisch (Einstellungen → Darstellung, Datum und Zahlen getrennt wählbar), die Betriebsanleitungen auf Deutsch und Englisch, die Spezifikation auf Deutsch. TrackCraft ist für **eine Person** im eigenen Heimnetz gedacht und hat bewusst **kein Login** (siehe [Sicherheit](#sicherheit-nur-fürs-heimnetz)).

**Ausprobieren ohne Installation:** Die [Demo](#ausprobieren) mit erfundenen Mallorca-Daten läuft unter Windows per Doppelklick oder mit einem `docker compose`-Befehl.

<p align="center"><img src="docs/screenshots/segment-erstellen.gif" width="720" alt="Animation: Auf einer Fahrt wird ein Anstieg auf Karte und Höhenprofil markiert und als Segment angelegt. TrackCraft wertet alle Aktivitäten aus und zeigt die Bestzeiten-Entwicklung."></p>
<p align="center"><sub>Segment markieren, anlegen, fertig: TrackCraft findet es sofort in allen bisherigen Fahrten.</sub></p>

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/screenshots/aktivitaet-dunkel.png">
  <img src="docs/screenshots/aktivitaet-hell.png" alt="Aktivitätsdetail mit Kennzahlen, Karte und Höhen- und Pulsprofil">
</picture>

| Segment mit Bestenliste | Statistik |
| :---: | :---: |
| <picture><source media="(prefers-color-scheme: dark)" srcset="docs/screenshots/segment-dunkel.png"><img src="docs/screenshots/segment-hell.png" alt="Segment Sa Calobra mit Karte, Höhenprofil und Bestzeiten"></picture> | <picture><source media="(prefers-color-scheme: dark)" srcset="docs/screenshots/statistik-dunkel.png"><img src="docs/screenshots/statistik-hell.png" alt="Statistik mit Saisons, Monatsdistanz und Verteilung je Sportklasse"></picture> |
| **Dashboard** | **Aktivitätenliste** |
| <picture><source media="(prefers-color-scheme: dark)" srcset="docs/screenshots/dashboard-dunkel.png"><img src="docs/screenshots/dashboard-hell.png" alt="Dashboard mit letzten Aktivitäten, Woche, Monat und Jahr"></picture> | <picture><source media="(prefers-color-scheme: dark)" srcset="docs/screenshots/aktivitaeten-dunkel.png"><img src="docs/screenshots/aktivitaeten-hell.png" alt="Aktivitätenliste mit Filtern und Summen"></picture> |
| **Form** | **Plan** |
| <picture><source media="(prefers-color-scheme: dark)" srcset="docs/screenshots/form-dunkel.png"><img src="docs/screenshots/form-hell.png" alt="Formkurve mit Fitness, Ermüdung und Form samt Vorschau"></picture> | <picture><source media="(prefers-color-scheme: dark)" srcset="docs/screenshots/plan-dunkel.png"><img src="docs/screenshots/plan-hell.png" alt="Trainingsplan im Kalender mit erledigten und geplanten Einheiten und einem Ziel"></picture> |

Alle Bilder zeigen erfundene Beispieldaten auf Mallorca ([scripts/demo](scripts/demo/README.md)); eine Tablet-Ansicht liegt unter [docs/screenshots](docs/screenshots).

## Was TrackCraft heute kann

**Import**
- FIT- und GPX-Dateien per Drag-and-drop, viele auf einmal; Dateien ohne Konflikt werden ohne Rückfrage gespeichert.
- Erkennt Duplikate, korrigiert falsche Gerätezeiten und merkt sich abgelehnte Dateien.
- Filtert GPS-Ausreißer, glättet die Höhe und erkennt Fahrten im Auto oder Zug.
- Liest auch FIT-Dateien mit falscher Prüfsumme, solange sie lesbar sind.
- **Garmin Connect:** holt mit der persönlichen Garmin-API alle Aktivitäten ab einem Startdatum und danach regelmäßig die neuen (mit Fortschrittsanzeige).

**Aktivitäten**
- Liste mit Filtern (Sportklasse, Zeitraum, Saison, Strecke), Sortierung und Summenzeile.
- Detailseite mit Kennzahlen, Karte (OpenStreetMap) und Höhen-, Puls- und Geschwindigkeitsprofil, beide mit gemeinsamer Markierung.
- Titel, Sportart, Ort, Beschreibung und Zeit bearbeiten; ein leerer Ort wird aus deinen anderen Aktivitäten oder per OpenStreetMap vorgeschlagen. Löschen mit Vorschau, was dabei wegfällt.
- Sportklassen Radfahren, E-Bike, Laufen, Skifahren, Schwimmen und Studio, Untertypen (z. B. Rennrad, MTB, Trail) in der Oberfläche pflegbar.

**Segmente und Strecken**
- Abschnitt direkt auf der Karte einer Aktivität markieren und als Segment oder ganze Strecke speichern, auch auf dem Tablet per Touch.
- TrackCraft findet jedes Segment in allen bisherigen und künftigen Aktivitäten wieder, auch bei GPS-Versatz und Lücken.
- Bestenlisten gesamt, je Jahr und je Saison, mit Plakette für die Saisonbestzeit und Verlaufsdiagramm.
- Erkennt Rundkurse und warnt, wenn eine neue Strecke einer vorhandenen gleicht.

**Auswertung**
- Dashboard mit den letzten Aktivitäten samt Kartenvorschau, der laufenden Woche (erledigt gegen geplant), Monat und Jahr.
- Statistikseite mit Summen und Verteilung je Sportklasse über frei wählbare Zeiträume.
- **Saisons:** frei benannte Zeiträume (z. B. „Sommer 2026“), in der Oberfläche gepflegt, wirken auf Bestenlisten, Filter und Statistik.

**Belastung und Form**
- Herzfrequenzzonen im Konto, allgemein oder je Sportklasse und mit Gültigkeitsdatum; die maximale HF wird aus dem Geburtstag vorgeschlagen.
- Belastung je Aktivität aus der Herzfrequenz (TRIMP nach Banister) und Zeit in den Zonen; ohne Pulsdaten geschätzt und so gekennzeichnet.
- Seite **Form** mit Fitness, Ermüdung und Form über die Zeit und einer Vorschau (Ruhe, gleichbleibend oder nach Plan); Modell wählbar, jede Einstellung mit Erklärung.

**Planung**
- Kalender mit geplanten Einheiten (Zielbelastung oder Minuten je Zone), Serien, Vorlagen und Verschieben per Drag-and-drop.
- Ziele wie Wettkämpfe oder Touren im Kalender; die **Planungshilfe** schlägt die Einheiten bis zum Ziel vor (Minuten je Wochentag, Steigerung, Entlastungswochen).
- Eine Aktivität lässt sich der geplanten Einheit des Tages zuordnen (TrackCraft schlägt sie vor); die Formvorschau rechnet den offenen Plan mit ein.

**Betrieb**
- Läuft als Windows-Dienst, als Linux-systemd-Dienst (z. B. in einem Proxmox-Container) oder mit Docker.
- Eigenes HTTPS mit einer Heimnetz-Zertifizierungsstelle, die das Setup anlegt; Handy und Tablet vertrauen ihr nach einmaligem Import.
- Bedienbar am PC, Tablet und Handy, helles und dunkles Design.
- Eine Datenbank (SQLite) und die unveränderten Originaldateien; jede Kennzahl lässt sich aus den Originalen neu berechnen.
- **Export:** alle Daten als ZIP (Originale, GPX je Aktivität, Fachdaten als JSON), je Aktivität Original oder GPX, die gefilterte Aktivitätenliste als CSV.

**Noch nicht enthalten:** Später denkbar und als [Issues](../../issues) beschrieben: Ausrüstung und Wartung, Leistungsdaten (Watt), Laufprognosen, Gesundheitsdaten, mehrere Benutzer mit Anmeldung, weitere Sprachen, Raspberry Pi, MCP-Server.

## Sicherheit: nur fürs Heimnetz

TrackCraft hat in V1 **kein Login**. Wer die Oberfläche erreicht, darf alles lesen, ändern und löschen. Deshalb gehört TrackCraft ins eigene LAN oder hinter ein VPN (z. B. Tailscale oder WireGuard), **nie per Portfreigabe ins Internet**. Schutz im Browser über HTTPS, Host- und Origin-Prüfung und eine strenge Content-Security-Policy (ADR-0012, RFC-0011). Sicherheitslücken bitte wie in [SECURITY.md](SECURITY.md) beschrieben melden.

## Ausprobieren

Eine Demo mit den erfundenen Mallorca-Daten von oben läuft ohne Installation auf dem eigenen Rechner. Die Daten werden
beim Start so verschoben, dass die letzte Aktivität gestern war. Ein Hinweisband zeigt, dass es eine Demo ist. Eine
eigene TrackCraft-Installation bleibt unberührt.

- **Windows:** `trackcraft-<version>-windows-demo.zip` aus dem [neuesten Release](https://github.com/ulilaun/trackcraft/releases/latest)
  entpacken und `demo.cmd` doppelklicken. Keine Administratorrechte nötig. Der Browser öffnet sich mit
  <https://localhost:8443>. Neu beginnen, Port und Entfernen stehen in `DEMO.md`.
- **Docker:** `compose.demo.yaml` aus `trackcraft-<version>-docker.zip` nehmen und
  `docker compose -f compose.demo.yaml up` ausführen, dann <https://localhost:8443> öffnen. Die Demo-Datenbank wird
  beim ersten Start aus dem Release geladen. Beenden und alles entfernen: `docker compose -f compose.demo.yaml down -v`.

**Zertifikatswarnung:** Der Browser warnt beim ersten Aufruf („Ihre Verbindung ist nicht privat“). Das ist bei der
Demo erwartet: Sie erstellt beim ersten Start ein eigenes Zertifikat, das kein Gerät kennt. Weiter über **Erweitert**
und **Weiter zu localhost**, nur für diese Demo-Adresse. Bei der richtigen [Installation](#installation) gibt es
diese Warnung nicht.

## Installation

Fertige Pakete gibt es unter [Releases](../../releases), jeweils mit `SHA256SUMS.txt`.

| Plattform | Voraussetzungen | Paket | Anleitung |
| :--- | :--- | :--- | :--- |
| Windows | Windows 11, x86-64, 2 GB freier Arbeitsspeicher | `trackcraft-<version>-windows-x86_64.zip`, Start über `setup.cmd` | [infrastructure/windows/BETRIEB.md](infrastructure/windows/BETRIEB.md) |
| Linux | x86-64 mit systemd und glibc ≥ 2.35 (z. B. Debian 12/13, Ubuntu 22.04 und neuer), 1 GB freier Arbeitsspeicher | `trackcraft-<version>-linux-x86_64.tar.gz`, Start über `setup.sh` | [infrastructure/linux/BETRIEB.md](infrastructure/linux/BETRIEB.md) |
| Docker | x86-64-Host mit Docker Engine und dem Compose-Plugin (`docker compose`), davor ein Reverse Proxy für HTTPS | `trackcraft-<version>-docker.zip` mit `compose.yaml`; Images `ghcr.io/ulilaun/trackcraft-backend` und `-frontend`, ohne Anmeldung ladbar | [infrastructure/docker/BETRIEB.md](infrastructure/docker/BETRIEB.md) |

Unter Windows und Linux fragt das Setup nach dem Namen des Rechners im Heimnetz (z. B. `trackcraft.home.arpa`), legt Zertifikate und Konfiguration an und startet den Dienst. Ein Update ist dasselbe Setup aus dem neueren Paket; die Datenbank wird dabei automatisch migriert. Im Browser genügt eine aktuelle Version von Firefox, Chrome, Edge oder Safari.

**Kartenkacheln:** Ohne eigene Einstellung lädt der Browser die Karten direkt von den Servern der OpenStreetMap Foundation (`tile.openstreetmap.org`). Das ist für gelegentliche private Nutzung gedacht und unterliegt deren [Nutzungsrichtlinie](https://operations.osmfoundation.org/policies/tiles/), die Namensnennung blendet TrackCraft ein. Wer viel lädt oder TrackCraft für mehrere Personen betreibt, trägt in der Konfiguration einen eigenen Kachelanbieter ein:

```toml
[maps]
tile_url    = "https://tiles.example.org/{z}/{x}/{y}.png"
attribution = "© OpenStreetMap-Mitwirkende, Kacheln: Beispielanbieter"
```

**Ortsvorschlag:** Findet TrackCraft keinen bekannten Ort in der Nähe des Starts, fragt der Server den Dienst Nominatim von OpenStreetMap (`nominatim.openstreetmap.org`) mit der auf gut 10 m gerundeten Startkoordinate. Das lässt sich unter Einstellungen → Orte abschalten; mit `[places] nominatim_url` geht auch eine eigene Nominatim-Instanz.

## Häufige Fragen

**Warum gibt es kein Login?**
Bewusste Entscheidung für V1: TrackCraft ist für eine Person im eigenen Netz gebaut. Von unterwegs erreichst du es
über ein VPN wie Tailscale oder WireGuard, nicht über eine Portfreigabe. So bleibt die Angriffsfläche klein und es
gibt kein Passwort, das verloren gehen kann. Mehrere Benutzer mit Anmeldung sind als
[Erweiterung](../../issues/39) beschrieben.

**Ist das ein Strava-Ersatz?**
Für die eigene Auswertung ja: Segmente, Bestenlisten, Statistik, Form und Planung. Für Soziales nein, es gibt keine
Freunde, Kudos oder öffentlichen Bestenlisten. Deine Segmente gelten nur für deine eigenen Aktivitäten, dafür
rückwirkend für alle, auch die von vor zehn Jahren.

**Wie kommen meine alten Aktivitäten hinein?**
Per Garmin Connect (persönlicher API-Schlüssel, holt die ganze Historie) oder als FIT- und GPX-Dateien per
Drag-and-drop, etwa aus dem Datenexport einer anderen Plattform (gepackte Dateien vorher entpacken). Duplikate
erkennt TrackCraft selbst.

**Wie sichere ich meine Daten?**
Alles steckt in einem Datenordner: eine SQLite-Datenbank und die unveränderten Originaldateien. TrackCraft
sichert beides täglich in ein Sicherungsziel deiner Wahl (USB-Laufwerk, zweite Platte oder Netzwerkfreigabe),
nur bei Änderungen, und behält die letzten fünf Stände (Uhrzeit und Anzahl einstellbar); vor jeder Datenbankmigration zusätzlich.
Wiederherstellen geht auf der Seite Sicherung. Alle Daten als ZIP gibt es unter Einstellungen → Export.

**Was verlässt mein Netz?**
Nur die Abrufe von Kartenkacheln (Browser → OpenStreetMap oder dein Anbieter), der abschaltbare Ortsvorschlag
(Server → Nominatim, gerundete Startkoordinate) und, falls eingerichtet, der Abruf bei Garmin Connect. Keine
Telemetrie, keine Konten, keine Cloud.

**Läuft es auf einem Raspberry Pi oder NAS?**
Offiziell bisher auf x86-64 (Windows, Linux, Docker). Ein Proxmox-Container mit Debian reicht völlig aus, arm64 für
den Raspberry Pi ist als [Erweiterung](../../issues/7) beschrieben.

## Fragen, Fehler und Beiträge

TrackCraft ist ein Freizeitprojekt mit einem Betreuer. Fragen, Fehler und Ideen bitte als [Issue](../../issues) melden, gern mit Version (Seite System) und Plattform; Sicherheitslücken vertraulich wie in [SECURITY.md](SECURITY.md). Code-Beiträge bitte erst im Issue absprechen, Einzelheiten in [CONTRIBUTING.md](CONTRIBUTING.md).

## Entwicklung

Rust-Backend (Axum, SQLx/SQLite, rustls) und React-Frontend (React 19, TypeScript, Vite, Tailwind, Leaflet, Recharts). Unter Windows mit Rust stable (MSVC) und Node 24:

```powershell
cd backend; cargo fmt --check; cargo clippy --all-targets -- -D warnings; cargo test
cd frontend; npm ci; npm run lint; npm test; npm run build
```

Lokal ausprobieren: eine TOML mit `listen = "127.0.0.1:8443"`, `[tls]` und `allowed_origins = ["https://localhost:5173"]` anlegen, `TRACKCRAFT_CONFIG` darauf zeigen lassen und `cargo run` im Ordner `backend` starten. Dann `npm run dev` im Ordner `frontend` und `http://localhost:5173` öffnen; Vite leitet `/api` an das Backend weiter.

| Ordner | Inhalt |
| :--- | :--- |
| `backend/` | Rust/Axum, Programm `trackcraft` (RFC-0004, RFC-0007) |
| `frontend/` | React/TypeScript/Vite/Tailwind (RFC-0006) |
| `infrastructure/` | Betriebsanleitungen, Setup-Skripte, Dockerfiles, Compose |
| `scripts/` | Windows- und Linux-Paket, Coverage-Auswertung, Beispieldaten und Screenshots (`scripts/demo`) |
| `docs/` | [Spezifikation](docs/README.md): ADRs, RFCs, [OpenAPI-Vertrag](docs/api/openapi.yaml) |
| `.github/workflows/` | `ci` (Pull Requests), `release` (Tags `v*`), `linux-package` |

Die Spezifikation ist verbindlich (Spec first): Änderungen am Verhalten stehen zuerst im zugehörigen RFC. Echte Trainingsdateien enthalten Positionen und liegen nie im Repository; Tests mit eigenen Dateien laufen nur lokal über `TRACKCRAFT_TEST_CORPUS` (RFC-0014).

## Lizenz

TrackCraft steht unter der [GNU Affero General Public License v3.0 oder später](LICENSE). Kartendaten © [OpenStreetMap](https://www.openstreetmap.org/copyright)-Mitwirkende (ODbL). Lizenzen der Abhängigkeiten liegen jedem Paket als `THIRD-PARTY-LICENSES.txt` bei.
