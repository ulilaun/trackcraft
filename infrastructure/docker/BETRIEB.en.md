# Running TrackCraft with Docker

[Deutsch](BETRIEB.md) · English

Second platform according to RFC-0005: an x86-64 host with Docker Engine and the Compose plugin (command `docker compose`, not the old `docker-compose`), e.g. a mini PC with Linux or a Synology with Container Manager. Tested on Debian 13 with Docker 29 from the official Docker package repository, in a Proxmox container with `nesting` and `keyctl`. Unlike on Windows and Linux, TrackCraft does **not do its own HTTPS** in the container: a reverse proxy belongs in front of it that offers HTTPS for the name in the home network (section “Reverse proxy”).

> **Language:** the web interface is available in English (Settings → Appearance). Log messages of the program are German.

Two containers: `trackcraft-backend` (program, data) and `trackcraft-frontend` (Nginx with the user interface, forwards `/api` to the backend). The images are public at `ghcr.io/ulilaun/trackcraft-backend` and `ghcr.io/ulilaun/trackcraft-frontend`; no GitHub login is needed.

## Setup

1. Download `trackcraft-<version>-docker.zip` and `SHA256SUMS.txt` from the GitHub release, compare the checksum and unpack on the host:

   ```bash
   sha256sum -c --ignore-missing SHA256SUMS.txt
   unzip trackcraft-<version>-docker.zip
   cd trackcraft-<version>-docker
   ```

2. Create the configuration from the template and adjust it, at least `display_name` and `allowed_origins` (the address at which the reverse proxy offers TrackCraft, e.g. `https://trackcraft.home.arpa`):

   ```bash
   cp config/trackcraft.container.example.toml config/trackcraft.toml
   nano config/trackcraft.toml
   ```

3. In `compose.yaml`, enter the path of the backup target (line `/pfad/zum/sicherungsziel`, “path to backup target”), e.g. a mounted NAS share.
4. Create the data folder and hand it to the user inside the container (UID 10001); the backend container does not run as root and could not write otherwise:

   ```bash
   mkdir -p data
   sudo chown -R 10001:10001 data /path/to/backup-target
   ```

5. Start and check:

   ```bash
   docker compose up -d
   docker compose ps
   curl -s http://127.0.0.1:8081/api/v1/health
   ```

   `health` answers with `"status":"ok"` as soon as the backend is running. Other requests via `127.0.0.1` are rejected by TrackCraft with `host_not_allowed`; the user interface is only available via the name in `allowed_origins`, i.e. via the reverse proxy. If `trackcraft-backend` keeps showing “Restarting” in `docker compose ps`, `docker compose logs trackcraft-backend` names the cause, e.g. `Permission denied` if step 4 is missing.

## Reverse proxy

The frontend container can only be reached on `127.0.0.1:8081` of the host (HTTP). The reverse proxy terminates HTTPS for the name in `allowed_origins` and forwards to `http://127.0.0.1:8081`. It passes `Host`, `Origin` and `Sec-Fetch-*` on unchanged and adds no CORS headers (RFC-0005). Uploads of up to 50 MB and a running time of one hour must get through.

Example with Caddy on the same host (on Debian/Ubuntu `apt install caddy`, file `/etc/caddy/Caddyfile`, then `systemctl reload caddy`), certificate from Caddy's own local CA (`tls internal`). Install its root certificate `/var/lib/caddy/.local/share/caddy/pki/authorities/local/root.crt` as a CA on PC, phone and tablet, as described in the [Linux guide](../linux/BETRIEB.en.md#3-certificate-and-devices):

```
trackcraft.home.arpa {
    tls internal
    request_body {
        max_size 50MB
    }
    reverse_proxy 127.0.0.1:8081
}
```

On a Synology, the DSM reverse proxy does this (Control Panel → Login Portal → Advanced → Reverse Proxy), with a certificate for the name.

## Backup

> **No daily backup yet:** so far TrackCraft only backs up the database itself before a schema migration (`data/backups`). The daily backup to the backup target and restoring from the interface come with a later version. Until then back up yourself: `docker compose stop`, copy the folders `data/database` and `data/activities`, `docker compose start`.

## Update

1. In `compose.yaml`, enter the new exact version for both images (never `latest`).
2. `docker compose pull` and `docker compose up -d`.

Configuration and data stay; before a schema migration TrackCraft backs up the database to `data/backups`. New required keys are listed in the release notes.

## Rollback

An older image does not start against a newer schema and then changes nothing. Enter the old version in `compose.yaml` and run `docker compose up -d`. If the containers then keep restarting and `docker compose logs trackcraft-backend` names a schema version that is too new, the database has already been migrated:

```bash
docker compose down
mkdir -p after-update && mv data/database/trackcraft.db* after-update/
cp data/backups/trackcraft-vor-migration-v<old>-v<new>.db data/database/trackcraft.db
sudo chown 10001:10001 data/database/trackcraft.db
docker compose up -d
```

Changes since the update are then missing; the original files in `data/activities` stay as they are.

## Logs

`docker compose logs trackcraft-backend` and `data/logs/app.<date>.log`.

## Uninstall

`docker compose down` removes the containers and the network. The folder with `config` and `data` (training and position data) stays and is only deleted by hand; `docker image rm` removes the images.

## Demo

`compose.demo.yaml` starts a demo with made-up data as its own Compose project `trackcraft-demo` (its own volumes, port 8443 on 127.0.0.1 only). On first start it downloads `trackcraft-<version>-demo.zip` from the release, serves the user interface itself and uses a self-made certificate (the browser warns; that is expected). Stop and remove it with `docker compose -f compose.demo.yaml down -v`. An installation from `compose.yaml` is not affected.
