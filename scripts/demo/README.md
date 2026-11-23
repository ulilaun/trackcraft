# Beispieldaten und Screenshots

Erfundene Trainingsdaten auf Mallorca für Screenshots und Vorführungen, ohne echte Positionen.

| Datei | Zweck |
| :--- | :--- |
| `routes.json` | Sechs Strecken (Rad: Formentor, Sa Calobra, Pollença–Lluc, Alcúdia; Laufen: Hafenrunde, Vall de Boquer). Aus OpenStreetMap-Daten (© OpenStreetMap-Mitwirkende, ODbL), geroutet mit [BRouter](https://brouter.de). |
| `make_demo.py` | Erzeugt daraus rund 180 GPX-Aktivitäten von März 2025 bis November 2026 mit erfundenem Tempo, Puls und GPS-Rauschen (fester Zufallswert, reproduzierbar). |
| `seed_demo.py` | Importiert die Dateien in eine **leere** Test-Instanz, legt zwei Saisons, fünf Segmente und zwei Strecken an. |
| `screenshots.mjs` | Fotografiert Dashboard, Aktivitätenliste, Aktivität, Segment, Statistik und eine Tablet-Ansicht, hell und dunkel, nach `docs/screenshots/`, und prüft danach, dass keine Hauptseite bei 360 px Breite seitlich scrollt (sonst Exit-Code 1). |

## Ablauf

1. Test-Instanz mit eigener TOML starten (nie die echte Installation): `listen = "127.0.0.1:8443"`, `allowed_origins = ["https://localhost:8443"]`, `static_dir` auf `frontend/dist`, leere Datenordner, selbst signiertes Zertifikat, z. B.
   `openssl req -x509 -newkey rsa:2048 -nodes -keyout server-key.pem -out server.pem -days 30 -subj "/CN=localhost" -addext "subjectAltName=DNS:localhost"`
2. `python scripts/demo/make_demo.py <ordner>`
3. `python scripts/demo/seed_demo.py <ordner>` (dauert einige Minuten, die Segmentauswertung läuft im Hintergrund)
4. Im Ordner `scripts/demo`: `npm ci`, dann `npm run screenshots` (mit `PW_CHANNEL=chrome` bzw. `msedge`, um einen installierten Browser statt des Playwright-Chromiums zu verwenden; sonst vorher `npx playwright install chromium`)

Das Dashboard bezieht sich auf das heutige Datum; liegen die Beispieldaten weit zurück, `LAST_DAY` in `make_demo.py` anpassen.
