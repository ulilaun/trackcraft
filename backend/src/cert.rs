//! Eigene Heimnetz-CA und Serverzertifikat (RFC-0005, RFC-0011): TrackCraft braucht kein Zusatzwerkzeug.
//!
//! Die CA ist per Namensbeschränkung auf die eingerichteten Namen begrenzt. Selbst wenn ihr Schlüssel
//! abhandenkommt, lassen sich damit keine Zertifikate für fremde Seiten ausstellen.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use rcgen::{
    BasicConstraints, CertificateParams, DistinguishedName, DnType, ExtendedKeyUsagePurpose, GeneralSubtree, IsCa, Issuer, KeyPair,
    KeyUsagePurpose, NameConstraints, SanType,
};
use time::{Duration, OffsetDateTime};
use x509_parser::extensions::{GeneralName, ParsedExtension};
use x509_parser::prelude::{FromDer, X509Certificate};

/// Gültigkeit des Serverzertifikats; iOS akzeptiert höchstens 825 Tage.
pub const SERVER_DAYS: i64 = 825;
/// Gültigkeit der CA.
pub const CA_DAYS: i64 = 3650;
/// Ab so vielen Resttagen stellt `setup` bzw. `cert` neu aus (wie die Warnung `certificate_expiring`).
pub const RENEW_BEFORE_DAYS: i64 = 30;

/// Von TrackCraft verwaltete Dateien unter dem Konfigurationsordner.
#[derive(Debug, Clone)]
pub struct Managed {
    pub ca_cert: PathBuf,
    pub ca_key: PathBuf,
    pub server_cert: PathBuf,
    pub server_key: PathBuf,
}

impl Managed {
    pub fn in_config_dir(config_dir: &Path) -> Self {
        Self {
            ca_cert: config_dir.join("ca").join("TrackCraft-CA.crt"),
            ca_key: config_dir.join("ca").join("TrackCraft-CA-key.pem"),
            server_cert: config_dir.join("tls").join("server.pem"),
            server_key: config_dir.join("tls").join("server-key.pem"),
        }
    }
}

/// Was beim Abgleich passiert ist.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Outcome {
    pub ca_created: bool,
    pub server_issued: Option<String>,
}

/// Legt die CA bei Bedarf an und stellt das Serverzertifikat neu aus, wenn es fehlt, bald abläuft
/// oder nicht genau die gewünschten Namen trägt. `new_ca` ersetzt die CA (neue Namen außerhalb der Beschränkung).
pub fn ensure(files: &Managed, names: &[String], new_ca: bool, ca_label: &str) -> anyhow::Result<Outcome> {
    let names = normalize(names)?;
    let mut out = Outcome::default();

    if new_ca || !files.ca_cert.is_file() || !files.ca_key.is_file() {
        create_ca(files, &names, ca_label)?;
        out.ca_created = true;
    } else {
        let ca_pem = fs::read_to_string(&files.ca_cert)?;
        let outside: Vec<&String> = names.iter().filter(|n| !ca_permits(&ca_pem, n).unwrap_or(false)).collect();
        if !outside.is_empty() {
            bail!(
                "Die vorhandene TrackCraft-CA ist auf andere Namen beschränkt und darf {} nicht bestätigen. \
                 Mit `trackcraft cert --new-ca` eine neue CA anlegen und sie danach auf den Geräten neu installieren.",
                outside.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
            );
        }
    }

    let reason = if out.ca_created {
        Some("neue CA".to_string())
    } else if !files.server_cert.is_file() || !files.server_key.is_file() {
        Some("noch keins vorhanden".to_string())
    } else {
        let pem = fs::read_to_string(&files.server_cert)?;
        let info = inspect(&pem)?;
        if info.not_after - OffsetDateTime::now_utc() < Duration::days(RENEW_BEFORE_DAYS) {
            Some(format!("läuft am {} ab", info.not_after.date()))
        } else if sorted(info.dns_names) != sorted(names.clone()) {
            Some("Namen geändert".to_string())
        } else if !info.has_authority_key_id {
            // Ältere Fassungen stellten ohne AKI aus; strenge Prüfer (z. B. Python ab 3.13) lehnen das ab.
            Some("fehlende Angabe der ausstellenden CA ergänzt".to_string())
        } else {
            None
        }
    };
    if let Some(reason) = reason {
        issue_server(files, &names)?;
        out.server_issued = Some(reason);
    }
    Ok(out)
}

fn normalize(names: &[String]) -> anyhow::Result<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    for n in names {
        let n = n.trim().trim_end_matches('.').to_ascii_lowercase();
        if n.is_empty() {
            continue;
        }
        if !n.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.') || n.starts_with(['-', '.']) {
            bail!("'{n}' ist kein gültiger Rechnername (nur a–z, 0–9, '-' und '.')");
        }
        if !out.contains(&n) {
            out.push(n);
        }
    }
    if out.is_empty() {
        bail!("Mindestens ein Name für das Zertifikat ist nötig");
    }
    Ok(out)
}

fn sorted(mut v: Vec<String>) -> Vec<String> {
    v.sort();
    v
}

fn write_private(path: &Path, content: &str) -> anyhow::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(path, content).with_context(|| format!("{} lässt sich nicht schreiben", path.display()))
}

fn create_ca(files: &Managed, names: &[String], label: &str) -> anyhow::Result<()> {
    let key = KeyPair::generate()?;
    let mut params = CertificateParams::default();
    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, format!("TrackCraft Heimnetz-CA ({label})"));
    dn.push(DnType::OrganizationName, "TrackCraft");
    params.distinguished_name = dn;
    params.is_ca = IsCa::Ca(BasicConstraints::Constrained(0));
    params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign, KeyUsagePurpose::DigitalSignature];
    params.name_constraints = Some(NameConstraints {
        permitted_subtrees: names.iter().map(|n| GeneralSubtree::DnsName(n.clone())).collect(),
        excluded_subtrees: vec![],
    });
    let now = OffsetDateTime::now_utc();
    params.not_before = now - Duration::days(1);
    params.not_after = now + Duration::days(CA_DAYS);
    let cert = params.self_signed(&key)?;
    write_private(&files.ca_key, &key.serialize_pem())?;
    write_private(&files.ca_cert, &cert.pem())?;
    Ok(())
}

fn issue_server(files: &Managed, names: &[String]) -> anyhow::Result<()> {
    let ca_key = KeyPair::from_pem(&fs::read_to_string(&files.ca_key)?).context("CA-Schlüssel nicht lesbar")?;
    let issuer = Issuer::from_ca_cert_pem(&fs::read_to_string(&files.ca_cert)?, ca_key).context("CA-Zertifikat nicht lesbar")?;

    let key = KeyPair::generate()?;
    let mut params = CertificateParams::new(Vec::<String>::new())?;
    params.subject_alt_names = names.iter().map(|n| Ok(SanType::DnsName(n.clone().try_into()?))).collect::<Result<_, rcgen::Error>>()?;
    let mut dn = DistinguishedName::new();
    // Als Hauptname den ersten Netznamen nehmen; „localhost“ nur, wenn es nichts anderes gibt.
    let main = names.iter().find(|n| n.as_str() != "localhost").unwrap_or(&names[0]);
    dn.push(DnType::CommonName, main.clone());
    params.distinguished_name = dn;
    params.is_ca = IsCa::ExplicitNoCa;
    params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
    params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    // Authority Key Identifier: verlangt RFC 5280 und prüfen strenge Clients
    params.use_authority_key_identifier_extension = true;
    let now = OffsetDateTime::now_utc();
    params.not_before = now - Duration::days(1);
    params.not_after = now + Duration::days(SERVER_DAYS);
    let cert = params.signed_by(&key, &issuer)?;
    // Kette: Server, dann CA, damit Clients die CA auch ohne Zwischenablage zuordnen.
    let chain = format!("{}{}", cert.pem(), fs::read_to_string(&files.ca_cert)?);
    write_private(&files.server_key, &key.serialize_pem())?;
    write_private(&files.server_cert, &chain)?;
    Ok(())
}

pub struct CertInfo {
    pub not_after: OffsetDateTime,
    pub dns_names: Vec<String>,
    /// Trägt das Zertifikat einen Authority Key Identifier?
    pub has_authority_key_id: bool,
}

/// Ablaufdatum und DNS-Namen des ersten Zertifikats einer PEM-Datei.
pub fn inspect(pem: &str) -> anyhow::Result<CertInfo> {
    let (_, block) = x509_parser::pem::parse_x509_pem(pem.as_bytes()).map_err(|e| anyhow::anyhow!("kein PEM-Zertifikat: {e}"))?;
    let (_, cert) = X509Certificate::from_der(&block.contents).map_err(|e| anyhow::anyhow!("Zertifikat nicht lesbar: {e}"))?;
    let not_after = cert.validity().not_after.to_datetime();
    let mut dns_names = Vec::new();
    let mut has_authority_key_id = false;
    for ext in cert.extensions() {
        if let ParsedExtension::AuthorityKeyIdentifier(_) = ext.parsed_extension() {
            has_authority_key_id = true;
        }
        if let ParsedExtension::SubjectAlternativeName(san) = ext.parsed_extension() {
            for name in &san.general_names {
                if let GeneralName::DNSName(d) = name {
                    dns_names.push(d.to_ascii_lowercase());
                }
            }
        }
    }
    Ok(CertInfo { not_after, dns_names, has_authority_key_id })
}

/// Erlaubt die Namensbeschränkung der CA diesen Namen?
pub fn ca_permits(ca_pem: &str, name: &str) -> anyhow::Result<bool> {
    let (_, block) = x509_parser::pem::parse_x509_pem(ca_pem.as_bytes()).map_err(|e| anyhow::anyhow!("{e}"))?;
    let (_, cert) = X509Certificate::from_der(&block.contents).map_err(|e| anyhow::anyhow!("{e}"))?;
    for ext in cert.extensions() {
        if let ParsedExtension::NameConstraints(nc) = ext.parsed_extension() {
            let Some(permitted) = &nc.permitted_subtrees else { return Ok(true) };
            return Ok(permitted.iter().any(|s| match &s.base {
                GeneralName::DNSName(base) => {
                    let base = base.trim_start_matches('.').to_ascii_lowercase();
                    name == base || name.ends_with(&format!(".{base}"))
                }
                _ => false,
            }));
        }
    }
    Ok(true)
}
