//! TLS im Backend (RFC-0011): Zertifikat und Schlüssel müssen lesbar sein und zusammenpassen.

use trackcraft::tls;

fn write_pair(dir: &std::path::Path, name: &str) -> (std::path::PathBuf, std::path::PathBuf) {
    let cert = rcgen::generate_simple_self_signed(vec!["trackcraft.test".to_string()]).unwrap();
    let c = dir.join(format!("{name}.pem"));
    let k = dir.join(format!("{name}-key.pem"));
    std::fs::write(&c, cert.cert.pem()).unwrap();
    std::fs::write(&k, cert.signing_key.serialize_pem()).unwrap();
    (c, k)
}

#[test]
fn matching_pair_builds_config_with_alpn() {
    let dir = tempfile::tempdir().unwrap();
    let (c, k) = write_pair(dir.path(), "a");
    let cfg = tls::server_config(&c, &k).unwrap();
    assert_eq!(cfg.alpn_protocols, vec![b"http/1.1".to_vec()]);
}

#[test]
fn mismatched_key_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let (c, _) = write_pair(dir.path(), "a");
    let (_, k) = write_pair(dir.path(), "b");
    let err = tls::server_config(&c, &k).unwrap_err();
    assert!(format!("{err:#}").contains("passen nicht zusammen"), "{err:#}");
}

#[test]
fn missing_or_invalid_files_are_reported() {
    let dir = tempfile::tempdir().unwrap();
    let (c, k) = write_pair(dir.path(), "a");
    let missing = dir.path().join("fehlt.pem");
    assert!(format!("{:#}", tls::server_config(&missing, &k).unwrap_err()).contains("nicht lesbar"));
    let junk = dir.path().join("junk.pem");
    std::fs::write(&junk, "kein pem").unwrap();
    assert!(tls::server_config(&junk, &k).is_err());
    assert!(tls::server_config(&c, &junk).is_err());
}
