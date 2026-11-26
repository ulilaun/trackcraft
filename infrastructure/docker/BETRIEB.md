# TrackCraft mit Docker betreiben

Deutsch · [English](BETRIEB.en.md)

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

TrackCraft sichert täglich ab einer Uhrzeit (Vorgabe `[backup] daily_time`, sonst 03:00; auf der Seite **Daten** änderbar) ins Sicherungsziel, aber nur, wenn sich seit der letzten Sicherung etwas geändert hat und das Ziel erreichbar ist. Läuft der Container um diese Zeit nicht, holt TrackCraft den Versuch nach dem Start einmal nach. Die Seite **Daten** zeigt Status, Fehler und die vorhandenen Stände und hat den Knopf „Jetzt sichern“.

Je Stand entsteht im Ziel ein Ordner `trackcraft-<datum>-<uhrzeit>-s<schema>-c<zähler>` mit `trackcraft.db`, den Originaldateien (`originals`), einer Kopie der Konfiguration und `manifest.json` mit Größe und SHA-256 jeder Datei. TrackCraft prüft jeden Stand nach dem Kopieren im Ziel; die letzten fünf bleiben (auf der Seite Daten zwischen 1 und 30 einstellbar), ältere entfernt es erst nach einem neuen Erfolg. Sicherungen sind unverschlüsselt: das Ziel vor fremdem Zugriff schützen. Ein Ziel auf demselben Rechner schützt nicht gegen dessen Verlust.

**Wiederherstellen:** Auf der Seite **Daten** hat jeder Stand den Knopf „Wiederherstellen“. TrackCraft prüft den Stand vollständig, zeigt, ob er älter, gleich oder neuer als die aktuellen Daten ist, und verlangt bei einem älteren oder nicht vergleichbaren Stand eine ausdrückliche Bestätigung. Dann ersetzt es Datenbank und Originaldateien vollständig und startet dabei kurz neu; ein Stand einer älteren Version wird dabei aktualisiert. Geht etwas schief, nimmt TrackCraft das Umschalten zurück. Der bisherige Bestand bleibt bis zum nächsten Wiederherstellen in `data/database/vor-wiederherstellung`. Konfiguration und Sicherungen werden nicht ersetzt. Vorübergehend braucht es freien Platz in der Größe des Stands. So gelingt auch der Umzug auf einen anderen Rechner: dort TrackCraft einrichten, dasselbe Sicherungsziel eintragen und den letzten Stand wiederherstellen.

Das Sicherungsziel ist in `compose.yaml` als `/backup-target` eingehängt (Zeile `/pfad/zum/sicherungsziel` anpassen), in der Konfiguration steht `target_dir = "/backup-target"`. Der Container läuft als Benutzer 10001; der Ordner muss für ihn beschreibbar sein, z. B. `sudo chown 10001:10001 /pfad/zum/sicherungsziel`.

## Geheimnisse

Gespeicherte Geheimnisse wie der Garmin-Schlüssel liegen nur verschlüsselt in der Datenbank. Den Schlüssel dafür enthält `data/secrets.key` (im Container `/data/secrets.key`, Einstellung `[secrets] key_file`), angelegt beim ersten Start. Sicherungen und Export enthalten ihn nie.

- Die Datei nicht löschen und nicht weitergeben. Ohne sie sind gespeicherte Geheimnisse nicht mehr lesbar; die Seite Einstellungen → Garmin Connect meldet das, der Abruf ruht, bis der Schlüssel neu eingetragen ist.
- Bei einem Umzug auf einen anderen Rechner die Datei mitkopieren, dann bleibt der Garmin-Schlüssel gültig. Sonst nach dem Wiederherstellen den Garmin-Schlüssel neu eintragen. Wer das ganze Datenvolume sichert, sichert die Datei mit; dann dieses Ziel genauso schützen.

## Update

1. In `compose.yaml` bei beiden Images die neue genaue Version eintragen (nie `latest`).
2. `docker compose pull` und `docker compose up -d`.

Konfiguration und Daten bleiben; vor einer Schemamigration legt TrackCraft eine Vollsicherung in `data/backups` an und kopiert sie, wenn erreichbar, ins Sicherungsziel. Neue Pflichtschlüssel nennen die Release Notes.

## Rollback

Ein älteres Image startet nicht gegen ein neueres Schema und ändert dann nichts. Alte Version in `compose.yaml` eintragen und `docker compose up -d`. Starten die Container danach immer wieder neu und nennt `docker compose logs trackcraft-backend` eine zu neue Schemaversion, wurde schon migriert:

```bash
docker compose down
mkdir -p nach-update && mv data/database/trackcraft.db* nach-update/
cp data/backups/trackcraft-vor-migration-v<alt>-v<neu>/trackcraft.db data/database/trackcraft.db
sudo chown 10001:10001 data/database/trackcraft.db
docker compose up -d
```

Änderungen seit dem Update fehlen danach; die Originaldateien in `data/activities` bleiben, wie sie sind.

## Logs

`docker compose logs trackcraft-backend` sowie `data/logs/app.<datum>.log`. Die 30 neuesten Tagesdateien bleiben. Sie enthalten keine Schlüssel, Positionen oder Gesundheitswerte, aber Dateinamen von Uploads – vor dem Weitergeben durchsehen.

## Deinstallieren

`docker compose down` entfernt die Container und das Netz. Der Ordner mit `config` und `data` (Trainings- und Positionsdaten) bleibt stehen und wird nur von Hand gelöscht; die Images entfernt `docker image rm`.

## Demo

`compose.demo.yaml` startet eine Demo mit erfundenen Daten als eigenes Compose-Projekt `trackcraft-demo` (eigene
Volumes, Port 8443 nur auf 127.0.0.1). Sie lädt beim ersten Start `trackcraft-<version>-demo.zip` aus dem Release,
liefert die Oberfläche selbst aus und verwendet ein selbst erstelltes Zertifikat (der Browser warnt, das ist erwartet).
Beenden und entfernen: `docker compose -f compose.demo.yaml down -v`. Die Installation nach `compose.yaml` bleibt
unberührt.
