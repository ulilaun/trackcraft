# ADR-0007: Deployment

Status: Akzeptiert, konsolidiert am 2026-10-29 (Piggy 005). Plattformen festgelegt am 08.11.2026.

Zwei Plattformen aus demselben Quellcode (RFC-0005):

- **Windows (Referenzgerät)**: Ulrichs Windows-11-Desktop-PC, x86-64. Ein nativer Prozess `trackcraft.exe` als Windows-Dienst `TrackCraft` unter einem virtuellen Dienstkonto ohne Administratorrechte. Das Backend liefert das Frontend selbst aus und terminiert HTTPS mit rustls, Zertifikat aus TOML. Kein Docker, kein Reverse Proxy, kein Nginx.
- **Docker (zweite Plattform)**: x86-64-Host mit Docker, z. B. Synology mit Container Manager oder Mini-PC. Zwei Appcontainer: Frontend (Nginx/React), Backend (Rust/Axum). Reverse Proxy terminiert HTTPS, interne Containerkommunikation darf HTTP nutzen, kein veröffentlichter Backend-Port.

Verteilung (F2): Releases baut nur GitHub Actions aus einem Tag. Windows erhält ein ZIP mit `setup.cmd`, das aus dem Download-Ordner installiert und aktualisiert, den Installationsordner abfragt und eine eigene, auf die Heimnetz-Namen beschränkte CA für HTTPS anlegt (kein MSI, keine Codesignatur, kein mkcert), Docker die Images aus `ghcr.io/ulilaun`. Updates von Hand, kein Update-Check (RFC-0015).

SQLite liegt auf beiden Plattformen im lokalen Datenbereich des Backends. Kein direkter Internetzugriff, Zugriff nur aus LAN oder VPN.

Datenpfade kommen aus der TOML. Vorgaben: unter Windows `C:\ProgramData\TrackCraft\data\{database,activities,logs,backups}`, im Container `/data/database`, `/data/activities`, `/data/backups`, `/data/logs`. Sicherungsziel (USB-Laufwerk, zweite Platte oder Netzwerkfreigabe) in TOML konfigurieren. Der Migrationsordner nimmt nur die lokale Migrationssicherung auf.

TOML schreibgeschützt, Standardpfad unter Windows `%ProgramData%\TrackCraft\config\trackcraft.toml`, im Container `/config/trackcraft.toml`. Pfadauswahl nur über `TRACKCRAFT_CONFIG`, keine Einzelwert-Umgebungsvariablen. Details RFC-0005 und RFC-0021.
