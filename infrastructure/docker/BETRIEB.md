# TrackCraft mit Docker betreiben

Zweite Plattform nach RFC-0005: x86-64-Host mit Docker und Docker Compose v2, z. B. Synology mit Container Manager oder Mini-PC mit Linux. In CI gebaut und getestet, nicht auf eigener Hardware abgenommen.

## Einrichten

1. `trackcraft-<version>-docker.zip` aus dem GitHub Release laden, Prüfsumme mit `SHA256SUMS.txt` vergleichen, in einen Ordner auf dem Host entpacken.
2. Einmal an der privaten Registry anmelden, mit einem Token, das nur `read:packages` darf:

   ```bash
   docker login ghcr.io -u <github-name>
   ```

3. `config/trackcraft.toml` aus `trackcraft.container.example.toml` anlegen und anpassen (Anzeigename, `allowed_origins`).
4. In `compose.yaml` den Pfad des Sicherungsziels eintragen.
5. Starten:

   ```bash
   docker compose up -d
   ```

## Reverse Proxy

Der Frontend-Container ist nur auf `127.0.0.1:8081` des Hosts erreichbar. Der Reverse Proxy (auf der Synology der von DSM) terminiert HTTPS für den Namen aus `allowed_origins` und leitet an `http://127.0.0.1:8081` weiter. Er reicht `Host`, `Origin` und `Sec-Fetch-*` unverändert weiter und fügt keine CORS-Header hinzu (RFC-0005). Uploads bis 50 MB und eine Stunde Laufzeit müssen durchgehen.

## Update

1. Version in `compose.yaml` auf die neue genaue Version ändern.
2. `docker compose pull` und `docker compose up -d`.

Vor einer Schemamigration legt TrackCraft eine Sicherung in `data/backups` an. Rollback: alte Version eintragen und nach RFC-0015 verfahren; ein älteres Image startet nicht gegen ein neueres Schema.

## Logs

`docker compose logs trackcraft-backend` sowie `data/logs/app.<datum>.log`.
