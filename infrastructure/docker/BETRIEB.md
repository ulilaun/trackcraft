# TrackCraft mit Docker betreiben

Zweite Plattform nach RFC-0005: x86-64-Host mit Docker Engine und dem Compose-Plugin (Befehl `docker compose`, nicht das alte `docker-compose`), z. B. ein Mini-PC mit Linux oder eine Synology mit Container Manager. Erprobt auf Debian 13 mit Docker 29 aus dem offiziellen Docker-Paketarchiv, in einem Proxmox-Container mit `nesting` und `keyctl`. Anders als unter Windows und Linux macht TrackCraft im Container **kein eigenes HTTPS**: Davor gehört ein Reverse Proxy, der HTTPS für den Namen im Heimnetz anbietet (Abschnitt „Reverse Proxy“).

Zwei Container: `trackcraft-backend` (Programm, Daten) und `trackcraft-frontend` (Nginx mit der Oberfläche, leitet `/api` ans Backend). Die Images liegen öffentlich unter `ghcr.io/ulilaun/trackcraft-backend` und `ghcr.io/ulilaun/trackcraft-frontend`; eine Anmeldung bei GitHub ist nicht nötig.

## Einrichten

1. `trackcraft-<version>-docker.zip` und `SHA256SUMS.txt` aus dem GitHub Release laden, Prüfsumme vergleichen und auf dem Host entpacken:

   ```bash
   sha256sum -c --ignore-missing SHA256SUMS.txt
   unzip trackcraft-<version>-docker.zip
   cd trackcraft-<version>-docker
   ```

2. Konfiguration aus der Vorlage anlegen und anpassen, mindestens `display_name` und `allowed_origins` (die Adresse, unter der der Reverse Proxy TrackCraft anbietet, z. B. `https://trackcraft.home.arpa`):

   ```bash
   cp config/trackcraft.container.example.toml config/trackcraft.toml
   nano config/trackcraft.toml
   ```

3. In `compose.yaml` den Pfad des Sicherungsziels eintragen (Zeile `/pfad/zum/sicherungsziel`), z. B. eine eingehängte NAS-Freigabe.
4. Datenordner anlegen und dem Benutzer im Container (UID 10001) geben; der Backend-Container läuft nicht als root und darf sonst nicht schreiben:

   ```bash
   mkdir -p data
   sudo chown -R 10001:10001 data /pfad/zum/sicherungsziel
   ```

5. Starten und prüfen:

   ```bash
   docker compose up -d
   docker compose ps
   curl -s http://127.0.0.1:8081/api/v1/health
   ```

   `health` antwortet mit `"status":"ok"`, sobald das Backend läuft. Andere Aufrufe über `127.0.0.1` weist TrackCraft mit `host_not_allowed` ab; die Oberfläche gibt es nur über den Namen aus `allowed_origins`, also über den Reverse Proxy. Steht `trackcraft-backend` in `docker compose ps` dauernd auf „Restarting“, nennt `docker compose logs trackcraft-backend` die Ursache, z. B. `Permission denied`, wenn Schritt 4 fehlt.

## Reverse Proxy

Der Frontend-Container ist nur auf `127.0.0.1:8081` des Hosts erreichbar (HTTP). Der Reverse Proxy terminiert HTTPS für den Namen aus `allowed_origins` und leitet an `http://127.0.0.1:8081` weiter. Er reicht `Host`, `Origin` und `Sec-Fetch-*` unverändert weiter und fügt keine CORS-Header hinzu (RFC-0005). Uploads bis 50 MB und eine Stunde Laufzeit müssen durchgehen.

Beispiel mit Caddy auf demselben Host (unter Debian/Ubuntu `apt install caddy`, Datei `/etc/caddy/Caddyfile`, danach `systemctl reload caddy`), Zertifikat aus Caddys eigener lokaler CA (`tls internal`). Deren Stammzertifikat `/var/lib/caddy/.local/share/caddy/pki/authorities/local/root.crt` auf PC, Handy und Tablet als CA installieren, wie in der [Linux-Anleitung](../linux/BETRIEB.md#3-zertifikat-und-geräte) beschrieben:

```
trackcraft.home.arpa {
    tls internal
    request_body {
        max_size 50MB
    }
    reverse_proxy 127.0.0.1:8081
}
```

Auf einer Synology übernimmt das der Reverse Proxy von DSM (Systemsteuerung → Anmeldeportal → Erweitert → Reverse Proxy), mit einem Zertifikat für den Namen.

## Sicherung

> **Noch keine tägliche Sicherung:** TrackCraft legt bisher nur vor einer Schemamigration selbst eine Sicherung der Datenbank an (`data/backups`). Die tägliche Sicherung ins Sicherungsziel und die Wiederherstellung aus der Oberfläche kommen mit einer späteren Version. Bis dahin selbst sichern: `docker compose stop`, die Ordner `data/database` und `data/activities` kopieren, `docker compose start`.

## Update

1. In `compose.yaml` bei beiden Images die neue genaue Version eintragen (nie `latest`).
2. `docker compose pull` und `docker compose up -d`.

Konfiguration und Daten bleiben; vor einer Schemamigration legt TrackCraft eine Sicherung in `data/backups` an. Neue Pflichtschlüssel nennen die Release Notes.

## Rollback

Ein älteres Image startet nicht gegen ein neueres Schema und ändert dann nichts. Alte Version in `compose.yaml` eintragen und `docker compose up -d`. Starten die Container danach immer wieder neu und nennt `docker compose logs trackcraft-backend` eine zu neue Schemaversion, wurde schon migriert:

```bash
docker compose down
mkdir -p nach-update && mv data/database/trackcraft.db* nach-update/
cp data/backups/trackcraft-vor-migration-v<alt>-v<neu>.db data/database/trackcraft.db
sudo chown 10001:10001 data/database/trackcraft.db
docker compose up -d
```

Änderungen seit dem Update fehlen danach; die Originaldateien in `data/activities` bleiben, wie sie sind.

## Logs

`docker compose logs trackcraft-backend` sowie `data/logs/app.<datum>.log`.

## Deinstallieren

`docker compose down` entfernt die Container und das Netz. Der Ordner mit `config` und `data` (Trainings- und Positionsdaten) bleibt stehen und wird nur von Hand gelöscht; die Images entfernt `docker image rm`.
