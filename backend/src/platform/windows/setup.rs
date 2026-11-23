//! Einrichtung unter Windows (RFC-0005, F2): `setup` (Installation und Update), `cert`, `uninstall`, `service`.
//!
//! `setup` läuft aus dem entpackten ZIP (z. B. im Download-Ordner), kopiert das Programm in den
//! Installationsordner und richtet alles Weitere ein. Ist TrackCraft schon installiert, ist es ein Update.

use std::ffi::OsString;
use std::io::{BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use windows_service::service::{
    Service, ServiceAccess, ServiceAction, ServiceActionType, ServiceErrorControl, ServiceFailureActions, ServiceFailureResetPeriod,
    ServiceInfo, ServiceSidType, ServiceStartType, ServiceState, ServiceType,
};
use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};

use super::SERVICE_NAME;
use crate::cert::{self, Managed};
use crate::config::{self, Validated, normalize_for_compare};
use crate::install::{self, Answers};

const ACCOUNT: &str = r"NT SERVICE\TrackCraft";
const FIREWALL_RULE: &str = "TrackCraft";
const SID_SYSTEM: &str = "*S-1-5-18";
const SID_ADMINS: &str = "*S-1-5-32-544";
/// Dateien des ZIP, die in den Installationsordner gehören (neben `web\`).
const PAYLOAD: &[&str] = &["trackcraft.exe", "trackcraft.example.toml", "setup.cmd", "BETRIEB.md", "LICENSE", "THIRD-PARTY-LICENSES.txt"];

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
    base: PathBuf,
    config_dir: PathBuf,
    data_dir: PathBuf,
    config_file: PathBuf,
}

impl Layout {
    fn new() -> Self {
        let base =
            std::env::var_os("ProgramData").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\ProgramData")).join("TrackCraft");
        Self {
            config_dir: base.join("config"),
            data_dir: base.join("data"),
            config_file: base.join("config").join("trackcraft.toml"),
            base,
        }
    }
}

fn program_dir() -> anyhow::Result<PathBuf> {
    let exe = std::env::current_exe()?;
    Ok(exe.parent().context("Programmordner unbekannt")?.to_path_buf())
}

fn default_install_dir() -> PathBuf {
    std::env::var_os("ProgramFiles").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Program Files")).join("TrackCraft")
}

fn manager(access: ServiceManagerAccess) -> anyhow::Result<ServiceManager> {
    ServiceManager::local_computer(None::<&str>, access)
        .map_err(|e| anyhow::anyhow!("Dienststeuerung nicht erreichbar ({e}). Bitte als Administrator ausführen (setup.cmd fragt danach)."))
}

fn run(cmd: &mut Command) -> anyhow::Result<()> {
    let out = cmd.output().with_context(|| format!("{cmd:?} lässt sich nicht starten"))?;
    if !out.status.success() {
        bail!(
            "{:?} fehlgeschlagen: {}{}",
            cmd.get_program(),
            String::from_utf8_lossy(&out.stdout).trim(),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(())
}

fn icacls_restrict(path: &Path, inherit: bool, service_right: Option<&str>) -> anyhow::Result<()> {
    let flags = if inherit { "(OI)(CI)" } else { "" };
    let mut args =
        vec!["/inheritance:r".to_string(), "/grant:r".into(), format!("{SID_SYSTEM}:{flags}F"), format!("{SID_ADMINS}:{flags}F")];
    if let Some(r) = service_right {
        args.push(format!("{ACCOUNT}:{flags}{r}"));
    }
    args.push("/Q".into());
    run(Command::new("icacls").arg(path).args(&args))
}

fn icacls_grant(path: &Path, right: &str) -> anyhow::Result<()> {
    run(Command::new("icacls").arg(path).args(["/grant", &format!("{ACCOUNT}:{right}"), "/Q"]))
}

fn under_users(path: &Path) -> bool {
    let users = PathBuf::from(format!(r"{}\Users", std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into())));
    normalize_for_compare(path).starts_with(&normalize_for_compare(&users))
}

/// Eingabe mit Vorgabe; ohne Konsole oder ohne `interactive` gilt die Vorgabe.
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

/// Installationsordner eines vorhandenen Dienstes (für Updates).
fn installed_dir(mgr: &ServiceManager) -> Option<PathBuf> {
    let service = mgr.open_service(SERVICE_NAME, ServiceAccess::QUERY_CONFIG).ok()?;
    let config = service.query_config().ok()?;
    install::exe_dir_from_service_path(&config.executable_path.to_string_lossy())
}

/// Konfiguration mit dem Platzhalter der Beispieldatei gilt als noch nicht eingerichtet.
fn is_unconfigured(config_file: &Path) -> bool {
    std::fs::read_to_string(config_file).map(|t| install::is_unconfigured(&t)).unwrap_or(false)
}

/// Hebt den Schreibschutz einer vorhandenen Datei auf; fehlt sie, passiert nichts.
fn clear_readonly(path: &Path) {
    if let Ok(meta) = std::fs::metadata(path) {
        let mut perm = meta.permissions();
        if perm.readonly() {
            #[allow(clippy::permissions_set_readonly_false)] // nur Windows: entfernt das Attribut „Schreibgeschützt“
            perm.set_readonly(false);
            let _ = std::fs::set_permissions(path, perm);
        }
    }
}

fn copy_payload(source: &Path, target: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(target).with_context(|| format!("{} lässt sich nicht anlegen", target.display()))?;
    for name in PAYLOAD {
        let from = source.join(name);
        if from.is_file() {
            let to = target.join(name);
            // Kopieren übernimmt Dateiattribute: Eine schreibgeschützte Quelle (z. B. direkt aus dem ZIP gestartet)
            // machte das Ziel schreibgeschützt, und das nächste Update scheiterte mit „Zugriff verweigert“.
            clear_readonly(&to);
            std::fs::copy(&from, &to).with_context(|| format!("{name} lässt sich nicht nach {} kopieren", target.display()))?;
            clear_readonly(&to);
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
            clear_readonly(&dest);
        }
    }
    Ok(())
}

fn stop_if_running(service: &Service) -> anyhow::Result<bool> {
    let st = service.query_status()?;
    if st.current_state == ServiceState::Stopped {
        return Ok(false);
    }
    // Beim Start (Neuberechnung vor „Running“) nimmt der Dienst kein Stopp an (Fehler 1052). Dann den Prozess beenden:
    // SQLite mit WAL und Transaktionen bleibt heil, ein halb berechneter Stand wird beim nächsten Start verworfen.
    if st.current_state == ServiceState::StartPending
        && let Some(pid) = st.process_id
    {
        println!("Der Dienst startet noch (Neuberechnung); er wird beendet.");
        let ok = std::process::Command::new("taskkill").args(["/F", "/PID", &pid.to_string()]).status().is_ok_and(|s| s.success());
        if !ok {
            bail!("Dienst im Start ließ sich nicht beenden; als Administrator `taskkill /F /IM trackcraft.exe`, dann setup.cmd erneut");
        }
        wait_for(service, ServiceState::Stopped, Duration::from_secs(60))?;
        return Ok(true);
    }
    service.stop()?;
    wait_for(service, ServiceState::Stopped, Duration::from_secs(60))?;
    Ok(true)
}

/// `trackcraft.exe setup` (über `setup.cmd`): Installation oder Update.
pub fn setup(args: SetupArgs) -> anyhow::Result<()> {
    let mgr = manager(ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE)?;
    let source = program_dir()?;
    let layout = Layout::new();
    let example_src = source.join("trackcraft.example.toml");

    // Voraussetzungen prüfen, bevor irgendetwas verändert wird.
    if !example_src.is_file() || !source.join("web").join("index.html").is_file() {
        bail!(
            "Im Ordner {} fehlen trackcraft.example.toml oder web\\index.html. setup.cmd muss aus dem entpackten \
             TrackCraft-ZIP gestartet werden (lokal bauen: scripts\\package-windows.ps1 -Local). Es wurde nichts verändert.",
            source.display()
        );
    }
    // Ein Dienst, der in einen Benutzerordner zeigt, kann nie gelaufen sein; dann neu wählen.
    let existing = installed_dir(&mgr).filter(|d| !under_users(d));
    let fresh = !layout.config_file.exists() || is_unconfigured(&layout.config_file);

    println!("TrackCraft {} – {}", crate::version::APP_VERSION, if existing.is_some() && !fresh { "Update" } else { "Einrichtung" });
    if let Some(dir) = &existing {
        println!("Installiert in {}", dir.display());
    }
    println!();

    let install_dir = match (&args.install_dir, &existing) {
        (Some(d), _) => d.clone(),
        (None, Some(d)) => d.clone(),
        (None, None) => {
            if args.interactive {
                explain(&[
                    "Installationsordner",
                    "Hierhin kommt das Programm. Konfiguration und Daten liegen getrennt davon unter C:\\ProgramData\\TrackCraft.",
                    "Zu jeder Frage steht ein Vorschlag in eckigen Klammern; Enter übernimmt ihn.",
                ]);
            }
            PathBuf::from(ask(args.interactive, "Installationsordner", &default_install_dir().to_string_lossy()))
        }
    };
    if !install_dir.is_absolute() {
        bail!("Installationsordner {} ist kein absoluter Pfad", install_dir.display());
    }
    if under_users(&install_dir) {
        bail!(
            "Installationsordner {} liegt in einem Benutzerordner. Der Dienst darf dort nicht lesen; bitte z. B. {} wählen.",
            install_dir.display(),
            default_install_dir().display()
        );
    }

    let answers = if fresh { Some(collect_answers(&args, &install_dir, &layout)?) } else { None };
    if !fresh && (!args.names.is_empty() || args.display_name.is_some() || args.backup_dir.is_some() || args.port.is_some()) {
        println!(
            "Hinweis: Die Konfiguration besteht schon und bleibt unverändert; Werte bitte in {} ändern.",
            layout.config_file.display()
        );
    }

    // Ab hier wird verändert.
    let access = ServiceAccess::CHANGE_CONFIG
        | ServiceAccess::START
        | ServiceAccess::STOP
        | ServiceAccess::QUERY_STATUS
        | ServiceAccess::QUERY_CONFIG;
    if let Ok(service) = mgr.open_service(SERVICE_NAME, access)
        && stop_if_running(&service)?
    {
        println!("Dienst gestoppt");
    }

    println!("1. Programm nach {}", install_dir.display());
    if normalize_for_compare(&install_dir) == normalize_for_compare(&source) {
        println!("   läuft bereits von dort, nichts zu kopieren");
    } else {
        copy_payload(&source, &install_dir)?;
    }
    let exe = install_dir.join("trackcraft.exe");

    println!("2. Ordner unter {}", layout.base.display());
    for dir in [
        layout.config_dir.clone(),
        layout.data_dir.join("database"),
        layout.data_dir.join("activities"),
        layout.data_dir.join("logs"),
        layout.data_dir.join("backups"),
    ] {
        std::fs::create_dir_all(&dir).with_context(|| format!("{} lässt sich nicht anlegen", dir.display()))?;
    }

    println!("3. Konfiguration {}", layout.config_file.display());
    if let Some(a) = &answers {
        if layout.config_file.exists() {
            let backup = layout.config_file.with_extension("toml.beispiel");
            std::fs::rename(&layout.config_file, &backup)?;
            println!("   noch nicht eingerichtete Beispielkonfiguration beiseitegelegt: {}", backup.display());
        }
        if !super::is_unc(&a.backup_dir) {
            let _ = std::fs::create_dir_all(&a.backup_dir);
        }
        let text = install::render(&std::fs::read_to_string(install_dir.join("trackcraft.example.toml"))?, a)?;
        std::fs::write(&layout.config_file, text)?;
        println!("   angelegt");
    } else {
        println!("   vorhanden, bleibt unverändert");
    }
    let cfg = config::load(&layout.config_file);

    println!("4. Zertifikat");
    if let Ok(cfg) = &cfg {
        apply_certificates(cfg, &layout, false)?;
    } else {
        println!("   übersprungen, Konfiguration ist ungültig (siehe unten)");
    }

    println!("5. Dienst {SERVICE_NAME} ({ACCOUNT})");
    let service = register_service(&mgr, &exe, access)?;

    println!("6. Ereignisquelle {SERVICE_NAME}");
    register_event_source()?;

    println!("7. Zugriffsrechte");
    icacls_grant(&layout.config_dir, "(OI)(CI)RX")?;
    icacls_restrict(&layout.data_dir, true, Some("M"))?;
    secure_certificates(&layout)?;
    if let Ok(cfg) = &cfg {
        let target = &cfg.raw.backup.target_dir;
        if target.is_dir() && !super::is_unc(target) {
            icacls_grant(target, "(OI)(CI)M")?;
            println!("   Sicherungsziel {} für den Dienst beschreibbar", target.display());
        } else if super::is_unc(target) {
            println!("   Sicherungsziel ist eine Netzwerkfreigabe: Zugriff für den Dienst dort einrichten (BETRIEB.md)");
        }
    }

    let port = cfg.as_ref().map(|c| c.listen.port()).unwrap_or(443);
    println!("8. Firewallregel {FIREWALL_RULE} (TCP {port}, nur Profil Privat)");
    let _ = Command::new("netsh").args(["advfirewall", "firewall", "delete", "rule", &format!("name={FIREWALL_RULE}")]).output();
    run(Command::new("netsh").args([
        "advfirewall",
        "firewall",
        "add",
        "rule",
        &format!("name={FIREWALL_RULE}"),
        "dir=in",
        "action=allow",
        &format!("program={}", exe.display()),
        "protocol=TCP",
        &format!("localport={port}"),
        "profile=private",
    ]))?;

    println!("9. Konfiguration prüfen");
    if !crate::commands::check_config_report(&layout.config_file) {
        println!("\nDer Dienst wurde nicht gestartet. Konfiguration anpassen, dann setup.cmd erneut ausführen.");
        return Ok(());
    }
    service.start::<&str>(&[])?;
    wait_for(&service, ServiceState::Running, START_TIMEOUT)?;
    println!("\nTrackCraft läuft.");
    if let Ok(cfg) = &cfg {
        for o in &cfg.origins {
            println!("  {}", o.origin);
        }
    }
    let public = public_ca_path(&layout);
    if public.is_file() {
        println!("\nFür Handy und Tablet: {} als CA-Zertifikat installieren (BETRIEB.md).", public.display());
        println!("Dort die Adresse mit dem Netznamen aufrufen, nicht localhost.");
    }
    Ok(())
}

fn collect_answers(args: &SetupArgs, install_dir: &Path, layout: &Layout) -> anyhow::Result<Answers> {
    let i = args.interactive;
    let pc = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "pc".into()).to_ascii_lowercase();
    let default_names = if args.names.is_empty() { format!("localhost, {pc}") } else { args.names.join(", ") };
    if i && args.names.is_empty() {
        explain(&[
            "Adressen im Heimnetz",
            "Die Namen, unter denen du TrackCraft im Browser aufrufst, durch Komma getrennt.",
            &format!("Fürs Handy einen Namen, den dein Router kennt, z. B. {pc}.fritz.box bei einer FRITZ!Box."),
            "localhost funktioniert nur am PC selbst. Das Zertifikat gilt genau für diese Namen.",
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

    let user = std::env::var("USERNAME").unwrap_or_else(|_| "TrackCraft".into());
    let display_name = match &args.display_name {
        Some(n) => n.clone(),
        None => {
            if i {
                explain(&[
                    "Dein Name in TrackCraft",
                    "TrackCraft hat genau einen Benutzer: dich. Dieser Name erscheint in der Oberfläche als dein Profil.",
                    "Er ist kein Login und hat nichts mit der Windows-Anmeldung zu tun; vorgeschlagen ist nur dein Windows-Benutzername.",
                ]);
            }
            ask(i, "Dein Name", &user)
        }
    };

    let default_backup = layout.base.join("sicherung");
    let backup_dir = match &args.backup_dir {
        Some(d) => d.clone(),
        None => {
            if i {
                explain(&[
                    "Sicherungsziel",
                    "Ordner für die täglichen Sicherungen. Am besten ein USB-Laufwerk (z. B. E:\\TrackCraft-Sicherung)",
                    "oder ein NAS-Ordner (\\\\server\\freigabe\\trackcraft). Der Vorschlag liegt auf diesem PC und schützt",
                    "nicht vor dessen Verlust; später in der Konfiguration änderbar.",
                ]);
            }
            PathBuf::from(ask(i, "Sicherungsziel", &default_backup.to_string_lossy()))
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
    let is_managed = normalize_for_compare(&tls.cert_file) == normalize_for_compare(&managed.server_cert)
        && normalize_for_compare(&tls.key_file) == normalize_for_compare(&managed.server_key);
    if !is_managed {
        println!("   eigenes Zertifikat ({}), wird nicht verwaltet", tls.cert_file.display());
        return Ok(());
    }
    let label = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "PC".into());
    let out = cert::ensure(&managed, &install::hosts(&cfg.origins), new_ca, &label)?;
    if out.ca_created {
        let _ = Command::new("certutil").args(["-delstore", "Root", &format!("TrackCraft Heimnetz-CA ({label})")]).output();
        run(Command::new("certutil").args(["-addstore", "-f", "Root"]).arg(&managed.ca_cert))?;
        println!("   neue TrackCraft-CA angelegt und in Windows als vertrauenswürdig eingetragen");
    }
    match out.server_issued {
        Some(reason) => println!("   Serverzertifikat ausgestellt ({reason}), gültig {} Tage", cert::SERVER_DAYS),
        None => println!("   Serverzertifikat aktuell"),
    }
    secure_certificates(layout)?;
    // Das CA-Zertifikat ist öffentlich; eine Kopie außerhalb des geschützten ca\-Ordners lässt sich ohne
    // Adminrechte auf Handy und Tablet bringen. Der CA-Schlüssel bleibt geschützt.
    let public = public_ca_path(layout);
    std::fs::copy(&managed.ca_cert, &public).with_context(|| format!("{} lässt sich nicht schreiben", public.display()))?;
    Ok(())
}

/// Öffentliche Kopie des CA-Zertifikats für Handy und Tablet.
fn public_ca_path(layout: &Layout) -> PathBuf {
    layout.base.join("TrackCraft-CA.crt")
}

/// CA-Schlüssel nur für Administratoren, Serverschlüssel zusätzlich für den Dienst lesbar.
fn secure_certificates(layout: &Layout) -> anyhow::Result<()> {
    let managed = Managed::in_config_dir(&layout.config_dir);
    if let Some(ca_dir) = managed.ca_key.parent().filter(|d| d.is_dir()) {
        icacls_restrict(ca_dir, true, None)?;
    }
    if let Some(tls_dir) = managed.server_key.parent().filter(|d| d.is_dir()) {
        icacls_restrict(tls_dir, true, Some("R"))?;
    }
    Ok(())
}

fn register_service(mgr: &ServiceManager, exe: &Path, access: ServiceAccess) -> anyhow::Result<Service> {
    let info = ServiceInfo {
        name: OsString::from(SERVICE_NAME),
        display_name: OsString::from("TrackCraft"),
        service_type: ServiceType::OWN_PROCESS,
        start_type: ServiceStartType::AutoStart,
        error_control: ServiceErrorControl::Normal,
        executable_path: exe.to_path_buf(),
        launch_arguments: vec![OsString::from("service-run")],
        dependencies: vec![],
        account_name: Some(OsString::from(ACCOUNT)),
        account_password: None,
    };
    let service = match mgr.open_service(SERVICE_NAME, access) {
        Ok(s) => {
            s.change_config(&info)?;
            println!("   vorhanden, auf {} eingestellt", exe.display());
            s
        }
        Err(_) => {
            let s = mgr.create_service(&info, access)?;
            println!("   angelegt");
            s
        }
    };
    service.set_description("TrackCraft: FIT-/GPX-Trainingsdaten im Heimnetz")?;
    service.set_delayed_auto_start(true)?;
    service.set_config_service_sid_info(ServiceSidType::Unrestricted)?;
    // Bei Absturz nach einer Minute neu starten, höchstens zweimal am Tag; Startfehler (Exit-Code) ohne Neustart.
    service.update_failure_actions(ServiceFailureActions {
        reset_period: ServiceFailureResetPeriod::After(Duration::from_secs(86_400)),
        reboot_msg: None,
        command: None,
        actions: Some(vec![
            ServiceAction { action_type: ServiceActionType::Restart, delay: Duration::from_secs(60) },
            ServiceAction { action_type: ServiceActionType::Restart, delay: Duration::from_secs(60) },
            ServiceAction { action_type: ServiceActionType::None, delay: Duration::ZERO },
        ]),
    })?;
    service.set_failure_actions_on_non_crash_failures(false)?;
    Ok(service)
}

/// `trackcraft.exe cert [--new-ca]`: Zertifikat erneuern bzw. für geänderte Namen neu ausstellen.
pub fn cert_command(mut new_ca: bool, names: Vec<String>) -> anyhow::Result<()> {
    let mgr = manager(ServiceManagerAccess::CONNECT)?;
    let layout = Layout::new();
    let path = config::config_path();
    let mut cfg = config::load(&path)?;

    if !names.is_empty() {
        // Namen in die TOML übernehmen; nur allowed_origins ändert sich, die alte Fassung bleibt als .bak.
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
                println!("Danach die neue CA-Datei auf Handy und Tablet installieren und die alte dort entfernen.");
                new_ca = true;
            }
        }
        cfg = checked;
    }

    apply_certificates(&cfg, &layout, new_ca)?;
    if let Ok(service) = mgr.open_service(SERVICE_NAME, ServiceAccess::START | ServiceAccess::STOP | ServiceAccess::QUERY_STATUS)
        && stop_if_running(&service)?
    {
        service.start::<&str>(&[])?;
        wait_for(&service, ServiceState::Running, START_TIMEOUT)?;
        println!("Dienst neu gestartet, das Zertifikat ist aktiv.");
    }
    println!("CA-Zertifikat für Handy und Tablet: {}", public_ca_path(&layout).display());
    Ok(())
}

fn register_event_source() -> anyhow::Result<()> {
    let key = format!(r"HKLM\SYSTEM\CurrentControlSet\Services\EventLog\Application\{SERVICE_NAME}");
    // Die .NET-Nachrichtendatei zeigt beliebige Texte ohne eigene Ressource an; fehlt sie, bleibt der Text trotzdem lesbar.
    let windir = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
    let msg = format!(r"{windir}\Microsoft.NET\Framework64\v4.0.30319\EventLogMessages.dll");
    run(Command::new("reg").args(["add", &key, "/f"]))?;
    if Path::new(&msg).exists() {
        run(Command::new("reg").args(["add", &key, "/v", "EventMessageFile", "/t", "REG_EXPAND_SZ", "/d", &msg, "/f"]))?;
    }
    Ok(())
}

/// Start des Dienstes: Nach einem Update mit neuen Rechenparametern berechnet er vor „Running“ alle Versuche neu
/// (RFC-0007), das kann bei vielen Aktivitäten Minuten dauern.
const START_TIMEOUT: Duration = Duration::from_secs(900);

fn wait_for(service: &Service, target: ServiceState, timeout: Duration) -> anyhow::Result<()> {
    let start = Instant::now();
    let mut told = false;
    loop {
        let st = service.query_status()?;
        if st.current_state == target {
            return Ok(());
        }
        if st.current_state == ServiceState::Stopped && target == ServiceState::Running {
            bail!("Dienst hat sich wieder beendet. Ursache im Ereignisprotokoll (Anwendung, Quelle TrackCraft) oder im Log.");
        }
        if !told && target == ServiceState::Running && start.elapsed() > Duration::from_secs(15) {
            println!("Der Dienst startet noch; nach einem Update werden dabei alle Segmentversuche neu berechnet …");
            told = true;
        }
        if start.elapsed() > timeout {
            bail!("Dienst hat nach {} s den Zustand {target:?} nicht erreicht", timeout.as_secs());
        }
        std::thread::sleep(Duration::from_millis(300));
    }
}

/// `trackcraft.exe uninstall`: entfernt Dienst, Ereignisquelle, Firewallregel und das CA-Vertrauen; Daten bleiben.
pub fn uninstall() -> anyhow::Result<()> {
    let mgr = manager(ServiceManagerAccess::CONNECT)?;
    match mgr.open_service(SERVICE_NAME, ServiceAccess::STOP | ServiceAccess::DELETE | ServiceAccess::QUERY_STATUS) {
        Ok(service) => {
            stop_if_running(&service)?;
            service.delete()?;
            println!("Dienst {SERVICE_NAME} entfernt");
        }
        Err(_) => println!("Dienst {SERVICE_NAME} ist nicht eingerichtet"),
    }
    let _ = Command::new("netsh").args(["advfirewall", "firewall", "delete", "rule", &format!("name={FIREWALL_RULE}")]).output();
    let _ = Command::new("reg")
        .args(["delete", &format!(r"HKLM\SYSTEM\CurrentControlSet\Services\EventLog\Application\{SERVICE_NAME}"), "/f"])
        .output();
    let label = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "PC".into());
    let _ = Command::new("certutil").args(["-delstore", "Root", &format!("TrackCraft Heimnetz-CA ({label})")]).output();
    println!("Firewallregel, Ereignisquelle und CA-Vertrauen entfernt.");
    println!("Programm-, Konfigurations- und Datenordner bleiben stehen ({}), siehe BETRIEB.md.", Layout::new().base.display());
    Ok(())
}

pub enum ServiceCommand {
    Start,
    Stop,
    Status,
}

pub fn service_command(cmd: ServiceCommand) -> anyhow::Result<()> {
    let mgr = manager(ServiceManagerAccess::CONNECT)?;
    let service_access = match cmd {
        ServiceCommand::Start => ServiceAccess::START | ServiceAccess::QUERY_STATUS,
        ServiceCommand::Stop => ServiceAccess::STOP | ServiceAccess::QUERY_STATUS,
        ServiceCommand::Status => ServiceAccess::QUERY_STATUS | ServiceAccess::QUERY_CONFIG,
    };
    let service = mgr
        .open_service(SERVICE_NAME, service_access)
        .map_err(|e| anyhow::anyhow!("Dienst {SERVICE_NAME} nicht eingerichtet oder kein Zugriff ({e}); setup.cmd ausführen"))?;
    match cmd {
        ServiceCommand::Start => {
            service.start::<&str>(&[])?;
            wait_for(&service, ServiceState::Running, START_TIMEOUT)?;
            println!("Dienst läuft.");
        }
        ServiceCommand::Stop => {
            stop_if_running(&service)?;
            println!("Dienst gestoppt.");
        }
        ServiceCommand::Status => {
            let st = service.query_status()?;
            println!("Dienst:    {SERVICE_NAME}, {:?}", st.current_state);
            if let Ok(c) = service.query_config() {
                println!("Programm:  {}", c.executable_path.display());
            }
            println!("Version:   {}", crate::version::long());
            match config::load(&config::config_path()) {
                Ok(cfg) => {
                    println!("Lauscht:   {}", cfg.listen);
                    for o in &cfg.origins {
                        println!("Adresse:   {}", o.origin);
                    }
                    if let Some(tls) = &cfg.raw.tls
                        && let Ok(info) =
                            std::fs::read_to_string(&tls.cert_file).map_err(anyhow::Error::from).and_then(|p| cert::inspect(&p))
                    {
                        println!("Zertifikat gültig bis {}", info.not_after.date());
                    }
                }
                Err(e) => println!("Konfiguration: {e}"),
            }
        }
    }
    Ok(())
}
