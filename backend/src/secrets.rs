//! Gemeinsamer Geheimnisspeicher (#10): Geheimnisse liegen nur verschlüsselt in der Datenbank.
//!
//! Der Schlüssel steht in einer eigenen Datei (Vorgabe `secrets.key` neben `trackcraft.toml`, sonst
//! `[secrets] key_file`), nie in der Datenbank und nie in einer Sicherung. Verfahren: AES-256-GCM (ring), je Wert
//! eine eigene Zufalls-Nonce, der Zweck (z. B. `garmin.api_key`) als zusätzliche Daten, damit sich ein Wert nicht in
//! ein anderes Feld kopieren lässt.
//!
//! Gespeichertes Format: `tcs1:<Schlüssel-ID>:<Nonce>:<Chiffrat mit Tag>`, alles Hex. Die Schlüssel-ID (die ersten
//! 8 Byte von SHA-256 des Schlüssels) unterscheidet „andere Schlüsseldatei“ (z. B. nach einer Wiederherstellung auf
//! einem anderen Rechner) von beschädigten Werten. Werte ohne Präfix sind Klartext aus der Zeit vor #10.

use std::fmt;
use std::io::Write;
use std::path::{Path, PathBuf};

use ring::aead::{AES_256_GCM, Aad, LessSafeKey, NONCE_LEN, Nonce, UnboundKey};
use ring::rand::{SecureRandom, SystemRandom};
use sha2::{Digest, Sha256};

const PREFIX: &str = "tcs1:";
const KEY_LEN: usize = 32;

/// Zweck des Garmin-API-Schlüssels.
pub const GARMIN_API_KEY: &str = "garmin.api_key";

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SecretError {
    /// Keine Schlüsseldatei geladen (fehlt und ließ sich nicht anlegen, oder unlesbar).
    #[error("Geheimnisspeicher nicht verfügbar: {0}")]
    Unavailable(String),
    /// Der Wert wurde mit einer anderen Schlüsseldatei verschlüsselt.
    #[error("Wert gehört zu einer anderen Schlüsseldatei")]
    OtherKey,
    /// Format oder Prüfsumme stimmt nicht.
    #[error("Wert ist beschädigt")]
    Corrupt,
}

enum State {
    Ready { key: Box<LessSafeKey>, id: String },
    Unavailable(String),
}

/// Geladener Schlüssel; ohne Schlüssel lassen sich nur noch Klartext-Altwerte lesen.
pub struct Secrets {
    state: State,
    rng: SystemRandom,
}

impl Default for Secrets {
    fn default() -> Self {
        Self::unavailable("nicht geladen")
    }
}

/// Ohne Schlüssel (ADR-0013).
impl fmt::Debug for Secrets {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.state {
            State::Ready { id, .. } => f.debug_struct("Secrets").field("key_id", id).finish(),
            State::Unavailable(r) => f.debug_struct("Secrets").field("unavailable", r).finish(),
        }
    }
}

/// Ort der Schlüsseldatei: `[secrets] key_file` oder `secrets.key` neben der Konfigurationsdatei.
pub fn key_path(cfg: &crate::config::Validated) -> PathBuf {
    match &cfg.raw.secrets.key_file {
        Some(p) => p.clone(),
        None => cfg.path.parent().map(Path::to_path_buf).unwrap_or_default().join("secrets.key"),
    }
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn from_hex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) || !s.is_ascii() {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok()).collect()
}

fn key_id(raw: &[u8]) -> String {
    to_hex(&Sha256::digest(raw)[..8])
}

/// Inhalt einer neuen Schlüsseldatei.
fn new_key_text(rng: &SystemRandom) -> std::io::Result<String> {
    let mut raw = [0u8; KEY_LEN];
    rng.fill(&mut raw).map_err(|_| std::io::Error::other("Zufallsquelle nicht verfügbar"))?;
    Ok(format!(
        "# TrackCraft-Geheimnisschlüssel (#10). Nicht weitergeben, nicht löschen: ohne ihn sind gespeicherte\n\
         # Geheimnisse (z. B. der Garmin-Schlüssel) nicht mehr lesbar. Bei einem Umzug mitkopieren.\n{}\n",
        to_hex(&raw)
    ))
}

fn parse_key_text(text: &str) -> Result<Vec<u8>, String> {
    let line = text.lines().map(str::trim).find(|l| !l.is_empty() && !l.starts_with('#')).ok_or("Schlüsseldatei ist leer")?;
    match from_hex(line) {
        Some(raw) if raw.len() == KEY_LEN => Ok(raw),
        _ => Err(format!("Schlüsseldatei enthält keinen gültigen Schlüssel ({} Hex-Zeichen erwartet)", KEY_LEN * 2)),
    }
}

/// Legt die Schlüsseldatei an, wenn es sie noch nicht gibt (Setup und Start). `true` = neu angelegt.
/// Unter Unix nur für den Besitzer lesbar; Rechte für den Dienst setzt das Setup.
pub fn ensure_key_file(path: &Path) -> std::io::Result<bool> {
    if path.exists() {
        return Ok(false);
    }
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)?;
    }
    let text = new_key_text(&SystemRandom::new())?;
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut f = match opts.open(path) {
        Ok(f) => f,
        // Gleichzeitig angelegt: gilt als vorhanden.
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => return Ok(false),
        Err(e) => return Err(e),
    };
    f.write_all(text.as_bytes())?;
    f.sync_all()?;
    Ok(true)
}

impl Secrets {
    pub fn unavailable(reason: impl Into<String>) -> Self {
        Self { state: State::Unavailable(reason.into()), rng: SystemRandom::new() }
    }

    fn from_raw(raw: &[u8]) -> Self {
        let key = LessSafeKey::new(UnboundKey::new(&AES_256_GCM, raw).expect("32 Byte"));
        Self { state: State::Ready { key: Box::new(key), id: key_id(raw) }, rng: SystemRandom::new() }
    }

    /// Zufälliger Schlüssel nur im Speicher (Tests, Demo).
    pub fn ephemeral() -> Self {
        let mut raw = [0u8; KEY_LEN];
        SystemRandom::new().fill(&mut raw).expect("Zufallsquelle");
        Self::from_raw(&raw)
    }

    /// Lädt die Schlüsseldatei und legt sie an, wenn sie fehlt und der Ordner beschreibbar ist. Scheitert das,
    /// bleibt der Speicher „nicht verfügbar“: Der Server startet trotzdem, Geheimnisse lassen sich dann nicht
    /// speichern.
    pub fn open(path: &Path) -> Self {
        match ensure_key_file(path) {
            Ok(true) => tracing::info!(file = %path.display(), "Schlüsseldatei für Geheimnisse angelegt"),
            Ok(false) => {}
            Err(e) => {
                tracing::warn!(file = %path.display(), "Schlüsseldatei für Geheimnisse lässt sich nicht anlegen: {e}");
                return Self::unavailable(format!("{} fehlt und lässt sich nicht anlegen", path.display()));
            }
        }
        match std::fs::read_to_string(path).map_err(|e| e.to_string()).and_then(|t| parse_key_text(&t)) {
            Ok(raw) => Self::from_raw(&raw),
            Err(e) => {
                tracing::error!(file = %path.display(), "Schlüsseldatei für Geheimnisse nicht lesbar: {e}");
                Self::unavailable(format!("{} ist nicht lesbar", path.display()))
            }
        }
    }

    pub fn is_available(&self) -> bool {
        matches!(self.state, State::Ready { .. })
    }

    /// Grund, falls nicht verfügbar.
    pub fn unavailable_reason(&self) -> Option<&str> {
        match &self.state {
            State::Unavailable(r) => Some(r),
            State::Ready { .. } => None,
        }
    }

    /// Ist der gespeicherte Wert schon verschlüsselt (sonst Klartext-Altwert)?
    pub fn is_sealed(stored: &str) -> bool {
        stored.starts_with(PREFIX)
    }

    pub fn seal(&self, purpose: &str, plain: &str) -> Result<String, SecretError> {
        let State::Ready { key, id } = &self.state else {
            return Err(SecretError::Unavailable(self.unavailable_reason().unwrap_or_default().into()));
        };
        let mut nonce = [0u8; NONCE_LEN];
        self.rng.fill(&mut nonce).map_err(|_| SecretError::Unavailable("Zufallsquelle nicht verfügbar".into()))?;
        let mut data = plain.as_bytes().to_vec();
        key.seal_in_place_append_tag(Nonce::assume_unique_for_key(nonce), Aad::from(purpose.as_bytes()), &mut data)
            .map_err(|_| SecretError::Corrupt)?;
        Ok(format!("{PREFIX}{id}:{}:{}", to_hex(&nonce), to_hex(&data)))
    }

    /// Liest einen gespeicherten Wert; Klartext-Altwerte kommen unverändert zurück.
    pub fn open_value(&self, purpose: &str, stored: &str) -> Result<String, SecretError> {
        let Some(rest) = stored.strip_prefix(PREFIX) else {
            return Ok(stored.to_string());
        };
        let State::Ready { key, id } = &self.state else {
            return Err(SecretError::Unavailable(self.unavailable_reason().unwrap_or_default().into()));
        };
        let mut parts = rest.split(':');
        let (Some(kid), Some(nonce), Some(data), None) = (parts.next(), parts.next(), parts.next(), parts.next()) else {
            return Err(SecretError::Corrupt);
        };
        if kid != id {
            return Err(SecretError::OtherKey);
        }
        let nonce: [u8; NONCE_LEN] = from_hex(nonce).and_then(|n| n.try_into().ok()).ok_or(SecretError::Corrupt)?;
        let mut data = from_hex(data).ok_or(SecretError::Corrupt)?;
        let plain = key
            .open_in_place(Nonce::assume_unique_for_key(nonce), Aad::from(purpose.as_bytes()), &mut data)
            .map_err(|_| SecretError::Corrupt)?;
        String::from_utf8(plain.to_vec()).map_err(|_| SecretError::Corrupt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_with_fresh_nonce() {
        let s = Secrets::ephemeral();
        let a = s.seal(GARMIN_API_KEY, "geheim-123").unwrap();
        let b = s.seal(GARMIN_API_KEY, "geheim-123").unwrap();
        assert_ne!(a, b);
        assert!(Secrets::is_sealed(&a));
        assert!(!a.contains("geheim"));
        assert_eq!(s.open_value(GARMIN_API_KEY, &a).unwrap(), "geheim-123");
    }

    #[test]
    fn purpose_is_bound() {
        let s = Secrets::ephemeral();
        let a = s.seal(GARMIN_API_KEY, "x").unwrap();
        assert_eq!(s.open_value("other.secret", &a), Err(SecretError::Corrupt));
    }

    #[test]
    fn other_key_and_tampering_are_detected() {
        let s = Secrets::ephemeral();
        let a = s.seal(GARMIN_API_KEY, "x").unwrap();
        assert_eq!(Secrets::ephemeral().open_value(GARMIN_API_KEY, &a), Err(SecretError::OtherKey));
        let mut t = a.clone();
        let last = t.pop().unwrap();
        t.push(if last == '0' { '1' } else { '0' });
        assert_eq!(s.open_value(GARMIN_API_KEY, &t), Err(SecretError::Corrupt));
        assert_eq!(s.open_value(GARMIN_API_KEY, "tcs1:zz"), Err(SecretError::Corrupt));
    }

    #[test]
    fn plaintext_passes_through_and_unavailable_cannot_seal() {
        let u = Secrets::unavailable("test");
        assert_eq!(u.open_value(GARMIN_API_KEY, "alt-klartext").unwrap(), "alt-klartext");
        assert!(matches!(u.seal(GARMIN_API_KEY, "x"), Err(SecretError::Unavailable(_))));
        let sealed = Secrets::ephemeral().seal(GARMIN_API_KEY, "x").unwrap();
        assert!(matches!(u.open_value(GARMIN_API_KEY, &sealed), Err(SecretError::Unavailable(_))));
    }

    #[test]
    fn key_file_is_created_once_and_reloaded() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub").join("secrets.key");
        let first = Secrets::open(&path);
        assert!(first.is_available());
        let sealed = first.seal(GARMIN_API_KEY, "wert").unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!ensure_key_file(&path).unwrap());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
        assert_eq!(Secrets::open(&path).open_value(GARMIN_API_KEY, &sealed).unwrap(), "wert");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        }
    }

    #[test]
    fn broken_key_file_leaves_store_unavailable() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secrets.key");
        std::fs::write(&path, "# nur Kommentar\nabc\n").unwrap();
        let s = Secrets::open(&path);
        assert!(!s.is_available());
        assert!(s.unavailable_reason().unwrap().contains("nicht lesbar"));
    }
}
