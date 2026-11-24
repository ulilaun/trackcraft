# Beispieldaten und Screenshots

Erfundene Trainingsdaten auf Mallorca für Screenshots und Vorführungen, ohne echte Positionen.

| Datei | Zweck |
| :--- | :--- |
| `routes.json` | Sechs Strecken (Rad: Formentor, Sa Calobra, Pollença–Lluc, Alcúdia; Laufen: Hafenrunde, Vall de Boquer). Aus OpenStreetMap-Daten (© OpenStreetMap-Mitwirkende, ODbL), geroutet mit [BRouter](https://brouter.de). |
| `make_demo.py` | Erzeugt daraus rund 190 GPX-Aktivitäten über 21 Monate bis zum letzten Tag (Vorgabe gestern, zweites Argument `JJJJ-MM-TT`) mit erfundenem Tempo, Puls und GPS-Rauschen (fester Zufallswert, mit festem letzten Tag reproduzierbar). |
| `seed_demo.py` | Importiert die Dateien in eine **leere** Test-Instanz, legt zwei Saisons, fünf Segmente und zwei Strecken an. |
| `build_demo_db.py` | Baut die Demo-Datenbank des Releases (`trackcraft-<version>-demo.zip`): frische Instanz auf Port 18443, `make_demo.py`, `seed_demo.py`, dann `trackcraft demo mark` (Kennzeichen, nur damit verschiebt `[demo] enabled` die Daten). Braucht `openssl`. Etwa 15 Minuten. |
| `package_windows_demo.py` | Packt die portable Windows-Demo (`trackcraft-<version>-windows-demo.zip`) aus dem Windows-ZIP und dem Demo-ZIP: `trackcraft.exe`, `web/`, `demo-daten/`, `demo.cmd`, `DEMO.md`. `demo.cmd` ruft `trackcraft demo start` auf: Daten, selbst erstelltes Zertifikat und Konfiguration in `%LOCALAPPDATA%\TrackCraft-Demo`, Port 8443 nur auf 127.0.0.1, ohne Administratorrechte. |
| `screenshots.mjs` | Fotografiert Dashboard, Aktivitätenliste, Aktivität, Segment, Statistik und eine Tablet-Ansicht, hell und dunkel, nach `docs/screenshots/`, und prüft danach, dass keine Hauptseite bei 360 px Breite seitlich scrollt (sonst Exit-Code 1). |

## Ablauf

1. Test-Instanz mit eigener TOML starten (nie die echte Installation): `listen = "127.0.0.1:8443"`, `allowed_origins = ["https://localhost:8443"]`, `static_dir` auf `frontend/dist`, leere Datenordner, selbst signiertes Zertifikat, z. B.
   `openssl req -x509 -newkey rsa:2048 -nodes -keyout server-key.pem -out server.pem -days 30 -subj "/CN=localhost" -addext "subjectAltName=DNS:localhost"`
2. `python scripts/demo/make_demo.py <ordner>`
3. `python scripts/demo/seed_demo.py <ordner>` (dauert einige Minuten, die Segmentauswertung läuft im Hintergrund)
4. Im Ordner `scripts/demo`: `npm ci`, dann `npm run screenshots` (mit `PW_CHANNEL=chrome` bzw. `msedge`, um einen installierten Browser statt des Playwright-Chromiums zu verwenden; sonst vorher `npx playwright install chromium`)

Das Dashboard bezieht sich auf das heutige Datum; die Beispieldaten enden deshalb standardmäßig gestern.

Demo-Datenbank lokal bauen: `python scripts/demo/build_demo_db.py backend/target/release/trackcraft dist/trackcraft-demo.zip`

Windows-Demo lokal packen: erst `./scripts/package-windows.ps1 -Version 0.0.0-dev1 -Local`, dann
`python scripts/demo/package_windows_demo.py dist/trackcraft-0.0.0-dev1-windows-x86_64.zip dist/trackcraft-demo.zip dist/trackcraft-windows-demo.zip`.
