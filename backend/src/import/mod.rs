//! Import nach RFC-0008: Formaterkennung, Parser (FIT, GPX) und Aufbereitung je Session.
//! Dieses Modul hat keine Datenbank- oder HTTP-Abhängigkeit; Importvorgänge und Speichern stehen in `import_job`.

pub mod fit;
pub mod gpx;
pub mod process;

use crate::domain::track::Point;

/// Dateiformat eines Uploads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Fit,
    Gpx,
}

impl Format {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fit => "fit",
            Self::Gpx => "gpx",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "fit" => Some(Self::Fit),
            "gpx" => Some(Self::Gpx),
            _ => None,
        }
    }
}

/// Abweisung einer ganzen Datei; der Code entspricht RFC-0003 Abschnitt 7.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ImportError {
    #[error("weder FIT noch GPX")]
    UnsupportedFormat,
    #[error("Datei beschädigt oder nicht lesbar: {0}")]
    InvalidFile(String),
    #[error("GPX ohne Zeitangaben (geplante Route, keine Aktivität)")]
    GpxWithoutTime,
}

impl ImportError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::UnsupportedFormat => "unsupported_format",
            Self::InvalidFile(_) => "invalid_file",
            Self::GpxWithoutTime => "gpx_without_time",
        }
    }
}

/// Werte aus der Datei (FIT-Session); fehlend heißt unbekannt, nie 0 (RFC-0008 Abschnitt 6).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FileValues {
    pub start_ms: Option<i64>,
    pub elapsed_ms: Option<i64>,
    pub distance_m: Option<f64>,
    pub ascent_m: Option<f64>,
    pub avg_hr: Option<u16>,
}

/// Messpunkt wie gelesen, vor dem Outlier-Filter. Geschwindigkeit und Distanz der Records dienen nur der
/// Fahrzeugerkennung und werden nicht gespeichert (RFC-0008 Abschnitt 3).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct RawPoint {
    pub point: Point,
    pub speed_mps: Option<f64>,
    pub distance_m: Option<f64>,
}

/// Eine FIT-Session bzw. die ganze GPX-Datei: wird zu einer Aktivität.
#[derive(Debug, Clone, Default)]
pub struct ParsedSession {
    pub session_index: usize,
    /// Normalisierter Sporttyp aus der Datei (RFC-0008 Abschnitt 4), `None` ohne Angabe
    pub source_sport: Option<String>,
    pub file: FileValues,
    /// FIT: genau ein Abschnitt, GPX: je `<trkseg>` einer (ohne leere)
    pub sections: Vec<Vec<RawPoint>>,
    /// Titel aus der Datei (nur GPX)
    pub title: Option<String>,
}

impl ParsedSession {
    pub fn point_count(&self) -> usize {
        self.sections.iter().map(Vec::len).sum()
    }
}

#[derive(Debug, Clone)]
pub struct ParsedFile {
    pub format: Format,
    pub sessions: Vec<ParsedSession>,
    /// FIT mit falscher Datei-Prüfsumme, deren Inhalt sich trotzdem vollständig lesen ließ
    pub checksum_ignored: bool,
}

/// Erkennt das Format am Inhalt, nicht am Dateinamen.
pub fn detect(bytes: &[u8]) -> Result<Format, ImportError> {
    if bytes.len() >= 12 && (bytes[0] == 12 || bytes[0] == 14) && &bytes[8..12] == b".FIT" {
        return Ok(Format::Fit);
    }
    let text = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let start = text.iter().position(|b| !b.is_ascii_whitespace()).unwrap_or(text.len());
    let head = &text[start..text.len().min(start + 4096)];
    if head.starts_with(b"<?xml") || head.starts_with(b"<gpx") || head.starts_with(b"<!--") {
        let head = String::from_utf8_lossy(head);
        if head.contains("<gpx") {
            return Ok(Format::Gpx);
        }
    }
    Err(ImportError::UnsupportedFormat)
}

/// Erkennt das Format und liest die Datei.
pub fn parse(bytes: &[u8]) -> Result<ParsedFile, ImportError> {
    match detect(bytes)? {
        Format::Fit => {
            let (sessions, checksum_ignored) = fit::parse(bytes)?;
            Ok(ParsedFile { format: Format::Fit, sessions, checksum_ignored })
        }
        Format::Gpx => Ok(ParsedFile { format: Format::Gpx, sessions: vec![gpx::parse(bytes)?], checksum_ignored: false }),
    }
}
