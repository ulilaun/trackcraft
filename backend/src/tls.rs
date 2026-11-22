//! TLS im Backend (Windows, RFC-0005/RFC-0011): rustls mit ring, TLS 1.2 und 1.3, Zertifikat aus `[tls]`.

use std::path::Path;
use std::sync::Arc;

use anyhow::Context;
use rustls::ServerConfig;
use rustls_pki_types::pem::PemObject;
use rustls_pki_types::{CertificateDer, PrivateKeyDer};

/// Baut die Serverkonfiguration; prüft dabei Lesbarkeit und dass Schlüssel und Zertifikat zusammenpassen.
pub fn server_config(cert_file: &Path, key_file: &Path) -> anyhow::Result<Arc<ServerConfig>> {
    let certs: Vec<CertificateDer<'static>> = CertificateDer::pem_file_iter(cert_file)
        .with_context(|| format!("Zertifikat {} nicht lesbar", cert_file.display()))?
        .collect::<Result<_, _>>()
        .with_context(|| format!("Zertifikat {} ist kein gültiges PEM", cert_file.display()))?;
    if certs.is_empty() {
        anyhow::bail!("Zertifikat {} enthält kein Zertifikat", cert_file.display());
    }
    let key = PrivateKeyDer::from_pem_file(key_file)
        .with_context(|| format!("Schlüssel {} nicht lesbar oder kein PEM-Schlüssel", key_file.display()))?;

    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut config = ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()?
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .context("Zertifikat und Schlüssel passen nicht zusammen")?;
    // Nur HTTP/1.1, bis HTTP/2 (Host aus `:authority`) mit einem echten Client getestet ist.
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok(Arc::new(config))
}
