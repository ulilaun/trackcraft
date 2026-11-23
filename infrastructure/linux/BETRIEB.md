# TrackCraft unter Linux betreiben

Gilt für das Paket `trackcraft-<version>-linux-x86_64.tar.gz` (RFC-0005 „Linux“, RFC-0015). TrackCraft läuft als systemd-Dienst `trackcraft` unter einem eigenen Benutzer, liefert die Oberfläche selbst aus und spricht nur HTTPS, wie unter Windows. Erprobt auf Debian 13 in einem Proxmox-Container (LXC).

## Voraussetzungen

- Linux x86-64 mit systemd und glibc 2.35 oder neuer (z. B. Debian 12/13, Ubuntu 22.04 und neuer), mindestens 1 GB freier Arbeitsspeicher.
- Feste IP-Adresse im Heimnetz und ein Name, den der Router auflöst, z. B. `<hostname>.fritz.box` bei einer FRITZ!Box (Hostname des Containers) oder `trackcraft.home.arpa` im Router-DNS.
- Port 443 ist auf dem Rechner frei.

### Proxmox-Container

Unprivilegierter Container aus der Debian-Vorlage, 2 Kerne, 2 GB RAM, Platte nach Datenmenge (Programm etwa 50 MB, dazu Originaldateien und Datenbank). In der FRITZ!Box dem Container eine feste IP geben. `nesting` braucht TrackCraft nicht.

## 1. Herunterladen und prüfen

```bash
sha256sum -c --ignore-missing SHA256SUMS.txt
tar -xzf trackcraft-1.0.0-linux-x86_64.tar.gz
cd trackcraft-1.0.0-linux-x86_64
```

## 2. Installieren

```bash
sudo ./setup.sh
```

`setup` fragt, jeweils mit Vorschlag in eckigen Klammern (Enter übernimmt ihn):

| Frage | Vorschlag | Hinweis |
| :--- | :--- | :--- |
| Namen | `localhost, <hostname -f>` | durch Komma getrennt, z. B. `trackcraft.home.arpa, localhost` |
| Dein Name | `TrackCraft` | dein Profilname in TrackCraft (genau ein Benutzer, kein Login) |
| Sicherungsziel | `/mnt/trackcraft-sicherung` | eingehängte NAS-Freigabe oder zweite Platte, später änderbar |
| Port | `443` | |

Danach legt `setup` an:

| Was | Wo |
| :--- | :--- |
| Programm und Oberfläche | `/opt/trackcraft` |
| Konfiguration | `/etc/trackcraft/trackcraft.toml` (root, für den Dienst lesbar) |
| CA und Zertifikat | `/etc/trackcraft/ca` (nur root), `/etc/trackcraft/tls` |
| Daten, Logs, Migrationssicherung | `/var/lib/trackcraft` (Benutzer `trackcraft`) |
| Dienst | `/etc/systemd/system/trackcraft.service` |

Dann prüft es die Konfiguration, startet den Dienst und wartet, bis er antwortet. Am Ende stehen die Adressen, z. B. `https://trackcraft.home.arpa`.

Mit Angaben statt Rückfragen:

```bash
sudo ./setup.sh --names trackcraft.home.arpa,localhost --display-name Alex --backup-dir /mnt/trackcraft-sicherung
```

Der Dienst läuft als Benutzer `trackcraft` ohne Anmeldung. Er darf nur Port 443 öffnen und nur in `/var/lib/trackcraft` und im Sicherungsziel schreiben; der Rest des Systems ist für ihn schreibgeschützt.

## 3. Zertifikat und Geräte

`setup` legt eine eigene TrackCraft-CA an und stellt damit das Serverzertifikat für die eingerichteten Namen aus. Die CA darf nur diese Namen bestätigen. Auf jedem Gerät, das TrackCraft nutzt (PC, Handy, Tablet), die öffentliche Datei `/etc/trackcraft/TrackCraft-CA.crt` als vertrauenswürdige CA installieren. Vom PC aus holen:

```bash
scp root@trackcraft.home.arpa:/etc/trackcraft/TrackCraft-CA.crt .
```

- Windows: Doppelklick → Zertifikat installieren → Lokaler Computer → „Vertrauenswürdige Stammzertifizierungsstellen“.
- Android: Einstellungen → Sicherheit → Verschlüsselung und Anmeldedaten → Zertifikat installieren → CA-Zertifikat.
- iOS: Datei öffnen, Profil installieren, dann Einstellungen → Allgemein → Info → Zertifikatsvertrauenseinstellungen → TrackCraft einschalten.

Das Serverzertifikat gilt 825 Tage; `setup` erneuert es bei jedem Update, wenn es in weniger als 30 Tagen abläuft oder die Namen geändert wurden. Von Hand bzw. mit neuen Namen:

```bash
sudo /opt/trackcraft/trackcraft cert
sudo /opt/trackcraft/trackcraft cert --names trackcraft.home.arpa,localhost
```

Liegen neue Namen außerhalb der bisherigen CA, legt der Befehl eine neue CA an; dann die neue CA-Datei auf den Geräten installieren und die alte entfernen.

## Sicherungsziel

> **Noch keine tägliche Sicherung:** TrackCraft legt bisher nur vor einer Schemamigration selbst eine Sicherung der Datenbank an (`/var/lib/trackcraft/backups`). Die tägliche Sicherung ins Sicherungsziel und die Wiederherstellung aus der Oberfläche kommen mit einer späteren Version. Bis dahin selbst sichern, z. B.:
>
> ```bash
> sudo systemctl stop trackcraft
> sudo tar -czf /mnt/trackcraft-sicherung/trackcraft-$(date +%F).tgz -C /var/lib/trackcraft database activities
> sudo systemctl start trackcraft
> ```
>
> Proxmox-Nutzer können stattdessen den ganzen Container sichern (vzdump, Modus „Stopp“ oder „Snapshot“).

Eine NAS-Freigabe auf dem Proxmox-Host einhängen und per Bind-Mount in den Container geben, oder im Container direkt einhängen (z. B. CIFS in `/etc/fstab`). Den Pfad in `[backup] target_dir` eintragen und `setup.sh` erneut ausführen: Es macht den Ordner für den Dienst beschreibbar und trägt ihn in die systemd-Unit ein.

## Betrieb

| Befehl | Wirkung |
| :--- | :--- |
| `systemctl status trackcraft` | Zustand |
| `sudo systemctl restart trackcraft` | Neustart, z. B. nach Änderung der Konfiguration |
| `journalctl -u trackcraft -n 100` | Ausgaben des Dienstes, auch Startfehler |
| `sudo -u trackcraft /opt/trackcraft/trackcraft check-config` | Konfiguration prüfen, ändert nichts |

- Logs: `/var/lib/trackcraft/logs/app.<datum>.log` und das Journal.
- Bei einem Absturz startet systemd den Dienst nach fünf Sekunden neu. Bei einem Konfigurationsfehler beendet er sich mit Meldung im Journal.

## Umzug von Windows

Daten und Originaldateien sind plattformunabhängig, die Datenbank speichert nur relative Pfade.

1. Auf dem Linux-Rechner `setup.sh` ausführen und prüfen, dass TrackCraft leer startet; dann `sudo systemctl stop trackcraft`.
2. Unter Windows den Dienst stoppen (Administrator): `trackcraft.exe service stop`.
3. Von `C:\ProgramData\TrackCraft\data` die Ordner `database` und `activities` vollständig nach `/var/lib/trackcraft/` kopieren und die bisherigen Ordner dort ersetzen. Der Ordner ist nur für den Dienst lesbar, also als Administrator kopieren, z. B. `robocopy C:\ProgramData\TrackCraft\data\database C:\trackcraft\umzug\database /E` (ebenso `activities`). Liegt danach eine `trackcraft.db-wal` mit Inhalt daneben, lief der Dienst noch: neu kopieren. Ein Archiv unter Linux mit `tar --no-same-owner -xzf …` entpacken; in einem unprivilegierten Container scheitert `tar` sonst an den Windows-Besitzern.
4. Werte aus der Windows-Konfiguration, die von der Vorlage abweichen (z. B. `[segments.matching]`, Zeitzone, Anzeigename), in `/etc/trackcraft/trackcraft.toml` übernehmen. Pfade und `[server]`/`[tls]` bleiben die von Linux.
5. `sudo ./setup.sh` erneut ausführen: Es setzt die Rechte auf die kopierten Daten und startet den Dienst.
6. Unter Windows `trackcraft.exe uninstall` ausführen, damit der alte Dienst nicht wieder startet.

## Update

1. Neues Paket laden, prüfen und entpacken wie in Abschnitt 1. Das bisherige aufbewahren.
2. `sudo ./setup.sh` aus dem neuen Ordner.

`setup` erkennt die Installation, stoppt den Dienst, ersetzt Programm und Oberfläche, erneuert bei Bedarf das Zertifikat und startet wieder. Konfiguration und Daten bleiben; vor einer Schemamigration legt TrackCraft eine Sicherung in `/var/lib/trackcraft/backups` an.

## Rollback

Ein älteres Programm startet nicht gegen ein neueres Schema und ändert dann nichts. `sudo ./setup.sh` aus dem aufbewahrten alten Paket ausführen. Startet es nicht, weil schon migriert wurde, die Datenbank von vor der Migration zurückholen:

```bash
sudo systemctl stop trackcraft
cd /var/lib/trackcraft
sudo mkdir -p /root/nach-update && sudo mv database/trackcraft.db* /root/nach-update/
sudo cp backups/trackcraft-vor-migration-v<alt>-v<neu>.db database/trackcraft.db
sudo chown trackcraft:trackcraft database/trackcraft.db
sudo systemctl start trackcraft
```

Änderungen seit dem Update fehlen danach; die Originaldateien in `activities` bleiben, wie sie sind.

## Deinstallieren

```bash
sudo systemctl disable --now trackcraft
sudo rm /etc/systemd/system/trackcraft.service && sudo systemctl daemon-reload
sudo rm -r /opt/trackcraft
```

`/etc/trackcraft` und `/var/lib/trackcraft` (Trainings- und Positionsdaten) bleiben stehen und werden nur von Hand gelöscht. Auf den Geräten die TrackCraft-CA entfernen.
