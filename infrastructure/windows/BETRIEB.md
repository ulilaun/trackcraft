# TrackCraft unter Windows betreiben

Gilt für das Windows-ZIP `trackcraft-<version>-windows-x86_64.zip` (RFC-0005, RFC-0015). TrackCraft läuft als Windows-Dienst `TrackCraft`, liefert die Oberfläche selbst aus und spricht nur HTTPS. Zuletzt am 23.11.2026 auf Windows 11 durchgespielt: Neueinrichtung nach `uninstall` mit vorhandener Konfiguration, Migration alter Daten, Import, Update-Lauf.

## Voraussetzungen

- Windows 11, x86-64, mindestens 2 GB freier Arbeitsspeicher.
- Feste IP-Adresse des PCs im Heimnetz und ein Name, den der Router auflöst. Bei einer FRITZ!Box ist das meist `<rechnername>.fritz.box`, sonst ein Eintrag im Router-DNS, z. B. `trackcraft.home.arpa`. Für einen ersten Test am PC selbst reicht `localhost`.
- Das Netzwerkprofil des Heimnetzes steht auf „Privat“ (Einstellungen → Netzwerk und Internet). Im Profil „Öffentlich“ ist TrackCraft nicht erreichbar.

## 1. Herunterladen und prüfen

1. ZIP und `SHA256SUMS.txt` aus dem GitHub Release laden.
2. Prüfsumme vergleichen (PowerShell):

   ```powershell
   Get-FileHash .\trackcraft-1.0.0-windows-x86_64.zip -Algorithm SHA256
   ```

   Der Wert muss mit der Zeile in `SHA256SUMS.txt` übereinstimmen.
3. Rechtsklick auf das ZIP → Eigenschaften → „Zulassen“ ankreuzen. Sonst warnt SmartScreen beim Start, weil TrackCraft nicht signiert ist.
4. Das ZIP entpacken (Rechtsklick → „Alle extrahieren“), wo es liegt, z. B. im Download-Ordner. `setup.cmd` nicht direkt im geöffneten ZIP doppelklicken: Dann fehlen ihm die übrigen Dateien.

### Für Entwickler: lokal gebautes Paket

Zum Ausprobieren eines eigenen Stands baut das Skript ein gleich aufgebautes Paket aus dem Repository (Rust und Node nötig):

```powershell
.\scripts\package-windows.ps1 -Version 0.1.0-dev -Local
```

Ergebnis: `dist\trackcraft-0.1.0-dev-windows-x86_64.zip` und daneben derselbe Inhalt entpackt, darin `setup.cmd`.

## 2. Installieren

Doppelklick auf `setup.cmd` im entpackten Ordner. Es fordert Administratorrechte an (Windows fragt nach) und fragt dann, jeweils mit Vorschlag in eckigen Klammern; Enter übernimmt ihn:

| Frage | Vorschlag | Hinweis |
| :--- | :--- | :--- |
| Installationsordner | `C:\Program Files\TrackCraft` | nicht in einem Benutzerordner |
| Namen | `localhost, <rechnername>` | durch Komma getrennt, z. B. `pc.home.arpa, localhost` |
| Dein Name | Windows-Benutzername | dein Profilname in TrackCraft (genau ein Benutzer, kein Login) |
| Sicherungsziel | `C:\ProgramData\TrackCraft\sicherung` | besser USB-Laufwerk oder NAS, später änderbar |
| Port | `443` | |

Danach kopiert `setup` das Programm in den Installationsordner, legt Konfiguration und Datenordner unter `C:\ProgramData\TrackCraft` an, erzeugt das HTTPS-Zertifikat (Abschnitt 3), richtet Dienst, Zugriffsrechte, Ereignisquelle und Firewallregel ein, prüft die Konfiguration und startet den Dienst. Am Ende stehen die Adressen, z. B. `https://pc.home.arpa`.

Mit Angaben statt Rückfragen, z. B. für einen anderen Installationsordner:

```powershell
.\setup.cmd --install-dir "D:\Programme\TrackCraft" --names pc.home.arpa,localhost --display-name Alex --backup-dir E:\TrackCraft-Sicherung
```

Jede übergebene Angabe ersetzt die Frage, fehlende werden weiter abgefragt. Die Konfiguration liegt danach in `C:\ProgramData\TrackCraft\config\trackcraft.toml` und kann dort angepasst werden; Änderungen wirken nach einem Neustart des Dienstes.

Aufruf im Browser nur über einen der eingerichteten Namen. Ein Aufruf über die IP-Adresse wird abgewiesen.

## 3. Zertifikat und Handy

`setup` legt eine eigene TrackCraft-CA an und stellt damit das Serverzertifikat für die eingerichteten Namen aus. Die CA darf nur diese Namen bestätigen. Am PC trägt `setup` sie selbst als vertrauenswürdig ein; der Browser am PC zeigt also keine Warnung.

Aufs Handy oder Tablet kommt die Datei `C:\ProgramData\TrackCraft\TrackCraft-CA.crt` (per Mail, Cloud-Ordner oder USB; ohne Adminrechte kopierbar). Dort TrackCraft dann über den Netznamen aufrufen, z. B. `https://trackcraft.home.arpa`, nie über `localhost`: `localhost` ist auf dem Tablet das Tablet selbst.

- Android: Einstellungen → Sicherheit → Verschlüsselung und Anmeldedaten → Zertifikat installieren → CA-Zertifikat.
- iOS: Datei öffnen, Profil installieren (Einstellungen → Profil geladen), dann Einstellungen → Allgemein → Info → Zertifikatsvertrauenseinstellungen → TrackCraft einschalten.

Das Serverzertifikat gilt 825 Tage. `setup` erneuert es bei jedem Update, wenn es in weniger als 30 Tagen abläuft oder die Namen in `allowed_origins` geändert wurden. Von Hand (Administrator):

```powershell
& "C:\Program Files\TrackCraft\trackcraft.exe" cert
```

Namen ändern oder ergänzen (Administrator), ohne die TOML von Hand zu bearbeiten:

```powershell
& "C:\Program Files\TrackCraft\trackcraft.exe" cert --names localhost,trackcraft.home.arpa
```

Das übernimmt die Namen in `allowed_origins` (alte Fassung als `trackcraft.toml.bak`), stellt das Zertifikat aus und startet den Dienst neu. Liegen neue Namen außerhalb der bisherigen CA, legt der Befehl eine neue CA an; dann die neue `TrackCraft-CA.crt` auf Handy und Tablet installieren und die alte dort entfernen. `--new-ca` erzwingt eine neue CA.

Zeigt das Tablet „nicht sicher“, nennt die Zertifikatsanzeige im Browser den Namen, für den das Zertifikat gilt. Steht dort ein anderer Name als in der Adresszeile, fehlt der Name in `allowed_origins`: Befehl oben ausführen.

Wer ein eigenes Zertifikat nutzen will, trägt dessen Pfade in `[tls]` ein; TrackCraft verwaltet es dann nicht.

## Sicherungsziel

> **Noch keine tägliche Sicherung:** TrackCraft legt bisher nur vor einer Schemamigration selbst eine Sicherung der Datenbank an (`data\backups`). Die tägliche Sicherung ins Sicherungsziel und die Wiederherstellung aus der Oberfläche kommen mit einer späteren Version. Bis dahin selbst sichern: Dienst stoppen, `C:\ProgramData\TrackCraft\data\database` und `data\activities` als Administrator kopieren (z. B. mit `robocopy … /E`), Dienst starten.

Das Ziel lässt sich trotzdem schon eintragen; `trackcraft.exe check-config` meldet, ob es erreichbar ist.

- USB-Laufwerk oder zweite Platte: Pfad in `[backup] target_dir`, z. B. `'E:\TrackCraft-Sicherung'`. Ist der Ordner beim `setup` vorhanden, erhält der Dienst Schreibrecht.
- Netzwerkfreigabe (z. B. auf einem NAS): als UNC-Pfad `'\\nas\backup\trackcraft'`, nie als Laufwerksbuchstabe. Der Dienst greift als Computerkonto zu; ein NAS ohne Domäne kennt dieses Konto nicht. Dann den Dienst unter einem lokalen Konto laufen lassen, das auf dem NAS mit gleichem Namen und Passwort Schreibrecht auf die Freigabe hat (Dienste → TrackCraft → Anmelden), und diesem Konto dieselben Rechte auf `C:\ProgramData\TrackCraft\data` und Lesen auf `config` geben.

## Betrieb

| Befehl | Wirkung |
| :--- | :--- |
| `trackcraft.exe service status` | Zustand, Programmpfad, Version, Adressen, Zertifikatsablauf |
| `trackcraft.exe service stop` / `start` | Dienst steuern (Administrator) |
| `trackcraft.exe check-config` | Konfiguration prüfen, ändert nichts |
| `trackcraft.exe cert` | Zertifikat prüfen und bei Bedarf erneuern (Administrator) |
| `trackcraft.exe` | Start im Vordergrund zur Diagnose (vorher Dienst stoppen), Ende mit Strg+C |

- Logs: `C:\ProgramData\TrackCraft\data\logs\app.<datum>.log`. Startfehler vor dem Log: Ereignisanzeige → Windows-Protokolle → Anwendung, Quelle `TrackCraft`.
- TrackCraft ist nur erreichbar, wenn der PC läuft.
- Bei einem Absturz startet Windows den Dienst nach einer Minute neu, höchstens zweimal am Tag. Bei einem Konfigurationsfehler bleibt er gestoppt.

## Update

1. Neues ZIP herunterladen, prüfen und entpacken wie in Abschnitt 1. Das bisherige ZIP aufbewahren.
2. `setup.cmd` aus dem neuen Ordner doppelklicken.

`setup` erkennt die vorhandene Installation, stoppt den Dienst, ersetzt das Programm im bisherigen Installationsordner, erneuert das Zertifikat bei Bedarf, prüft die Konfiguration und startet den Dienst wieder. Konfiguration und Daten bleiben unberührt; vor einer Schemamigration legt TrackCraft selbst eine Sicherung in `data\backups` an. Neue Pflichtschlüssel nennen die Release Notes.

## Rollback

Ein älteres Programm startet nicht gegen ein neueres Schema und ändert dann nichts. Zurück geht es so:

1. `setup.cmd` aus dem aufbewahrten alten ZIP ausführen; es ersetzt das Programm durch die alte Version.
2. Startet sie nicht, weil schon migriert wurde, die Datenbank von vor der Migration zurückholen (als Administrator):
   1. `trackcraft.exe service stop`
   2. In `C:\ProgramData\TrackCraft\data\database` die Dateien `trackcraft.db`, `trackcraft.db-wal` und `trackcraft.db-shm` in einen anderen Ordner verschieben.
   3. `C:\ProgramData\TrackCraft\data\backups\trackcraft-vor-migration-v<alt>-v<neu>.db` als `trackcraft.db` nach `data\database` kopieren.
   4. `trackcraft.exe service start`

   Änderungen seit dem Update fehlen danach; die Originaldateien in `data\activities` bleiben, wie sie sind.

## Deinstallieren

```powershell
& "C:\Program Files\TrackCraft\trackcraft.exe" uninstall
```

Entfernt Dienst, Ereignisquelle, Firewallregel und den Eintrag der TrackCraft-CA in Windows. Der Installationsordner und `C:\ProgramData\TrackCraft` (Konfiguration, Zertifikate, Daten mit Trainings- und Positionsdaten) bleiben stehen und werden nur von Hand gelöscht, wenn sie nicht mehr gebraucht werden. Auf dem Handy die TrackCraft-CA in den Zertifikatseinstellungen entfernen.
