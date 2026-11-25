# Running TrackCraft on Windows

[Deutsch](BETRIEB.md) · English

Applies to the Windows ZIP `trackcraft-<version>-windows-x86_64.zip` (RFC-0005, RFC-0015). TrackCraft runs as the Windows service `TrackCraft`, serves the user interface itself and speaks HTTPS only. Last walked through on 23.11.2026 on Windows 11: fresh setup after `uninstall` with an existing configuration, migration of old data, import, update run.

> **Language:** the web interface is available in English (Settings → Appearance). The setup and the command-line tool talk German; this guide quotes their questions and messages in German with a translation.

## Requirements

- Windows 11, x86-64, at least 2 GB of free RAM.
- A fixed IP address for the PC in the home network and a name that the router resolves. On a FRITZ!Box this is usually `<computername>.fritz.box`, otherwise an entry in the router's DNS, e.g. `trackcraft.home.arpa`. For a first test on the PC itself, `localhost` is enough.
- The network profile of the home network is set to “Private” (Settings → Network & internet). With the profile “Public”, TrackCraft cannot be reached.

## 1. Download and verify

1. Download the ZIP and `SHA256SUMS.txt` from the GitHub release.
2. Compare the checksum (PowerShell):

   ```powershell
   Get-FileHash .\trackcraft-1.0.0-windows-x86_64.zip -Algorithm SHA256
   ```

   The value must match the line in `SHA256SUMS.txt`.
3. Right-click the ZIP → Properties → tick “Unblock”. Otherwise SmartScreen warns on start, because TrackCraft is not signed.
4. Extract the ZIP where it is (right-click → “Extract All”), e.g. in the Downloads folder. Do not double-click `setup.cmd` inside the opened ZIP: it would miss the other files.

### For developers: locally built package

To try your own state, the script builds a package with the same layout from the repository (Rust and Node required):

```powershell
.\scripts\package-windows.ps1 -Version 0.1.0-dev -Local
```

Result: `dist\trackcraft-0.1.0-dev-windows-x86_64.zip` and next to it the same content extracted, including `setup.cmd`.

## 2. Install

Double-click `setup.cmd` in the extracted folder. It requests administrator rights (Windows asks) and then asks the following questions, each with a suggestion in square brackets; Enter accepts it:

| Question (German) | Meaning | Suggestion | Note |
| :--- | :--- | :--- | :--- |
| Installationsordner | installation folder | `C:\Program Files\TrackCraft` | not inside a user folder |
| Namen | host names | `localhost, <computername>` | comma-separated, e.g. `pc.home.arpa, localhost` |
| Dein Name | your name | Windows user name | your profile name in TrackCraft (exactly one user, no login) |
| Sicherungsziel | backup target | `C:\ProgramData\TrackCraft\sicherung` | better a USB drive or NAS, can be changed later |
| Port | port | `443` | |

`setup` then copies the program into the installation folder, creates the configuration and data folders under `C:\ProgramData\TrackCraft`, creates the HTTPS certificate (section 3), sets up the service, access rights, event source and firewall rule, checks the configuration and starts the service. At the end it lists the addresses, e.g. `https://pc.home.arpa`.

With options instead of questions, e.g. for a different installation folder:

```powershell
.\setup.cmd --install-dir "D:\Programs\TrackCraft" --names pc.home.arpa,localhost --display-name Alex --backup-dir E:\TrackCraft-Backup
```

Every option given replaces its question; missing ones are still asked. The configuration then lives in `C:\ProgramData\TrackCraft\config\trackcraft.toml` and can be adjusted there; changes take effect after restarting the service.

Open TrackCraft in the browser only via one of the configured names. Access via the IP address is rejected.

## 3. Certificate and phone

`setup` creates its own TrackCraft CA and uses it to issue the server certificate for the configured names. The CA may only confirm these names. On the PC, `setup` marks it as trusted itself, so the browser on the PC shows no warning.

Copy the file `C:\ProgramData\TrackCraft\TrackCraft-CA.crt` to your phone or tablet (by mail, cloud folder or USB; it can be copied without admin rights). Then open TrackCraft there via the network name, e.g. `https://trackcraft.home.arpa`, never via `localhost`: on the tablet, `localhost` is the tablet itself.

- Android: Settings → Security → Encryption & credentials → Install a certificate → CA certificate.
- iOS: open the file, install the profile (Settings → Profile Downloaded), then Settings → General → About → Certificate Trust Settings → switch on TrackCraft.

The server certificate is valid for 825 days. `setup` renews it on every update if it expires in less than 30 days or if the names in `allowed_origins` have changed. By hand (administrator):

```powershell
& "C:\Program Files\TrackCraft\trackcraft.exe" cert
```

Change or add names (administrator) without editing the TOML by hand:

```powershell
& "C:\Program Files\TrackCraft\trackcraft.exe" cert --names localhost,trackcraft.home.arpa
```

This writes the names to `allowed_origins` (old version kept as `trackcraft.toml.bak`), issues the certificate and restarts the service. If new names lie outside the previous CA, the command creates a new CA; then install the new `TrackCraft-CA.crt` on phone and tablet and remove the old one there. `--new-ca` forces a new CA.

If the tablet shows “not secure”, the certificate view in the browser names the host the certificate is valid for. If that differs from the address bar, the name is missing in `allowed_origins`: run the command above.

To use your own certificate, enter its paths in `[tls]`; TrackCraft then does not manage it.

## Backup target

TrackCraft backs up daily from the time in `[backup] daily_time` (default 03:00) to the backup target, but only if something changed since the last backup and the target is reachable. If the PC is not running at that time, TrackCraft makes up the attempt once after starting. The **Backup** page shows status, errors and the existing backups and has a “Back up now” button.

Each backup is a folder `trackcraft-<date>-<time>-s<schema>-c<counter>` in the target with `trackcraft.db`, the original files (`originals`), a copy of the configuration and `manifest.json` with size and SHA-256 of every file. TrackCraft verifies each backup in the target after copying; the last five are kept, older ones are removed only after a new success. Backups are not encrypted: protect the target from access by others. A target on the same computer does not protect against losing that computer. Restoring from the interface comes with a later version.

`trackcraft.exe check-config` reports whether the target is reachable.

- USB drive or second disk: path in `[backup] target_dir`, e.g. `'E:\TrackCraft-Backup'`. If the folder exists during `setup`, the service gets write access.
- Network share (e.g. on a NAS): as a UNC path `'\\nas\backup\trackcraft'`, never as a drive letter. The service accesses it as the computer account; a NAS without a domain does not know this account. In that case run the service under a local account that has write access to the share on the NAS with the same name and password (Services → TrackCraft → Log On), and give this account the same rights on `C:\ProgramData\TrackCraft\data` and read access on `config`.

## Operation

| Command | Effect |
| :--- | :--- |
| `trackcraft.exe service status` | state, program path, version, addresses, certificate expiry |
| `trackcraft.exe service stop` / `start` | control the service (administrator) |
| `trackcraft.exe check-config` | check the configuration, changes nothing |
| `trackcraft.exe cert` | check the certificate and renew it if needed (administrator) |
| `trackcraft.exe` | start in the foreground for diagnosis (stop the service first), end with Ctrl+C |

- Logs: `C:\ProgramData\TrackCraft\data\logs\app.<date>.log`. Start errors before the log exists: Event Viewer → Windows Logs → Application, source `TrackCraft`.
- TrackCraft can only be reached while the PC is running.
- After a crash Windows restarts the service after one minute, at most twice a day. With a configuration error it stays stopped.

## Update

1. Download, verify and extract the new ZIP as in section 1. Keep the previous ZIP.
2. Double-click `setup.cmd` in the new folder.

`setup` detects the existing installation, stops the service, replaces the program in the existing installation folder, renews the certificate if needed, checks the configuration and starts the service again. Configuration and data are not touched; before a schema migration TrackCraft makes a full backup in `data\backups` and, if reachable, copies it to the backup target. New required keys are listed in the release notes.

## Rollback

An older program does not start against a newer schema and then changes nothing. To go back:

1. Run `setup.cmd` from the old ZIP you kept; it replaces the program with the old version.
2. If it does not start because the database has already been migrated, restore the database from before the migration (as administrator):
   1. `trackcraft.exe service stop`
   2. In `C:\ProgramData\TrackCraft\data\database`, move the files `trackcraft.db`, `trackcraft.db-wal` and `trackcraft.db-shm` to another folder.
   3. Copy `C:\ProgramData\TrackCraft\data\backups\trackcraft-vor-migration-v<old>-v<new>\trackcraft.db` to `data\database`.
   4. `trackcraft.exe service start`

   Changes since the update are then missing; the original files in `data\activities` stay as they are.

## Uninstall

```powershell
& "C:\Program Files\TrackCraft\trackcraft.exe" uninstall
```

Removes the service, event source, firewall rule and the TrackCraft CA entry in Windows. The installation folder and `C:\ProgramData\TrackCraft` (configuration, certificates, data with training and position data) stay and are only deleted by hand when no longer needed. On the phone, remove the TrackCraft CA in the certificate settings.
