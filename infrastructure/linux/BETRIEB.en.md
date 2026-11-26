# Running TrackCraft on Linux

[Deutsch](BETRIEB.md) · English

Applies to the package `trackcraft-<version>-linux-x86_64.tar.gz` (RFC-0005 “Linux”, RFC-0015). TrackCraft runs as the systemd service `trackcraft` under its own user, serves the user interface itself and speaks HTTPS only, just like on Windows. Tested on Debian 13 in a Proxmox container (LXC), last on 23.11.2026 on a fresh container with installation, import, update, backup and rollback.

> **Language:** the web interface is available in English (Settings → Appearance). The setup and the command-line tool talk German; this guide quotes their questions and messages in German with a translation.

## Requirements

- Linux x86-64 with systemd and glibc 2.35 or newer (e.g. Debian 12/13, Ubuntu 22.04 and newer), at least 1 GB of free RAM.
- A fixed IP address in the home network and a name that the router resolves, e.g. `<hostname>.fritz.box` on a FRITZ!Box (the container's host name) or `trackcraft.home.arpa` in the router's DNS.
- Port 443 is free on the machine.

### Proxmox container

Unprivileged container from the Debian template, 2 cores, 2 GB RAM, disk according to your data (program about 50 MB, plus original files and database). Leave the `nesting` option switched on (the default in the Proxmox UI): TrackCraft itself does not need it, but without it some system services fail on Debian 13 (`systemctl is-system-running` reports “degraded”). Give the container a fixed IP in the router.

## 1. Download and verify

Download the package and `SHA256SUMS.txt` from the [GitHub release](https://github.com/ulilaun/trackcraft/releases), directly on the machine e.g. with `wget` (adjust the version):

```bash
V=1.0.0
wget https://github.com/ulilaun/trackcraft/releases/download/v$V/trackcraft-$V-linux-x86_64.tar.gz \
     https://github.com/ulilaun/trackcraft/releases/download/v$V/SHA256SUMS.txt
sha256sum -c --ignore-missing SHA256SUMS.txt
tar -xzf trackcraft-$V-linux-x86_64.tar.gz
cd trackcraft-$V-linux-x86_64
```

`sha256sum` must report `OK`. If an older `SHA256SUMS.txt` is still in the folder, `wget` saves the new one as `SHA256SUMS.txt.1`; delete the old one first.

## 2. Install

```bash
sudo ./setup.sh
```

`setup` asks the following questions, each with a suggestion in square brackets (Enter accepts it):

| Question (German) | Meaning | Suggestion | Note |
| :--- | :--- | :--- | :--- |
| Namen | host names | `localhost, <hostname -f>` | comma-separated, e.g. `trackcraft.home.arpa, localhost` |
| Dein Name | your name | `TrackCraft` | your profile name in TrackCraft (exactly one user, no login) |
| Sicherungsziel | backup target | `/mnt/trackcraft-sicherung` | mounted NAS share or second disk, can be changed later |
| Port | port | `443` | |

`setup` then creates:

| What | Where |
| :--- | :--- |
| Program and user interface | `/opt/trackcraft` |
| Configuration | `/etc/trackcraft/trackcraft.toml` (root, readable by the service) |
| CA and certificate | `/etc/trackcraft/ca` (root only), `/etc/trackcraft/tls` |
| Data, logs, migration backups | `/var/lib/trackcraft` (user `trackcraft`) |
| Service | `/etc/systemd/system/trackcraft.service` |

It then checks the configuration, starts the service and waits until it answers. At the end it lists the addresses, e.g. `https://trackcraft.home.arpa`.

With options instead of questions:

```bash
sudo ./setup.sh --names trackcraft.home.arpa,localhost --display-name Alex --backup-dir /mnt/trackcraft-backup
```

The service runs as the user `trackcraft` without a login. It may only open port 443 and only write to `/var/lib/trackcraft` and the backup target; the rest of the system is read-only for it.

## 3. Certificate and devices

`setup` creates its own TrackCraft CA and uses it to issue the server certificate for the configured names. The CA may only confirm these names. On every device that uses TrackCraft (PC, phone, tablet), install the public file `/etc/trackcraft/TrackCraft-CA.crt` as a trusted CA. Fetch it from the PC:

```bash
scp root@trackcraft.home.arpa:/etc/trackcraft/TrackCraft-CA.crt .
```

- Windows: double-click → Install Certificate → Local Machine → “Trusted Root Certification Authorities”.
- Android: Settings → Security → Encryption & credentials → Install a certificate → CA certificate.
- iOS: open the file, install the profile, then Settings → General → About → Certificate Trust Settings → switch on TrackCraft.

The server certificate is valid for 825 days; `setup` renews it on every update if it expires in less than 30 days or the names have changed. By hand, or with new names:

```bash
sudo /opt/trackcraft/trackcraft cert
sudo /opt/trackcraft/trackcraft cert --names trackcraft.home.arpa,localhost
```

If new names lie outside the previous CA, the command creates a new CA; then install the new CA file on the devices and remove the old one.

## Backup target

TrackCraft backs up daily from a set time (default `[backup] daily_time`, otherwise 03:00; changeable on the **Backup** page) to the backup target, but only if something changed since the last backup and the target is reachable. If the computer is not running at that time, TrackCraft makes up the attempt once after starting. The **Backup** page shows status, errors and the existing backups and has a “Back up now” button.

Each backup is a folder `trackcraft-<date>-<time>-s<schema>-c<counter>` in the target with `trackcraft.db`, the original files (`originals`), a copy of the configuration and `manifest.json` with size and SHA-256 of every file. TrackCraft verifies each backup in the target after copying; the last five are kept (adjustable between 1 and 30 on the Backup page), older ones are removed only after a new success. Backups are not encrypted: protect the target from access by others. A target on the same computer does not protect against losing that computer.

**Restore:** On the **Backup** page every backup has a “Restore” button. TrackCraft checks the backup completely, shows whether it is older than, equal to or newer than the current data, and asks for explicit confirmation for an older or non-comparable backup. It then replaces the database and the original files completely and restarts briefly; a backup from an older version is updated along the way. If something goes wrong, TrackCraft rolls the switch back. The previous data stays in `/var/lib/trackcraft/database/vor-wiederherstellung` until the next restore. Configuration and backups are not replaced. Free space the size of the backup is needed temporarily. This also works for moving to another computer: set up TrackCraft there, enter the same backup target and restore the latest backup.

Proxmox users can additionally back up the whole container (vzdump, mode “Stop” or “Snapshot”).

Mount a NAS share on the Proxmox host and pass it into the container as a bind mount, or mount it directly in the container (e.g. CIFS in `/etc/fstab`). Enter the path in `[backup] target_dir` and run `setup.sh` again: it makes the folder writable for the service and adds it to the systemd unit.

## Operation

| Command | Effect |
| :--- | :--- |
| `systemctl status trackcraft` | state |
| `sudo systemctl restart trackcraft` | restart, e.g. after changing the configuration |
| `journalctl -u trackcraft -n 100` | service output, including start errors |
| `sudo -u trackcraft /opt/trackcraft/trackcraft check-config` | check the configuration, changes nothing |

- Logs: `/var/lib/trackcraft/logs/app.<date>.log` and the journal. The 30 most recent daily files are kept. They contain no keys, positions or health values, but do contain upload file names – check them before passing them on.
- After a crash or start error, systemd restarts the service every five seconds (`systemctl status` then shows “activating (auto-restart)”); the cause is in the journal.

## Moving from Windows

Data and original files are platform-independent; the database only stores relative paths.

1. Run `setup.sh` on the Linux machine and check that TrackCraft starts empty; then `sudo systemctl stop trackcraft`.
2. On Windows, stop the service (administrator): `trackcraft.exe service stop`.
3. Copy the folders `database` and `activities` from `C:\ProgramData\TrackCraft\data` completely to `/var/lib/trackcraft/`, replacing the existing folders there. The folder is only readable for the service, so copy as administrator, e.g. `robocopy C:\ProgramData\TrackCraft\data\database C:\trackcraft\move\database /E` (same for `activities`). If a non-empty `trackcraft.db-wal` lies next to the database afterwards, the service was still running: copy again. Unpack an archive on Linux with `tar --no-same-owner -xzf …`; otherwise `tar` fails on the Windows owners in an unprivileged container.
4. Take over values from the Windows configuration that differ from the template (e.g. `[segments.matching]`, time zone, display name) into `/etc/trackcraft/trackcraft.toml`. Paths and `[server]`/`[tls]` stay the Linux ones.
5. Run `sudo ./setup.sh` again: it sets the permissions on the copied data and starts the service.
6. On Windows, run `trackcraft.exe uninstall` so that the old service does not start again.

## Update

1. Download, verify and unpack the new package as in section 1. Keep the previous one.
2. `sudo ./setup.sh` from the new folder.

`setup` detects the installation, stops the service, replaces program and user interface, renews the certificate if needed and starts again. Configuration and data stay; before a schema migration TrackCraft makes a full backup in `/var/lib/trackcraft/backups` and, if reachable, copies it to the backup target.

## Rollback

An older program does not start against a newer schema and then changes nothing. Run `sudo ./setup.sh` from the old package you kept. If it reports “Der Dienst ist beim Start abgebrochen” (the service aborted on start) and `journalctl -u trackcraft` names a schema version that is too new, the database has already been migrated; then restore the database from before the migration:

```bash
sudo systemctl stop trackcraft
cd /var/lib/trackcraft
sudo mkdir -p /root/after-update && sudo mv database/trackcraft.db* /root/after-update/
sudo cp backups/trackcraft-vor-migration-v<old>-v<new>/trackcraft.db database/trackcraft.db
sudo chown trackcraft:trackcraft database/trackcraft.db
sudo systemctl start trackcraft
```

Changes since the update are then missing; the original files in `activities` stay as they are.

## Uninstall

```bash
sudo systemctl disable --now trackcraft
sudo rm /etc/systemd/system/trackcraft.service && sudo systemctl daemon-reload
sudo rm -r /opt/trackcraft
```

`/etc/trackcraft` and `/var/lib/trackcraft` (training and position data) stay and are only deleted by hand. Remove the TrackCraft CA from the devices.
