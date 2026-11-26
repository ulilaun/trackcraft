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

TrackCraft backs up daily from a set time (default `[backup] daily_time`, otherwise 03:00; changeable on the **Data** page) to the backup target, but only if something changed since the last backup and the target is reachable. If the container is not running at that time, TrackCraft makes up the attempt once after starting. The **Data** page shows status, errors and the existing backups and has a “Back up now” button.

Each backup is a folder `trackcraft-<date>-<time>-s<schema>-c<counter>` in the target with `trackcraft.db`, the original files (`originals`), a copy of the configuration and `manifest.json` with size and SHA-256 of every file. TrackCraft verifies each backup in the target after copying; the last five are kept (adjustable between 1 and 30 on the Data page), older ones are removed only after a new success. Backups are not encrypted: protect the target from access by others. A target on the same computer does not protect against losing that computer.

**Restore:** On the **Data** page every backup has a “Restore” button. TrackCraft checks the backup completely, shows whether it is older than, equal to or newer than the current data, and asks for explicit confirmation for an older or non-comparable backup. It then replaces the database and the original files completely and restarts briefly; a backup from an older version is updated along the way. If something goes wrong, TrackCraft rolls the switch back. The previous data stays in `data/database/vor-wiederherstellung` until the next restore. Configuration and backups are not replaced. Free space the size of the backup is needed temporarily. This also works for moving to another computer: set up TrackCraft there, enter the same backup target and restore the latest backup.

The backup target is mounted as `/backup-target` in `compose.yaml` (adjust the line `/pfad/zum/sicherungsziel`), the configuration says `target_dir = "/backup-target"`. The container runs as user 10001; the folder must be writable for it, e.g. `sudo chown 10001:10001 /path/to/backup-target`.

## Secrets

Stored secrets such as the Garmin key are kept in the database only in encrypted form. The key for this is in `data/secrets.key` (inside the container `/data/secrets.key`, setting `[secrets] key_file`), created on first start. Backups and exports never contain it.

- Do not delete or share the file. Without it, stored secrets can no longer be read; Settings → Garmin Connect reports this and downloads pause until the key is entered again.
- When moving to another machine, copy the file along and the Garmin key stays valid. Otherwise enter the Garmin key again after restoring. If you back up the whole data volume, the file is included; protect that target accordingly.

## Update

1. In `compose.yaml`, enter the new exact version for both images (never `latest`).
2. `docker compose pull` and `docker compose up -d`.

Configuration and data stay; before a schema migration TrackCraft makes a full backup in `data/backups` and, if reachable, copies it to the backup target. New required keys are listed in the release notes.

## Rollback

An older image does not start against a newer schema and then changes nothing. Enter the old version in `compose.yaml` and run `docker compose up -d`. If the containers then keep restarting and `docker compose logs trackcraft-backend` names a schema version that is too new, the database has already been migrated:

```bash
docker compose down
mkdir -p after-update && mv data/database/trackcraft.db* after-update/
cp data/backups/trackcraft-vor-migration-v<old>-v<new>/trackcraft.db data/database/trackcraft.db
sudo chown 10001:10001 data/database/trackcraft.db
docker compose up -d
```

Changes since the update are then missing; the original files in `data/activities` stay as they are.

## Logs

`docker compose logs trackcraft-backend` and `data/logs/app.<date>.log`. The 30 most recent daily files are kept. They contain no keys, positions or health values, but do contain upload file names – check them before passing them on.

## Uninstall

`docker compose down` removes the containers and the network. The folder with `config` and `data` (training and position data) stays and is only deleted by hand; `docker image rm` removes the images.

## Demo

`compose.demo.yaml` starts a demo with made-up data as its own Compose project `trackcraft-demo` (its own volumes, port 8443 on 127.0.0.1 only). On first start it downloads `trackcraft-<version>-demo.zip` from the release, serves the user interface itself and uses a self-made certificate (the browser warns; that is expected). Stop and remove it with `docker compose -f compose.demo.yaml down -v`. An installation from `compose.yaml` is not affected.
