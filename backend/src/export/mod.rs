//! Export (RFC-0019, #58): GPX aus den aufbereiteten Punkten, Dateinamen und das Gesamt-ZIP im Strom.
//!
//! Das ZIP entsteht beim Herunterladen: ein Blocking-Thread schreibt mit `ZipWriter::new_stream` in einen
//! [`ChunkWriter`], der volle Blöcke über einen begrenzten Kanal an den Antwortkörper ([`ChannelBody`]) gibt.
//! Bricht der Browser ab, schlägt das nächste Senden fehl und der Thread endet ohne Rückstände.

pub mod gpx;

use std::io::{self, Write};
use std::pin::Pin;
use std::task::{Context, Poll};

use axum::body::Bytes;
use chrono::{TimeZone, Utc};
use tokio::sync::mpsc;

/// Größe eines Blocks im Kanal.
const CHUNK: usize = 64 * 1024;
/// Blöcke im Kanal, bevor der Schreiber wartet.
const CHANNEL_DEPTH: usize = 8;

/// Dateiname ohne Endung: Ortszeit des Starts und Titel, nur ASCII-Buchstaben, Ziffern und `-` (RFC-0019).
pub fn file_stem(start_time_ms: i64, tz: chrono_tz::Tz, title: &str) -> String {
    let when = Utc.timestamp_millis_opt(start_time_ms).single().map(|t| t.with_timezone(&tz).format("%Y-%m-%d-%H%M").to_string());
    let slug = slug(title);
    match (when, slug.is_empty()) {
        (Some(w), false) => format!("{w}-{slug}"),
        (Some(w), true) => w,
        (None, false) => slug,
        (None, true) => "aktivitaet".into(),
    }
}

/// Titel als Dateinamensteil: Umlaute umschreiben, alles andere außer ASCII-Buchstaben und Ziffern wird `-`,
/// höchstens 60 Zeichen.
pub fn slug(title: &str) -> String {
    let mut out = String::new();
    for ch in title.chars() {
        let rep: &str = match ch {
            'ä' => "ae",
            'ö' => "oe",
            'ü' => "ue",
            'Ä' => "Ae",
            'Ö' => "Oe",
            'Ü' => "Ue",
            'ß' => "ss",
            c if c.is_ascii_alphanumeric() => {
                out.push(c);
                continue;
            }
            _ => "-",
        };
        for r in rep.chars() {
            if r == '-' && (out.is_empty() || out.ends_with('-')) {
                continue;
            }
            out.push(r);
        }
    }
    let mut s: String = out.trim_end_matches('-').chars().take(60).collect();
    while s.ends_with('-') {
        s.pop();
    }
    s
}

/// Hängt bei Namensgleichheit `-2`, `-3` … an.
#[derive(Default)]
pub struct UniqueNames(std::collections::HashSet<String>);

impl UniqueNames {
    pub fn take(&mut self, stem: &str, ext: &str) -> String {
        let mut name = format!("{stem}.{ext}");
        let mut n = 2;
        while !self.0.insert(name.clone()) {
            name = format!("{stem}-{n}.{ext}");
            n += 1;
        }
        name
    }
}

/// `Content-Disposition` für einen Download; Namen aus [`file_stem`] sind schon ASCII ohne Anführungszeichen.
pub fn attachment(filename: &str) -> String {
    let safe: String = filename.chars().map(|c| if c.is_ascii_graphic() && c != '"' && c != '\\' { c } else { '_' }).collect();
    format!("attachment; filename=\"{safe}\"")
}

/// Schreibt in Blöcken in den Kanal; ein geschlossener Kanal (Download abgebrochen) wird zu `BrokenPipe`.
pub struct ChunkWriter {
    tx: mpsc::Sender<io::Result<Bytes>>,
    buf: Vec<u8>,
    sent: u64,
}

impl ChunkWriter {
    pub fn sent_bytes(&self) -> u64 {
        self.sent + self.buf.len() as u64
    }

    fn send(&mut self) -> io::Result<()> {
        if self.buf.is_empty() {
            return Ok(());
        }
        let chunk = Bytes::from(std::mem::replace(&mut self.buf, Vec::with_capacity(CHUNK)));
        self.sent += chunk.len() as u64;
        self.tx.blocking_send(Ok(chunk)).map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "Download abgebrochen"))
    }
}

impl Write for ChunkWriter {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        self.buf.extend_from_slice(data);
        if self.buf.len() >= CHUNK {
            self.send()?;
        }
        Ok(data.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.send()
    }
}

/// Antwortkörper, der die Blöcke aus dem Kanal liefert.
pub struct ChannelBody(mpsc::Receiver<io::Result<Bytes>>);

impl http_body::Body for ChannelBody {
    type Data = Bytes;
    type Error = io::Error;

    fn poll_frame(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Result<http_body::Frame<Bytes>, io::Error>>> {
        self.0.poll_recv(cx).map(|o| o.map(|r| r.map(http_body::Frame::data)))
    }
}

/// Kanal für einen Download: Schreiber für den Blocking-Thread, Körper für die Antwort und ein Sender für Fehler.
pub fn channel() -> (ChunkWriter, ChannelBody, mpsc::Sender<io::Result<Bytes>>) {
    let (tx, rx) = mpsc::channel(CHANNEL_DEPTH);
    (ChunkWriter { tx: tx.clone(), buf: Vec::with_capacity(CHUNK), sent: 0 }, ChannelBody(rx), tx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_and_stem() {
        assert_eq!(slug("Rund um den Großglockner – Tag 2!"), "Rund-um-den-Grossglockner-Tag-2");
        assert_eq!(slug("  Ölmühle  "), "Oelmuehle");
        assert_eq!(slug("***"), "");
        assert_eq!(slug(&"a".repeat(80)).len(), 60);
        let tz: chrono_tz::Tz = "Europe/Berlin".parse().unwrap();
        // 2026-07-01 08:15 UTC = 10:15 Sommerzeit
        assert_eq!(file_stem(1_782_893_700_000, tz, "Morgenrunde"), "2026-07-01-1015-Morgenrunde");
        assert_eq!(file_stem(1_782_893_700_000, tz, "!!"), "2026-07-01-1015");
    }

    #[test]
    fn unique_names() {
        let mut u = UniqueNames::default();
        assert_eq!(u.take("a", "gpx"), "a.gpx");
        assert_eq!(u.take("a", "gpx"), "a-2.gpx");
        assert_eq!(u.take("a", "gpx"), "a-3.gpx");
        assert_eq!(u.take("a", "fit"), "a.fit");
    }

    #[test]
    fn attachment_header() {
        assert_eq!(attachment("x y\"z.zip"), "attachment; filename=\"x_y_z.zip\"");
    }
}
