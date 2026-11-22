# RFC-0005: Deployment und Infrastruktur

Status: Konsolidierter Entwurf, 2026-10-29 (Piggy 007). Plattformen und Referenzgerät festgelegt am 08.11.2026, Linux mit systemd ergänzt am 13.11.2026.

## Zielsysteme

TrackCraft läuft auf drei Plattformen aus demselben Quellcode. Fachlogik, Datenbank, Sicherungsformat und API sind auf beiden identisch, nur Start, Pfade und der Weg zu HTTPS unterscheiden sich.

| | Windows (Referenz) | Linux mit systemd | Docker (zweite Plattform) |
| :--- | :--- | :--- | :--- |
| Host | Windows 11, x86-64, mindestens 2 GB freier Arbeitsspeicher | Linux x86-64 mit systemd und glibc ≥ 2.35, z. B. Debian 13 im Proxmox-Container (LXC), mindestens 1 GB RAM | x86-64-Host mit Docker und Docker Compose v2, mindestens 2 GB RAM, z. B. Synology mit Container Manager oder Mini-PC mit Linux |
| Artefakt | `trackcraft.exe` (Ziel `x86_64-pc-windows-msvc`) plus Frontend-Build | `trackcraft` (Ziel `x86_64-unknown-linux-gnu`) plus Frontend-Build | Images `trackcraft-frontend` und `trackcraft-backend` für `linux/amd64` |
| Start | Windows-Dienst `TrackCraft` | systemd-Dienst `trackcraft` | Compose mit zwei Appcontainern |
| HTTPS | im Backend (rustls), Zertifikat aus TOML | wie Windows | Reverse Proxy vor dem Frontend-Container |
| Frontend | vom Backend ausgeliefert (`[server] static_dir`) | wie Windows | Nginx im Frontend-Container |
| Security-Header | Backend | Backend | Nginx |

**Referenzgerät** ist Ulrichs Windows-Desktop-PC. Abnahme, Messungen und Restoretest für v1.0 finden dort statt (RFC-0014). Die Docker-Plattform wird in CI gebaut und getestet, aber nicht auf eigener Hardware abgenommen. Ulrichs laufende Installation zieht ab 13.11.2026 vom Windows-PC auf die Linux-Plattform um (Debian-Container auf einem Proxmox-Server); ob die Abnahme für v1.0 dorthin wechselt, wird vor M6 entschieden.

Die vorhandene DS214 (Marvell Armada XP, 32-Bit-ARM, 512 MB RAM, DSM 6.2.4) ist als Docker-Host ungeeignet: kein Docker-Paket für diese Architektur, zu wenig Arbeitsspeicher, DSM ohne Sicherheitsupdates. Sie kann als Sicherungsziel über eine Netzwerkfreigabe dienen (RFC-0012).

Keine Performanceziele in V1; Import-, Matching- und Antwortzeiten auf dem Referenzgerät messen und dokumentieren.

Plattformneutraler Code: keine Annahmen über Pfadtrenner, Groß-/Kleinschreibung von Dateinamen, Zeilenenden, offene Dateien oder Signale. Pfade werden nur über die Pfadtypen der Standardbibliothek gebildet. Die IANA-Zeitzonendaten sind im Programm enthalten, weil Windows keine liefert (RFC-0021).

## Windows

### Dienst

- Ein Prozess `trackcraft.exe` läuft als Windows-Dienst mit Dienstname `TrackCraft` (eingefroren, RFC-0021) und Starttyp „Automatisch (verzögerter Start)“. TrackCraft ist damit ohne Anmeldung erreichbar, auch für den Sicherungslauf um 03:00.
- Dienstkonto ist das virtuelle Konto `NT SERVICE\TrackCraft`, kein Administrator und nicht `LocalSystem`. Es hat Lesezugriff auf Programm- und Konfigurationsordner und Schreibzugriff nur auf die Datenordner und das Sicherungsziel (ACL bei der Installation).
- Wiederherstellungsaktion des Dienstes: bei Absturz nach einer Minute neu starten, höchstens zweimal am Tag. Startfehler durch ungültige Konfiguration führen nicht in eine Neustartschleife, sondern beenden den Dienst mit Fehlercode.
- Startfehler, die vor dem Öffnen der Logdateien auftreten, gehen ins Windows-Ereignisprotokoll (Anwendung, Quelle `TrackCraft`), alle anderen Meldungen in die Logdateien (RFC-0013).
- `trackcraft.exe` lässt sich zur Diagnose auch im Vordergrund in einer Konsole starten (gleiche Konfiguration, gleiche Prüfungen). Zwei laufende Instanzen auf denselben Daten verhindert eine Sperrdatei im Datenbankordner.
- Installation, Update, Dienstregistrierung, ACLs, Zertifikat und Firewallregel erledigt `setup.cmd` aus dem entpackten Windows-ZIP (Abschnitt „Installation und Befehle“).

### Ordner

```text
C:\Program Files\TrackCraft\           Programm (trackcraft.exe) und Frontend-Build (web\); Ordner bei setup wählbar
C:\ProgramData\TrackCraft\config\      trackcraft.toml, für das Dienstkonto nur lesbar
  ca\                                  TrackCraft-CA (Zertifikat und Schlüssel), nur Administratoren
  tls\                                 Serverzertifikat und -schlüssel, Dienstkonto nur lesend
C:\ProgramData\TrackCraft\data\
  database\
  activities\
  logs\
  backups\                             nur lokale Migrationssicherung
```

Das sind die Vorgabewerte der Windows-Beispieldatei, alle Datenpfade kommen aus der TOML (RFC-0021). Ohne `TRACKCRAFT_CONFIG` liest TrackCraft unter Windows `%ProgramData%\TrackCraft\config\trackcraft.toml`. Die Variable kann für den Dienst gesetzt werden, ist aber im Normalbetrieb nicht nötig.

Der Datenordner liegt auf einem lokalen NTFS-Laufwerk, nicht auf einer Netzwerkfreigabe (SQLite). Virenscanner dürfen Dateien kurz sperren; Schreib- und Ersetzungsoperationen wiederholen deshalb bei Zugriffsverweigerung einige Male mit kurzer Wartezeit, bevor sie mit Fehler abbrechen. Eine Ausnahme im Virenscanner für den Datenordner ist erlaubt, aber nicht nötig.

### Netz und HTTPS

- Das Backend lauscht auf `[server] listen` (Windows-Beispiel `0.0.0.0:443`) und spricht dort nur HTTPS. Zertifikat und Schlüssel liegen als PEM-Dateien im Konfigurationsordner, Pfade in `[tls] cert_file` und `[tls] key_file`. Unverschlüsseltes HTTP gibt es unter Windows nicht, auch keine Umleitung von Port 80.
- **Eigene Heimnetz-CA** (08.11.2026): `setup` legt einmalig eine TrackCraft-CA an (ECDSA P-256, 10 Jahre) und stellt damit das Serverzertifikat aus (825 Tage, die Grenze von iOS), mit genau den Rechnernamen aus `allowed_origins`. Die CA trägt eine Namensbeschränkung auf diese Namen; mit ihrem Schlüssel lassen sich keine Zertifikate für fremde Seiten ausstellen. `setup` trägt die CA unter Windows (Computer, Vertrauenswürdige Stammzertifizierungsstellen) ein; aufs Handy kommt die öffentliche Kopie `%ProgramData%\TrackCraft\TrackCraft-CA.crt` von Hand (BETRIEB.md). Hauptname (CN) des Serverzertifikats ist der erste Netzname, nicht `localhost`. Kein Zusatzwerkzeug wie mkcert.
- Erneuerung: `setup` und `trackcraft.exe cert` stellen das Serverzertifikat neu aus, wenn es fehlt, in weniger als 30 Tagen abläuft oder die Namen in `allowed_origins` sich geändert haben. Namen ändert `cert --names` ohne Bearbeiten der TOML; Namen außerhalb der Beschränkung führen dabei zu einer neuen CA, die danach auf Handy und Tablet installiert wird. Ein eigenes Zertifikat an anderer Stelle (andere Pfade in `[tls]`) verwaltet TrackCraft nicht.
- Namensauflösung im LAN über einen festen Namen, den der Router kennt, z. B. `<rechnername>.fritz.box` bei einer FRITZ!Box oder `trackcraft.home.arpa` im Router-DNS auf die feste IP-Adresse des PCs.
- Eine eingehende Firewallregel erlaubt nur den TLS-Port, nur für `trackcraft.exe` und nur im Profil „Privat“. Im Profil „Öffentlich“ ist TrackCraft nicht erreichbar.
- Zugriff von unterwegs nur über VPN ins Heimnetz, keine Router-Portfreigabe.

### Betrieb am Desktop-PC

- TrackCraft ist nur erreichbar, wenn der PC läuft. Im Energiesparmodus oder ausgeschaltet zeigt die PWA den Hinweis ohne Serververbindung (RFC-0006).
- Der Sicherungslauf um 03:00 findet nur statt, wenn der PC dann läuft. Sonst wird er nach dem Start einmal nachgeholt (RFC-0012). TrackCraft weckt den PC nicht und verhindert keinen Energiesparmodus.
- Beim Herunterfahren beendet Windows den Dienst. TrackCraft bricht laufende Operationen dabei so ab wie bei einem Absturz: Import, Sicherung und Restore setzen nach dem Neustart nach RFC-0012 und RFC-0008 sicher wieder an.

### Installation und Befehle

Festgelegt am 08.11.2026, Ablauf über `setup.cmd` ebenfalls 08.11.2026. Ausgeliefert wird ein ZIP `trackcraft-<version>-windows-x86_64.zip` aus GitHub Releases (RFC-0015), kein MSI-Installer und keine Codesignatur in V1.

Inhalt des ZIP:

```text
setup.cmd                     Installation und Update, Doppelklick genügt
trackcraft.exe
web\                          Frontend-Build
trackcraft.example.toml       vollständige Windows-Beispieldatei (RFC-0021), Vorlage für die erste Konfiguration
LICENSE                       AGPL-3.0-or-later
THIRD-PARTY-LICENSES.txt      Lizenzen der ausgelieferten Abhängigkeiten
BETRIEB.md                    Installation, Zertifikat, Update, Rollback, Deinstallation
```

**`setup.cmd`** wird aus dem entpackten ZIP gestartet, z. B. im Download-Ordner. Es fordert Administratorrechte an und ruft `trackcraft.exe setup --interactive` mit den übergebenen Optionen auf. Ablauf von `setup`:

1. Prüfen, bevor etwas verändert wird: Administratorrechte, vollständiges ZIP neben der `trackcraft.exe`, Installationsordner absolut und nicht in einem Benutzerordner (dort darf das Dienstkonto nicht lesen).
2. Erkennen: Ist der Dienst schon eingerichtet, ist es ein **Update** in den bisherigen Ordner. Sonst eine **Neuinstallation**.
3. Nur bei Neuinstallation fragen, jeweils mit Vorgabe (Enter übernimmt sie): Installationsordner (`C:\Program Files\TrackCraft`), Namen im Heimnetz (`localhost` und der Rechnername), dein Name als Profil des einen TrackCraft-Benutzers (Vorschlag: Windows-Benutzername, kein Login), Sicherungsziel (`C:\ProgramData\TrackCraft\sicherung`, später auf USB oder NAS ändern), Port (443). Jede Frage entfällt, wenn der Wert als Option übergeben wurde: `--install-dir`, `--names`, `--display-name`, `--backup-dir`, `--port`.
4. Dienst stoppen, falls er läuft. Programm und `web\` in den Installationsordner kopieren (entfällt, wenn `setup.cmd` schon dort liegt).
5. Ordner unter `%ProgramData%\TrackCraft` anlegen.
6. Bei Neuinstallation `trackcraft.toml` aus der Vorlage mit den Antworten erzeugen; Kommentare und alle übrigen Werte der Vorlage bleiben. Eine vorhandene TOML ändert `setup` nie. Ausnahme: Eine TOML mit dem Platzhalter-Anzeigenamen der Vorlage („Beispielname“) gilt als nicht eingerichtet, auch aus einer älteren Fassung der Vorlage, und wird als `trackcraft.toml.beispiel` beiseitegelegt.
7. Zertifikat nach „Netz und HTTPS“, CA in Windows eintragen.
8. Dienst registrieren bzw. auf den Installationsordner einstellen, Ereignisquelle, ACLs, Firewallregel.
9. `check-config`; nur bei gültiger Konfiguration den Dienst starten und die Adressen anzeigen, sonst die Fehler und den nächsten Schritt nennen.

`setup` ist wiederholbar und dient auch als Reparatur.

Befehle von `trackcraft.exe`:

| Befehl | Wirkung | Adminrechte |
| :--- | :--- | :--- |
| ohne Argument | Start im Vordergrund in der Konsole (Diagnose), Abbruch mit Strg+C | nein |
| `setup [Optionen]` | Installation oder Update wie oben; ohne `--interactive` gelten für fehlende Werte die Vorgaben | ja |
| `cert [--names …] [--new-ca]` | Serverzertifikat prüfen und bei Bedarf neu ausstellen, Dienst neu starten. `--names` ersetzt `allowed_origins` in der TOML (alte Fassung als `.bak`) und legt automatisch eine neue CA an, wenn die Namen außerhalb der bisherigen liegen; `--new-ca` erzwingt eine neue CA. Legt eine Kopie des CA-Zertifikats als `%ProgramData%\TrackCraft\TrackCraft-CA.crt` ab (öffentlich, ohne Adminrechte kopierbar) | ja |
| `check-config` | Konfiguration mit denselben Prüfungen wie beim Start prüfen (RFC-0021, inklusive Zertifikat und Pfade), Ergebnis ausgeben, nichts starten und nichts schreiben | nein |
| `service start`, `service stop`, `service status` | Dienst steuern bzw. Zustand, Programmpfad, Version, Adressen und Zertifikatsablauf anzeigen | `status` nein, sonst ja |
| `uninstall` | Dienst, Ereignisquelle, Firewallregel und CA-Vertrauen in Windows entfernen. Programm-, Konfigurations- und Datenordner bleiben stehen; ihr Löschen ist ein bewusster Schritt von Hand, den `BETRIEB.md` beschreibt. | ja |

Erstinstallation: ZIP herunterladen, SHA-256 mit der Prüfsummendatei des Releases vergleichen, in den Dateieigenschaften „Zulassen“ setzen (Download-Markierung, sonst warnt SmartScreen), entpacken, `setup.cmd` doppelklicken.

### Update

1. Neues ZIP herunterladen, prüfen und entpacken wie oben. Das bisherige ZIP aufbewahren (Rollback, RFC-0015).
2. `setup.cmd` aus dem neuen ZIP starten. Es stoppt den Dienst, ersetzt das Programm im bisherigen Installationsordner, erneuert das Zertifikat bei Bedarf, prüft die Konfiguration und startet den Dienst. Konfiguration und Daten bleiben unberührt. Neue Pflichtschlüssel nennen die Release Notes; fehlen sie, startet der Dienst nicht und `setup` nennt sie.
3. Beim Start greift die Migrationsregel unten (Sicherung vor Migration).

TrackCraft sucht nicht selbst nach Updates und lädt nichts nach (RFC-0015).

## Linux

Festgelegt am 13.11.2026 (Uli): nativer systemd-Dienst, gleiche Arbeitsweise wie unter Windows (eigenes HTTPS, eigene CA, Frontend aus `static_dir`), kein Reverse Proxy und kein Docker. Paket `trackcraft-<version>-linux-x86_64.tar.gz` mit `trackcraft`, `web/`, `trackcraft.example.toml` (Linux-Pfade), `setup.sh`, `BETRIEB.md`, Lizenzen. Gebaut auf Ubuntu 22.04, damit glibc 2.35 genügt.

```text
/opt/trackcraft                  Programm und Oberfläche (root)
/etc/trackcraft/trackcraft.toml  Konfiguration (root, Gruppe trackcraft lesbar)
/etc/trackcraft/ca               TrackCraft-CA mit Schlüssel (nur root)
/etc/trackcraft/tls              Serverzertifikat (Gruppe trackcraft lesbar)
/etc/trackcraft/TrackCraft-CA.crt öffentliche Kopie für die Geräte
/var/lib/trackcraft              Datenbank, Originale, Logs, Migrationssicherung (Benutzer trackcraft)
/etc/systemd/system/trackcraft.service
```

- `trackcraft setup` (über `setup.sh`, root) installiert oder aktualisiert wie unter Windows: Programm kopieren, Systembenutzer `trackcraft` ohne Anmeldung, Ordner und Rechte, Konfiguration aus der Vorlage nur bei Neueinrichtung, CA und Zertifikat (`cert.rs`), Unit schreiben, Konfiguration prüfen, Dienst starten und warten, bis der Port antwortet (bis 15 Minuten wegen Neuberechnung). `trackcraft cert [--names …] [--new-ca]` wie unter Windows.
- Die Unit setzt `TRACKCRAFT_CONFIG`, läuft als `trackcraft`, erhält nur `CAP_NET_BIND_SERVICE` für Port 443 und darf nur in `/var/lib/trackcraft` und im Sicherungsziel schreiben (`ProtectSystem=strict`, `ProtectHome`, `PrivateTmp`, `NoNewPrivileges`). Neustart bei Absturz nach fünf Sekunden.
- Ohne `TRACKCRAFT_CONFIG` liest TrackCraft unter Linux `/etc/trackcraft/trackcraft.toml`, wenn vorhanden, sonst `/config/trackcraft.toml` (Container).
- Der Umzug von Windows kopiert `data/database` und `data/activities`; die Datenbank enthält nur relative Pfade. Ablauf in `infrastructure/linux/BETRIEB.md`.

## Docker

Reverse Proxy -> React/Nginx -> Rust/Axum -> SQLite. Zwei Appcontainer `trackcraft-frontend` und `trackcraft-backend` im internen `trackcraft-network`. Diese Namen sind eingefroren (RFC-0021, G1), Imagenamen entsprechend `trackcraft-frontend` und `trackcraft-backend`.

Frontend intern Port 8080 (Nginx ohne root, Image `nginx-unprivileged`), auf dem Host nur `127.0.0.1:8081`; Backend intern 8080; Clients HTTPS 443. „Extern“ bedeutet außerhalb Docker für LAN/VPN, nicht öffentlich im Internet. Backend nicht veröffentlichen; keine Internetroute/Routerfreigabe zu TrackCraft. Das Backend lässt `[server] static_dir` und `[tls]` leer und liefert nur die API über HTTP im internen Netz.

Reverse Proxy: auf einer Synology der eingebaute Reverse Proxy von DSM mit Zertifikaten über DSM. Auf einem Mini-PC übernimmt ein vorgeschalteter Proxy dieselbe Rolle (z. B. ein Caddy- oder Nginx-Container vor dem Frontend), mit denselben Anforderungen an Header und TLS wie unten. TLS-Vertrauen/Erneuerung/Name auf Clients dokumentieren, interne HTTP-Kommunikation erlaubt.

### Proxykette und Browserschutz

Anforderungen aus dem Browserschutz (RFC-0011):

- Die Proxyregel wählt TrackCraft über den Hostnamen der erlaubten Herkunft (`[server] allowed_origins`, RFC-0021) und leitet an den Frontend-Container weiter.
- Der Frontend-Port wird nur auf `127.0.0.1` des Hosts veröffentlicht. TrackCraft ist damit nur über den Reverse Proxy mit TLS erreichbar, nicht direkt per HTTP aus dem LAN.
- Der Reverse Proxy reicht `Host`, `Origin` und `Sec-Fetch-*` unverändert weiter. Für `Host` ist das die übliche DSM-Vorgabe, sie wird bei der Einrichtung geprüft.
- Nginx leitet `/api/` mit `proxy_set_header Host $http_host` an das Backend weiter und entfernt keinen dieser Header.
- Nginx setzt die Security-Header aus RFC-0011 mit `add_header … always`, also auch auf Fehlerantworten. Weder Reverse Proxy noch Nginx fügen CORS-Header hinzu.

### Containerpfade

```text
/config/trackcraft.toml   schreibgeschützt eingebunden
/data/database
/data/activities
/data/logs
/data/backups
```

Ohne `TRACKCRAFT_CONFIG` liest TrackCraft unter Linux `/config/trackcraft.toml`.

### Images und Update

Festgelegt am 08.11.2026. Die Images kommen aus der GitHub Container Registry: `ghcr.io/ulilaun/trackcraft-frontend` und `ghcr.io/ulilaun/trackcraft-backend`, privat wie das Repository. Der Docker-Host meldet sich einmal mit `docker login ghcr.io` und einem Token an, das nur `read:packages` darf. Tags: genaue Version (`1.0.0`) und `stable` für die letzte Stable-Version; Betaversionen nur mit genauer Version. Die Compose-Datei des Betriebspakets nennt immer die genaue Version, nie `stable` oder `latest`, damit ein Update eine bewusste Änderung ist.

Selbst bauen bleibt möglich (`docker compose build` aus einem Checkout des Release-Tags), ist aber kein Releaseweg und nicht abgenommen.

Update: Version in der Compose-Datei ändern, `docker compose pull`, `docker compose up -d`. Beim Start greift die Migrationsregel unten. Rollback: alte Version eintragen und nach RFC-0015 verfahren.

## Daten und Konfiguration (alle Plattformen)

Das Sicherungsziel (USB-Laufwerk, zweite Platte oder Netzwerkfreigabe) wird per TOML konfiguriert, unter Docker separat eingebunden. Unter Windows ist eine Netzwerkfreigabe als UNC-Pfad anzugeben (`\\server\freigabe\trackcraft`), verbundene Laufwerksbuchstaben sieht der Dienst nicht. Das Dienstkonto greift auf Freigaben als Computerkonto zu; ein NAS ohne Domäne braucht deshalb ein eigenes Dienstkonto mit Freigaberechten, das bei der Einrichtung festgelegt wird. Restore wählt Stände in diesem Sicherungsziel, es gibt kein eigenes Restoreverzeichnis. Der Migrationsordner nimmt nur die lokale Migrationssicherung auf (nur die letzte).

TOML einzige Anwendungsquelle, schreibgeschützt. `TRACKCRAFT_CONFIG` wählt nur Datei. Keine Overrides durch Einzelwert-Umgebungsvariablen. Bei fehlender/ungültiger Datei, unbekannten Schlüsseln oder fehlenden Pflichtwerten Startfehler; nur dokumentierte optionale Werte haben Vorgaben. Änderungen nach Neustart, kein Reload.

## Funktionen und Betrieb

Backend: fester Benutzer mit Profilabgleich aus TOML, FIT-/GPX-Import mit Sessionauswahl/Ergänzung fehlender FIT-Sessions, Session-/Dateireferenzen, Matching, Statistiken, Sicherung/Restore und Operationskoordination. Kein App-Login durch Benutzerobjekt.

Initial Logging-Retention aus Vorentwurf 30 Tage, tatsächliches Rotationsverfahren und Konfigurationsfeld noch zu definieren. Health/Readiness nach RFC-0013.

Vor jeder Migration lokale Vollsicherung, Kopie ins Ziel falls erreichbar; scheitert die lokale, bricht der Start ab. Migration/Kompatibilität und sicherer Rollback nach RFC-0015/RFC-0012; nicht blind ein altes Programm oder Image gegen neues Schema starten.

## Backup und Wiederherstellung

Genau eine Vollsicherung in konfiguriertes Verzeichnis; einzige lokale Ausnahme ist die Sicherung vor Schemamigration (nur die letzte bleibt), keine Snapshots in der Definition. Bedingter Tageslauf 03:00 initial in Anwendungszeitzone, manueller Start/Nachholen, fünf unverschlüsselte validierte Stände. Sicherungs-Staging kurze Schreibsperre, Übertragung danach ohne.

Restore nicht allein durch einen Programmstart nachgewiesen: Im gestarteten leeren/gefüllten Backend Auswahl aus konfiguriertem Verzeichnis, Datenstandsvergleich, nötige Bestätigung, sicherer vollständiger Ersatz ohne Mischbestand und Funktionsprüfung. Unter Windows lassen sich geöffnete Dateien nicht ersetzen; der Restore schließt deshalb alle Datenbankverbindungen und Dateihandles, bevor er den neuen Stand einsetzt. Keine Zeitvorgabe. TOML-Referenzkopie nicht automatisch aktivieren.

## Netzverbindungen

Kernfunktionen benötigen LAN/VPN-Verbindung zum TrackCraft-Host, keine Internetdienste. Externe Standardkarte (OSM-Standard, URL in TOML) benötigt zusätzlich Internet. Keine Cloud-Speicherung oder externe Verarbeitung der Trainingsdateien.
