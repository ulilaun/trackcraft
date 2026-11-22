//! HTTP-Zugriff auf die persönliche Garmin-API: Token aus dem Schlüssel, Aktivitätenliste, Download.
//! Schlüssel und Token stehen nur in Headern, nie in URLs oder im Log (C7).

use std::io::Read;
use std::sync::Arc;
use std::time::Duration;

use serde::Deserialize;

/// Fehler eines Aufrufs, nach Folge für den Abruf unterschieden.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientError {
    /// 401/403: Schlüssel abgelaufen, widerrufen oder falsch
    KeyInvalid,
    /// 429: später erneut
    RateLimited,
    /// Aktivität gibt es nicht (mehr)
    NotFound,
    /// Andere HTTP-Antwort
    Status(u16),
    /// Verbindung, Zeitüberschreitung, TLS
    Network(String),
    /// Antwort nicht im erwarteten Format
    Format(String),
}

impl ClientError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::KeyInvalid => "garmin_key_invalid",
            Self::RateLimited => "garmin_rate_limited",
            Self::NotFound => "garmin_not_found",
            Self::Status(_) => "garmin_http_error",
            Self::Network(_) => "garmin_unreachable",
            Self::Format(_) => "garmin_bad_response",
        }
    }

    /// Deutsche Meldung für Status und Oberfläche.
    pub fn message(&self) -> String {
        match self {
            Self::KeyInvalid => "Garmin lehnt den Schlüssel ab (abgelaufen oder ungültig). Bitte einen neuen Schlüssel eintragen.".into(),
            Self::RateLimited => "Garmin meldet zu viele Anfragen; der nächste Lauf macht weiter.".into(),
            Self::NotFound => "Die Aktivität gibt es in Garmin Connect nicht mehr.".into(),
            Self::Status(s) => format!("Garmin antwortet mit HTTP {s}."),
            Self::Network(e) => format!("Garmin ist nicht erreichbar: {e}"),
            Self::Format(e) => format!("Unerwartete Antwort von Garmin: {e}"),
        }
    }
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message())
    }
}

/// Eintrag der Aktivitätenliste; nur die Felder, die der Abruf braucht.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ListEntry {
    #[serde(rename = "activityId")]
    pub activity_id: i64,
    /// Start in UTC, Millisekunden
    #[serde(rename = "beginTimestamp")]
    pub begin_ms: i64,
    #[serde(rename = "activityType", default)]
    pub activity_type: Option<ActivityType>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ActivityType {
    #[serde(rename = "typeKey", default)]
    pub type_key: Option<String>,
}

impl ListEntry {
    pub fn type_key(&self) -> Option<&str> {
        self.activity_type.as_ref().and_then(|t| t.type_key.as_deref()).filter(|k| !k.trim().is_empty())
    }
}

const TOKEN_PATH: &str = "/personalapitoken-service/personal-api/personal-api-key/token";
const LIST_PATH: &str = "/activitylist-service/activities/search/activities";
const DOWNLOAD_PATH: &str = "/download-service/files/activity";

pub struct Client {
    http: reqwest::Client,
    base: String,
}

fn network(e: reqwest::Error) -> ClientError {
    // Ohne URL: sie enthält zwar kein Geheimnis, die Meldung bleibt aber kurz.
    ClientError::Network(e.without_url().to_string())
}

fn check(status: reqwest::StatusCode) -> Result<(), ClientError> {
    match status.as_u16() {
        200..=299 => Ok(()),
        401 | 403 => Err(ClientError::KeyInvalid),
        404 => Err(ClientError::NotFound),
        429 => Err(ClientError::RateLimited),
        s => Err(ClientError::Status(s)),
    }
}

impl Client {
    pub fn new(base: &str) -> anyhow::Result<Self> {
        // rustls mit ring und den Mozilla-Wurzeln: gleich auf Windows, Linux und im Container.
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let roots = rustls::RootCertStore { roots: webpki_roots::TLS_SERVER_ROOTS.to_vec() };
        let tls = rustls::ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()?
            .with_root_certificates(roots)
            .with_no_client_auth();
        let http = reqwest::Client::builder()
            .tls_backend_preconfigured(tls)
            .user_agent(concat!("TrackCraft/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(20))
            .timeout(Duration::from_secs(180))
            .build()?;
        Ok(Self { http, base: base.trim_end_matches('/').to_string() })
    }

    /// Zugriffstoken (24 h gültig); die Antwort ist das Token als Text.
    pub async fn token(&self, api_key: &str) -> Result<String, ClientError> {
        let resp = self
            .http
            .post(format!("{}{TOKEN_PATH}", self.base))
            .header(reqwest::header::AUTHORIZATION, format!("ApiKey {api_key}"))
            .send()
            .await
            .map_err(network)?;
        check(resp.status())?;
        let text = resp.text().await.map_err(network)?;
        let token = text.trim().trim_matches('"').to_string();
        if token.is_empty() || token.len() > 16 * 1024 || token.contains(char::is_whitespace) {
            return Err(ClientError::Format("Token fehlt".into()));
        }
        Ok(token)
    }

    /// Eine Seite der Aktivitätenliste, neueste zuerst.
    pub async fn list(&self, token: &str, start: usize, limit: usize) -> Result<Vec<ListEntry>, ClientError> {
        let resp = self
            .http
            .get(format!("{}{LIST_PATH}?start={start}&limit={limit}", self.base))
            .bearer_auth(token)
            .send()
            .await
            .map_err(network)?;
        check(resp.status())?;
        let bytes = resp.bytes().await.map_err(network)?;
        serde_json::from_slice(&bytes).map_err(|e| ClientError::Format(format!("Aktivitätenliste: {e}")))
    }

    /// Aktivitätsdatei (ZIP mit einer FIT); höchstens `max_bytes`.
    pub async fn download(&self, token: &str, activity_id: i64, max_bytes: u64) -> Result<Vec<u8>, ClientError> {
        let mut resp =
            self.http.get(format!("{}{DOWNLOAD_PATH}/{activity_id}", self.base)).bearer_auth(token).send().await.map_err(network)?;
        check(resp.status())?;
        if resp.content_length().is_some_and(|n| n > max_bytes) {
            return Err(ClientError::Format("Datei größer als die Importgrenze".into()));
        }
        let mut out = Vec::new();
        while let Some(chunk) = resp.chunk().await.map_err(network)? {
            out.extend_from_slice(&chunk);
            if out.len() as u64 > max_bytes {
                return Err(ClientError::Format("Datei größer als die Importgrenze".into()));
            }
        }
        Ok(out)
    }
}

/// FIT aus dem Download: ZIP mit genau einer `.fit` oder schon eine FIT-Datei.
pub fn extract_fit(bytes: &[u8], max_bytes: u64) -> Result<Vec<u8>, String> {
    if bytes.len() >= 12 && &bytes[8..12] == b".FIT" {
        return Ok(bytes.to_vec());
    }
    if !bytes.starts_with(b"PK") {
        return Err("weder ZIP noch FIT".into());
    }
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| format!("ZIP unlesbar: {e}"))?;
    let mut found = None;
    for i in 0..zip.len() {
        let f = zip.by_index_raw(i).map_err(|e| format!("ZIP unlesbar: {e}"))?;
        if f.is_file() && f.name().is_ok_and(|n| n.to_ascii_lowercase().ends_with(".fit")) {
            if found.is_some() {
                return Err("ZIP enthält mehr als eine FIT-Datei".into());
            }
            found = Some(i);
        }
    }
    let i = found.ok_or("ZIP enthält keine FIT-Datei")?;
    let f = zip.by_index(i).map_err(|e| format!("ZIP unlesbar: {e}"))?;
    let mut out = Vec::new();
    // Größe beim Entpacken begrenzen, nicht nur nach Angabe im ZIP.
    f.take(max_bytes + 1).read_to_end(&mut out).map_err(|e| format!("ZIP unlesbar: {e}"))?;
    if out.len() as u64 > max_bytes {
        return Err("FIT-Datei größer als die Importgrenze".into());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    fn zip_of(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut w = zip::ZipWriter::new(&mut buf);
            let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
            for (name, data) in files {
                w.start_file(*name, opts).unwrap();
                w.write_all(data).unwrap();
            }
            w.finish().unwrap();
        }
        buf.into_inner()
    }

    #[test]
    fn extracts_single_fit() {
        let z = zip_of(&[("123_ACTIVITY.fit", b"0123456789abcdef")]);
        assert_eq!(extract_fit(&z, 1000).unwrap(), b"0123456789abcdef");
    }

    #[test]
    fn rejects_missing_double_and_oversized() {
        assert!(extract_fit(&zip_of(&[("a.txt", b"x")]), 1000).unwrap_err().contains("keine FIT"));
        assert!(extract_fit(&zip_of(&[("a.fit", b"x"), ("b.FIT", b"y")]), 1000).unwrap_err().contains("mehr als eine"));
        assert!(extract_fit(&zip_of(&[("a.fit", &[0u8; 100])]), 10).unwrap_err().contains("größer"));
        assert!(extract_fit(b"hello world!", 1000).is_err());
    }

    #[test]
    fn passes_plain_fit() {
        let mut fit = vec![14u8, 0x10, 0, 0, 0, 0, 0, 0];
        fit.extend_from_slice(b".FIT");
        assert_eq!(extract_fit(&fit, 1000).unwrap(), fit);
    }

    #[test]
    fn list_entry_reads_garmin_fields() {
        let e: Vec<ListEntry> = serde_json::from_str(
            r#"[{"activityId": 24814633380, "beginTimestamp": 1792653637000, "activityType": {"typeId": 10, "typeKey": "road_biking"}, "x": 1}]"#,
        )
        .unwrap();
        assert_eq!(e[0].activity_id, 24814633380);
        assert_eq!(e[0].type_key(), Some("road_biking"));
    }
}
