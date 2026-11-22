//! Eigene Heimnetz-CA und Serverzertifikat (RFC-0005, RFC-0011).

use std::sync::Arc;

use rustls::client::WebPkiServerVerifier;
use rustls::client::danger::ServerCertVerifier;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use trackcraft::cert::{self, Managed};

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

/// Prüft wie ein Browser: Kette bis zur CA, Name, Namensbeschränkung, Gültigkeit.
fn verify(files: &Managed, host: &str) -> Result<(), rustls::Error> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut roots = rustls::RootCertStore::empty();
    roots.add(CertificateDer::from_pem_file(&files.ca_cert).unwrap()).unwrap();
    let verifier = WebPkiServerVerifier::builder_with_provider(Arc::new(roots), provider).build().unwrap();
    let chain: Vec<CertificateDer> = CertificateDer::pem_file_iter(&files.server_cert).unwrap().map(|c| c.unwrap()).collect();
    verifier.verify_server_cert(&chain[0], &chain[1..], &ServerName::try_from(host.to_string()).unwrap(), &[], UnixTime::now()).map(|_| ())
}

#[test]
fn creates_ca_and_server_cert_that_clients_accept() {
    let dir = tempfile::tempdir().unwrap();
    let files = Managed::in_config_dir(dir.path());
    let out = cert::ensure(&files, &names(&["pc.home.arpa", "localhost"]), false, "TEST").unwrap();
    assert!(out.ca_created);
    assert!(out.server_issued.is_some());

    verify(&files, "pc.home.arpa").unwrap();
    verify(&files, "localhost").unwrap();
    assert!(verify(&files, "evil.example").is_err(), "fremder Name darf nicht passen");

    // Der Server startet mit dem Paar (Schlüssel passt zum Zertifikat).
    trackcraft::tls::server_config(&files.server_cert, &files.server_key).unwrap();

    let info = cert::inspect(&std::fs::read_to_string(&files.server_cert).unwrap()).unwrap();
    assert!(info.has_authority_key_id, "strenge Clients verlangen den Authority Key Identifier");
    let days = (info.not_after - time::OffsetDateTime::now_utc()).whole_days();
    assert!((cert::SERVER_DAYS - 2..=cert::SERVER_DAYS).contains(&days), "{days}");
}

#[test]
fn second_run_keeps_everything_and_name_change_reissues() {
    let dir = tempfile::tempdir().unwrap();
    let files = Managed::in_config_dir(dir.path());
    cert::ensure(&files, &names(&["pc.home.arpa", "localhost"]), false, "TEST").unwrap();
    let ca_before = std::fs::read(&files.ca_cert).unwrap();
    let server_before = std::fs::read(&files.server_cert).unwrap();

    let out = cert::ensure(&files, &names(&["LOCALHOST", "pc.home.arpa"]), false, "TEST").unwrap();
    assert_eq!(out, cert::Outcome::default(), "gleiche Namen in anderer Schreibweise: nichts tun");
    assert_eq!(std::fs::read(&files.server_cert).unwrap(), server_before);

    // Weniger Namen liegen innerhalb der Beschränkung: nur das Serverzertifikat wird neu ausgestellt.
    let out = cert::ensure(&files, &names(&["pc.home.arpa"]), false, "TEST").unwrap();
    assert!(!out.ca_created);
    assert_eq!(out.server_issued.as_deref(), Some("Namen geändert"));
    assert_eq!(std::fs::read(&files.ca_cert).unwrap(), ca_before);
    verify(&files, "pc.home.arpa").unwrap();
    assert!(verify(&files, "localhost").is_err());
}

#[test]
fn ca_is_limited_to_its_names() {
    let dir = tempfile::tempdir().unwrap();
    let files = Managed::in_config_dir(dir.path());
    cert::ensure(&files, &names(&["pc.home.arpa"]), false, "TEST").unwrap();
    let ca = std::fs::read_to_string(&files.ca_cert).unwrap();
    assert!(cert::ca_permits(&ca, "pc.home.arpa").unwrap());
    assert!(cert::ca_permits(&ca, "app.pc.home.arpa").unwrap());
    assert!(!cert::ca_permits(&ca, "home.arpa").unwrap());
    assert!(!cert::ca_permits(&ca, "bank.example").unwrap());

    let err = cert::ensure(&files, &names(&["trackcraft.home.arpa"]), false, "TEST").unwrap_err();
    assert!(err.to_string().contains("--new-ca"), "{err}");

    let out = cert::ensure(&files, &names(&["trackcraft.home.arpa"]), true, "TEST").unwrap();
    assert!(out.ca_created);
    verify(&files, "trackcraft.home.arpa").unwrap();
}

#[test]
fn invalid_names_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let files = Managed::in_config_dir(dir.path());
    for bad in [vec![], names(&["  "]), names(&["pc home"]), names(&["-pc"]), names(&["pc/x"])] {
        assert!(cert::ensure(&files, &bad, false, "TEST").is_err(), "{bad:?}");
    }
}

#[test]
fn server_cert_names_network_name_not_localhost() {
    let dir = tempfile::tempdir().unwrap();
    let files = Managed::in_config_dir(dir.path());
    cert::ensure(&files, &names(&["localhost", "trackcraft.home.arpa"]), false, "TEST").unwrap();
    let pem = std::fs::read_to_string(&files.server_cert).unwrap();
    let (_, block) = x509_parser::pem::parse_x509_pem(pem.as_bytes()).unwrap();
    let (_, c) = x509_parser::parse_x509_certificate(&block.contents).unwrap();
    let cn = c.subject().iter_common_name().next().unwrap().as_str().unwrap().to_string();
    assert_eq!(cn, "trackcraft.home.arpa");
    verify(&files, "trackcraft.home.arpa").unwrap();
}
