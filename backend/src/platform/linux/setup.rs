//! Einrichtung unter Linux (RFC-0005 „Linux“): `setup` (Installation und Update) und `cert`.
//!
//! `setup` läuft als root aus dem entpackten Paket, kopiert das Programm nach `/opt/trackcraft`, legt den
//! Dienstbenutzer, die Ordner, die Konfiguration, CA und Zertifikat sowie die systemd-Unit an und startet
//! den Dienst. Ist TrackCraft schon eingerichtet, ist es ein Update: Konfiguration und Daten bleiben.

use std::io::{BufRead, IsTerminal, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use anyhow::{Context, bail};

use super::SERVICE_NAME;
use crate::cert::{self, Managed};
use crate::config::{self, Validated};
use crate::install::{self, Answers};

/// Dateien des Pakets, die in den Installationsordner gehören (neben `web/`).
const PAYLOAD: &[&str] =
    &["trackcraft", "trackcraft.example.toml", "setup.sh", "BETRIEB.md", "BETRIEB.en.md", "LICENSE", "THIRD-PARTY-LICENSES.txt"];
const DEFAULT_INSTALL_DIR: &str = "/opt/trackcraft";
const CONFIG_DIR: &str = "/etc/trackcraft";
const DATA_DIR: &str = "/var/lib/trackcraft";
const UNIT_FILE: &str = "/etc/systemd/system/trackcraft.service";
/// So lange darf der erste Start dauern (Neuberechnung nach Update, vgl. Windows).
const START_TIMEOUT: Duration = Duration::from_secs(15 * 60);

/// Werte für `setup`; fehlende werden bei `interactive` abgefragt, sonst gelten die Vorgaben.
#[derive(Debug, Default)]
pub struct SetupArgs {
    pub install_dir: Option<PathBuf>,
    pub names: Vec<String>,
    pub display_name: Option<String>,
    pub backup_dir: Option<PathBuf>,
    pub port: Option<u16>,
    pub interactive: bool,
}

struct Layout {
    config_dir: PathBuf,
    data_dir: PathBuf,
    config_file: PathBuf,
}

impl Layout {
    fn new() -> Self {
        let config_dir = PathBuf::from(CONFIG_DIR);
        Self { config_file: config_dir.join("trackcraft.toml"), config_dir, data_dir: PathBuf::from(DATA_DIR) }
    }
}

fn run(cmd: &mut Command) -> anyhow::Result<()> {
    let out = cmd.output().with_context(|| format!("{cmd:?} lässt sich nicht starten"))?;
    if !out.status.success() {
        bail!("{cmd:?} ist fehlgeschlagen: {}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    }
    Ok(())
}

fn output(cmd: &mut Command) -> Option<String> {
    let out = cmd.output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn is_root() -> bool {
    output(Command::new("id").arg("-u")).as_deref() == Some("0")
}

/// Eingabe mit Vorgabe; ohne Terminal oder ohne `interactive` gilt die Vorgabe.
fn ask(interactive: bool, label: &str, default: &str) -> String {
    if !interactive || !std::io::stdin().is_terminal() {
        return default.to_string();
    }
    print!("{label} [{default}]: ");
    let _ = std::io::stdout().flush();
    let mut line = String::new();
    if std::io::stdin().lock().read_line(&mut line).is_err() {
        return default.to_string();
    }
    let line = line.trim();
    if line.is_empty() { default.to_string() } else { line.to_string() }
}

/// Überschrift und Erklärung vor einer Frage; die erste Zeile ist die Überschrift.
fn explain(lines: &[&str]) {
    if !std::io::stdin().is_terminal() {
        return;
    }
    println!();
    if let Some((title, rest)) = lines.split_first() {
        println!("{title}");
        for l in rest {
            println!("  {l}");
        }
    }
}

fn is_unconfigured(config_file: &Path) -> bool {
    std::fs::read_to_string(config_file).map(|t| install::is_unconfigured(&t)).unwrap_or(false)
}

/// Installationsordner aus `ExecStart=` einer vorhandenen Unit (für Updates).
fn installed_dir() -> Option<PathBuf> {
    let text = std::fs::read_to_string(UNIT_FILE).ok()?;
    exec_dir(&text)
}

fn exec_dir(unit: &str) -> Option<PathBuf> {
    let exec = unit.lines().find_map(|l| l.trim().strip_prefix("ExecStart="))?;
    let exe = exec.split_whitespace().next()?;
    Path::new(exe).parent().map(Path::to_path_buf)
}

fn copy_payload(source: &Path, target: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(target).with_context(|| format!("{} lässt sich nicht anlegen", target.display()))?;
    for name in PAYLOAD {
        let from = source.join(name);
        if from.is_file() {
            // Erst daneben schreiben, dann umbenennen: ein laufendes Programm lässt sich so ersetzen.
            let tmp = target.join(format!(".{name}.neu"));
            std::fs::copy(&from, &tmp).with_context(|| format!("{name} lässt sich nicht nach {} kopieren", target.display()))?;
            std::fs::rename(&tmp, target.join(name))?;
        }
    }
    let web = target.join("web");
    if web.exists() {
        std::fs::remove_dir_all(&web).with_context(|| format!("{} lässt sich nicht ersetzen", web.display()))?;
    }
    copy_dir(&source.join("web"), &web)
}

fn copy_dir(from: &Path, to: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let dest = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &dest)?;
        } else {
            std::fs::copy(entry.path(), &dest)?;
        }
    }
    Ok(())
}

fn chmod(path: &Path, mode: u32) -> anyhow::Result<()> {
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).with_context(|| format!("Rechte für {}", path.display()))
}

/// Eigentümer setzen; `recursive` für Datenordner.
fn chown(path: &Path, owner: &str, recursive: bool) -> anyhow::Result<()> {
    let mut cmd = Command::new("chown");
    if recursive {
        cmd.arg("-R");
    }
    run(cmd.arg(owner).arg(path))
}

fn ensure_user() -> anyhow::Result<bool> {
    if output(Command::new("id").args(["-u", SERVICE_NAME])).is_some() {
        return Ok(false);
    }
    run(Command::new("useradd").args([
        "--system",
        "--user-group",
        "--home-dir",
        DATA_DIR,
        "--no-create-home",
        "--shell",
        "/usr/sbin/nologin",
        SERVICE_NAME,
    ]))?;
    Ok(true)
}

fn systemctl(args: &[&str]) -> anyhow::Result<()> {
    run(Command::new("systemctl").args(args))
}

fn is_active() -> bool {
    output(Command::new("systemctl").args(["is-active", SERVICE_NAME])).as_deref() == Some("active")
}

/// systemd-Unit: eigener Benutzer, nur Port-Bindung unter 1024 als Recht, Dateisystem bis auf Daten und
/// Sicherungsziel schreibgeschützt.
pub fn unit_text(exe: &Path, config_file: &Path, backup_dir: &Path) -> String {
    format!(
        "# Von `trackcraft setup` angelegt (RFC-0005 „Linux“); Änderungen gehen beim nächsten Update verloren.\n\
         [Unit]\n\
         Description=TrackCraft\n\
         After=network-online.target\n\
         Wants=network-online.target\n\
         \n\
         [Service]\n\
         Type=simple\n\
         User={SERVICE_NAME}\n\
         Group={SERVICE_NAME}\n\
         Environment=TRACKCRAFT_CONFIG={config}\n\
         ExecStart={exe}\n\
         Restart=on-failure\n\
         RestartSec=5\n\
         TimeoutStopSec=60\n\
         UMask=0027\n\
         AmbientCapabilities=CAP_NET_BIND_SERVICE\n\
         CapabilityBoundingSet=CAP_NET_BIND_SERVICE\n\
         NoNewPrivileges=true\n\
         ProtectSystem=strict\n\
         ProtectHome=true\n\
         PrivateTmp=true\n\
         RestrictSUIDSGID=true\n\
         LockPersonality=true\n\
         ReadWritePaths={DATA_DIR}\n\
         ReadWritePaths=-{backup}\n\
         \n\
         [Install]\n\
         WantedBy=multi-user.target\n",
        config = config_file.display(),
        exe = exe.display(),
        backup = backup_dir.display(),
    )
}

pub fn setup(args: SetupArgs) -> anyhow::Result<()> {
    if !is_root() {
        bail!("setup braucht root-Rechte: sudo ./setup.sh oder als root ausführen. Es wurde nichts verändert.");
    }
    let source = std::env::current_exe()?.parent().map(Path::to_path_buf).context("Programmordner unbekannt")?;
    let layout = Layout::new();
    if !source.join("trackcraft.example.toml").is_file() || !source.join("web").join("index.html").is_file() {
        bail!(
            "Im Ordner {} fehlen trackcraft.example.toml oder web/index.html. setup muss aus dem entpackten \
             TrackCraft-Paket gestartet werden. Es wurde nichts verändert.",
            source.display()
        );
    }
    let existing = installed_dir();
    let fresh = !layout.config_file.exists() || is_unconfigured(&layout.config_file);
    println!("TrackCraft {} – {}", crate::version::APP_VERSION, if existing.is_some() && !fresh { "Update" } else { "Einrichtung" });
    if let Some(dir) = &existing {
        println!("Installiert in {}", dir.display());
    }
    println!();

    let install_dir = match (&args.install_dir, &existing) {
        (Some(d), _) => d.clone(),
        (None, Some(d)) => d.clone(),
        (None, None) => PathBuf::from(DEFAULT_INSTALL_DIR),
    };
    if !install_dir.is_absolute() {
        bail!("Installationsordner {} ist kein absoluter Pfad", install_dir.display());
    }
    let answers = if fresh { Some(collect_answers(&args, &install_dir, &layout)?) } else { None };
    if !fresh && (!args.names.is_empty() || args.display_name.is_some() || args.backup_dir.is_some() || args.port.is_some()) {
        println!(
            "Hinweis: Die Konfiguration besteht schon und bleibt unverändert; Werte bitte in {} ändern.",
            layout.config_file.display()
        );
    }

    // Ab hier wird verändert.
    if is_active() {
        systemctl(&["stop", SERVICE_NAME])?;
        println!("Dienst gestoppt");
    }

    println!("1. Programm nach {}", install_dir.display());
    if std::fs::canonicalize(&install_dir).ok() == std::fs::canonicalize(&source).ok() {
        println!("   läuft bereits von dort, nichts zu kopieren");
    } else {
        copy_payload(&source, &install_dir)?;
    }
    let exe = install_dir.join("trackcraft");
    chmod(&exe, 0o755)?;

    println!("2. Dienstbenutzer {SERVICE_NAME}");
    println!("   {}", if ensure_user()? { "angelegt" } else { "vorhanden" });
    let owner = format!("{SERVICE_NAME}:{SERVICE_NAME}");
    let group = format!("root:{SERVICE_NAME}");

    println!("3. Ordner {} und {}", layout.config_dir.display(), layout.data_dir.display());
    std::fs::create_dir_all(&layout.config_dir)?;
    chown(&layout.config_dir, &group, false)?;
    chmod(&layout.config_dir, 0o750)?;
    for sub in ["database", "activities", "logs", "backups"] {
        std::fs::create_dir_all(layout.data_dir.join(sub))?;
    }
    // Auch nach einem Datenumzug gehören alle Daten dem Dienst.
    chown(&layout.data_dir, &owner, true)?;
    chmod(&layout.data_dir, 0o750)?;

    println!("4. Konfiguration {}", layout.config_file.display());
    if let Some(a) = &answers {
        if layout.config_file.exists() {
            let backup = layout.config_file.with_extension("toml.beispiel");
            std::fs::rename(&layout.config_file, &backup)?;
            println!("   noch nicht eingerichtete Beispielkonfiguration beiseitegelegt: {}", backup.display());
        }
        let text = install::render(&std::fs::read_to_string(source.join("trackcraft.example.toml"))?, a)?;
        std::fs::write(&layout.config_file, text)?;
        println!("   angelegt");
    } else {
        println!("   vorhanden, bleibt unverändert");
    }
    chown(&layout.config_file, &group, false)?;
    chmod(&layout.config_file, 0o640)?;
    let cfg = config::load(&layout.config_file);
    // Geheimnisse (#10): Der Dienst darf /etc/trackcraft nicht beschreiben, deshalb legt das Setup die Datei an.
    let key_file = cfg.as_ref().map(crate::secrets::key_path).unwrap_or_else(|_| layout.config_dir.join("secrets.key"));
    let created = crate::secrets::ensure_key_file(&key_file)?;
    chown(&key_file, &group, false)?;
    chmod(&key_file, 0o640)?;
    println!("   Schlüsseldatei für Geheimnisse {}: {}", key_file.display(), if created { "angelegt" } else { "vorhanden" });

    println!("5. Zertifikat");
    match &cfg {
        Ok(cfg) => apply_certificates(cfg, &layout, false)?,
        Err(_) => println!("   übersprungen, Konfiguration ist ungültig (siehe unten)"),
    }

    let backup_dir = cfg.as_ref().map(|c| c.raw.backup.target_dir.clone()).unwrap_or_else(|_| PathBuf::from("/mnt/trackcraft-sicherung"));
    if let Ok(cfg) = &cfg {
        let target = &cfg.raw.backup.target_dir;
        if std::fs::create_dir_all(target).is_ok() && chown(target, &owner, false).is_ok() {
            println!("   Sicherungsziel {} für den Dienst beschreibbar", target.display());
        } else {
            println!("   Sicherungsziel {} nicht erreichbar, später einrichten (BETRIEB.md)", target.display());
        }
    }

    println!("6. systemd-Unit {UNIT_FILE}");
    std::fs::write(UNIT_FILE, unit_text(&exe, &layout.config_file, &backup_dir))?;
    systemctl(&["daemon-reload"])?;
    systemctl(&["enable", SERVICE_NAME])?;

    println!("7. Konfiguration prüfen");
    if !crate::commands::check_config_report(&layout.config_file) {
        println!("\nDer Dienst wurde nicht gestartet. Konfiguration anpassen, dann setup erneut ausführen.");
        return Ok(());
    }
    // Prüfung lief als root und kann Ordner angelegt haben
    chown(&layout.data_dir, &owner, true)?;
    systemctl(&["start", SERVICE_NAME])?;
    wait_until_listening(cfg.as_ref().map(|c| c.listen.port()).unwrap_or(443))?;
    println!("\nTrackCraft läuft.");
    if let Ok(cfg) = &cfg {
        for o in &cfg.origins {
            println!("  {}", o.origin);
        }
    }
    let public = public_ca_path(&layout);
    if public.is_file() {
        println!("\nFür PC, Handy und Tablet: {} als CA-Zertifikat installieren (BETRIEB.md).", public.display());
    }
    println!("Status: systemctl status {SERVICE_NAME} · Protokoll: journalctl -u {SERVICE_NAME}");
    Ok(())
}

/// Wartet, bis der Port offen ist; beim ersten Start nach einem Update kann die Neuberechnung dauern.
fn wait_until_listening(port: u16) -> anyhow::Result<()> {
    let start = Instant::now();
    let mut told = false;
    let restarts =
        || output(Command::new("systemctl").args(["show", "-p", "NRestarts", "--value", SERVICE_NAME])).and_then(|n| n.parse::<u32>().ok());
    // Erst nach RestartSec (5 s) kann ein automatischer Neustart dazukommen, der Wert direkt nach dem Start ist die Basis.
    let restarts_before = restarts().unwrap_or(0);
    loop {
        if std::net::TcpStream::connect_timeout(&([127, 0, 0, 1], port).into(), Duration::from_secs(1)).is_ok() {
            return Ok(());
        }
        // Restart=on-failure: Ein Startfehler (z. B. neueres Schema nach einem Rollback) endet nie in „failed“,
        // systemd startet den Dienst alle paar Sekunden neu. Schon der erste Neustart zeigt den Fehler.
        let failed = output(Command::new("systemctl").args(["is-failed", SERVICE_NAME])).as_deref() == Some("failed");
        let restarting = restarts().is_some_and(|n| n > restarts_before);
        if failed || restarting {
            bail!("Der Dienst ist beim Start abgebrochen. Ursache: journalctl -u {SERVICE_NAME} -n 50");
        }
        if start.elapsed() > START_TIMEOUT {
            bail!(
                "Der Dienst läuft nach {} Minuten noch nicht. journalctl -u {SERVICE_NAME} zeigt den Stand.",
                START_TIMEOUT.as_secs() / 60
            );
        }
        if !told && start.elapsed() > Duration::from_secs(10) {
            println!("   Start dauert, vermutlich Neuberechnung der Auswertung …");
            told = true;
        }
        std::thread::sleep(Duration::from_secs(1));
    }
}

fn collect_answers(args: &SetupArgs, install_dir: &Path, layout: &Layout) -> anyhow::Result<Answers> {
    let i = args.interactive;
    let host = output(Command::new("hostname").arg("-f"))
        .filter(|h| !h.is_empty())
        .or_else(|| output(&mut Command::new("hostname")))
        .unwrap_or_else(|| "trackcraft".into())
        .to_ascii_lowercase();
    let default_names = if args.names.is_empty() { format!("localhost, {host}") } else { args.names.join(", ") };
    if i && args.names.is_empty() {
        explain(&[
            "Adressen im Heimnetz",
            "Die Namen, unter denen du TrackCraft im Browser aufrufst, durch Komma getrennt.",
            "Ein Name, den dein Router kennt, z. B. <hostname>.fritz.box bei einer FRITZ!Box oder trackcraft.home.arpa.",
            "localhost funktioniert nur auf diesem Rechner. Das Zertifikat gilt genau für diese Namen.",
        ]);
    }
    let names: Vec<String> = ask(i && args.names.is_empty(), "Namen", &default_names)
        .split([',', ';', ' '])
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .collect();
    if names.is_empty() {
        bail!("Mindestens ein Name ist nötig");
    }
    let display_name = match &args.display_name {
        Some(n) => n.clone(),
        None => {
            if i {
                explain(&[
                    "Dein Name in TrackCraft",
                    "TrackCraft hat genau einen Benutzer: dich. Dieser Name erscheint in der Oberfläche als dein Profil.",
                ]);
            }
            ask(i, "Dein Name", "TrackCraft")
        }
    };
    let backup_dir = match &args.backup_dir {
        Some(d) => d.clone(),
        None => {
            if i {
                explain(&[
                    "Sicherungsziel",
                    "Ordner für die täglichen Sicherungen, am besten eine eingehängte NAS-Freigabe oder eine zweite Platte.",
                    "Später in der Konfiguration änderbar.",
                ]);
            }
            PathBuf::from(ask(i, "Sicherungsziel", "/mnt/trackcraft-sicherung"))
        }
    };
    let port = match args.port {
        Some(p) => p,
        None => {
            if i {
                explain(&["HTTPS-Port", "443 ist der Standard; dann braucht die Adresse im Browser keine Portangabe."]);
            }
            ask(i, "Port", "443").parse::<u16>().context("Port ist keine Zahl von 1 bis 65535")?
        }
    };
    if i {
        println!();
    }
    let managed = Managed::in_config_dir(&layout.config_dir);
    Ok(Answers {
        display_name,
        names,
        port,
        static_dir: install_dir.join("web"),
        cert_file: managed.server_cert,
        key_file: managed.server_key,
        backup_dir,
    })
}

/// Erzeugt bzw. erneuert CA und Serverzertifikat, wenn die TOML auf die von TrackCraft verwalteten Dateien zeigt.
fn apply_certificates(cfg: &Validated, layout: &Layout, new_ca: bool) -> anyhow::Result<()> {
    let managed = Managed::in_config_dir(&layout.config_dir);
    let Some(tls) = &cfg.raw.tls else {
        println!("   kein [tls] in der Konfiguration");
        return Ok(());
    };
    if tls.cert_file != managed.server_cert || tls.key_file != managed.server_key {
        println!("   eigenes Zertifikat ({}), wird nicht verwaltet", tls.cert_file.display());
        return Ok(());
    }
    let label = output(&mut Command::new("hostname")).unwrap_or_else(|| "Server".into());
    let out = cert::ensure(&managed, &install::hosts(&cfg.origins), new_ca, &label)?;
    if out.ca_created {
        println!("   neue TrackCraft-CA angelegt");
    }
    match out.server_issued {
        Some(reason) => println!("   Serverzertifikat ausgestellt ({reason}), gültig {} Tage", cert::SERVER_DAYS),
        None => println!("   Serverzertifikat aktuell"),
    }
    secure_certificates(&managed)?;
    let public = public_ca_path(layout);
    std::fs::copy(&managed.ca_cert, &public).with_context(|| format!("{} lässt sich nicht schreiben", public.display()))?;
    chmod(&public, 0o644)?;
    Ok(())
}

/// Öffentliche Kopie des CA-Zertifikats für Geräte.
fn public_ca_path(layout: &Layout) -> PathBuf {
    layout.config_dir.join("TrackCraft-CA.crt")
}

/// CA-Schlüssel nur für root, Serverschlüssel zusätzlich für den Dienst lesbar.
fn secure_certificates(managed: &Managed) -> anyhow::Result<()> {
    if let Some(ca_dir) = managed.ca_key.parent().filter(|d| d.is_dir()) {
        chown(ca_dir, "root:root", true)?;
        chmod(ca_dir, 0o700)?;
        chmod(&managed.ca_key, 0o600)?;
    }
    if let Some(tls_dir) = managed.server_key.parent().filter(|d| d.is_dir()) {
        let group = format!("root:{SERVICE_NAME}");
        chown(tls_dir, &group, true)?;
        chmod(tls_dir, 0o750)?;
        chmod(&managed.server_key, 0o640)?;
        chmod(&managed.server_cert, 0o644)?;
    }
    Ok(())
}

pub fn cert_command(mut new_ca: bool, names: Vec<String>) -> anyhow::Result<()> {
    if !is_root() {
        bail!("cert braucht root-Rechte. Es wurde nichts verändert.");
    }
    let layout = Layout::new();
    let path = config::config_path();
    let mut cfg = config::load(&path)?;
    if !names.is_empty() {
        let names: Vec<String> = names.iter().map(|n| n.trim().to_ascii_lowercase()).filter(|n| !n.is_empty()).collect();
        let text = std::fs::read_to_string(&path)?;
        let updated = install::set_origins(&text, &names, cfg.listen.port())?;
        let checked = config::parse(&path, &updated)?;
        std::fs::write(path.with_extension("toml.bak"), &text)?;
        std::fs::write(&path, &updated)?;
        println!("Adressen in {} übernommen:", path.display());
        for o in &checked.origins {
            println!("  {}", o.origin);
        }
        let managed = Managed::in_config_dir(&layout.config_dir);
        if !new_ca && managed.ca_cert.is_file() {
            let ca = std::fs::read_to_string(&managed.ca_cert)?;
            if install::hosts(&checked.origins).iter().any(|h| !cert::ca_permits(&ca, h).unwrap_or(false)) {
                println!("Neue Namen liegen außerhalb der bisherigen TrackCraft-CA: Es wird eine neue CA angelegt.");
                println!("Danach die neue CA-Datei auf den Geräten installieren und die alte dort entfernen.");
                new_ca = true;
            }
        }
        cfg = checked;
    }
    apply_certificates(&cfg, &layout, new_ca)?;
    if is_active() {
        systemctl(&["restart", SERVICE_NAME])?;
        println!("Dienst neu gestartet, das Zertifikat ist aktiv.");
    }
    println!("CA-Zertifikat für die Geräte: {}", public_ca_path(&layout).display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_runs_as_service_user_with_writable_data_only() {
        let u = unit_text(Path::new("/opt/trackcraft/trackcraft"), Path::new("/etc/trackcraft/trackcraft.toml"), Path::new("/mnt/s"));
        assert!(u.contains("ExecStart=/opt/trackcraft/trackcraft\n"));
        assert!(u.contains("User=trackcraft\n"));
        assert!(u.contains("Environment=TRACKCRAFT_CONFIG=/etc/trackcraft/trackcraft.toml\n"));
        assert!(u.contains("ReadWritePaths=/var/lib/trackcraft\nReadWritePaths=-/mnt/s\n"));
        assert_eq!(exec_dir(&u), Some(PathBuf::from("/opt/trackcraft")));
    }
}
