//! Binärformat der Messpunkte, Version 1 (RFC-0022): spaltenweise, Zigzag-Delta-Varint, kanonisch und streng geprüft.

/// Ein Messpunkt. Jedes Feld ist optional; Breite und Länge nur gemeinsam (RFC-0022 Abschnitt 2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Point {
    /// Millisekunden seit 1970-01-01 UTC
    pub t_ms: Option<i64>,
    /// (Breite, Länge) in Grad mal 10^7
    pub pos: Option<(i32, i32)>,
    /// Höhe in Dezimetern
    pub ele_dm: Option<i32>,
    /// Herzfrequenz in bpm, 1 bis 300
    pub hr: Option<u16>,
}

impl Point {
    pub fn is_empty(&self) -> bool {
        self.t_ms.is_none() && self.pos.is_none() && self.ele_dm.is_none() && self.hr.is_none()
    }
}

pub const FORMAT_VERSION: u8 = 1;
pub const MAX_POINTS: usize = 1_000_000;
pub const LAT_MAX: i32 = 900_000_000;
pub const LON_MAX: i32 = 1_800_000_000;
pub const HR_MIN: u16 = 1;
pub const HR_MAX: u16 = 300;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FormatError {
    #[error("Formatversion {0} unbekannt")]
    Version(u8),
    #[error("Punktzahl {0} außerhalb 1 bis 1.000.000")]
    Count(u64),
    #[error("Punktzahl {found} passt nicht zur Tabelle ({expected})")]
    CountMismatch { expected: usize, found: usize },
    #[error("unbekannter Spaltenmodus {0}")]
    Mode(u8),
    #[error("Daten abgeschnitten")]
    Truncated,
    #[error("Varint nicht minimal")]
    NonMinimalVarint,
    #[error("Varint zu lang")]
    VarintTooLong,
    #[error("Spaltenlänge passt nicht")]
    ColumnLength,
    #[error("Rest in einer Spalte")]
    ColumnTrailing,
    #[error("Rest am Ende")]
    Trailing,
    #[error("Bitmap nicht kanonisch (leer oder voll)")]
    NonCanonicalBitmap,
    #[error("Bits hinter dem letzten Punkt gesetzt")]
    BitmapOverhang,
    #[error("Koordinate außerhalb des Bereichs")]
    Coordinate,
    #[error("Herzfrequenz außerhalb 1 bis 300")]
    HeartRate,
    #[error("Höhe außerhalb des Wertebereichs")]
    Elevation,
}

/// Fehler beim Kodieren: nur unzulässige Eingaben (der Aufrufer prüft vorher).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EncodeError {
    #[error("Punktzahl {0} außerhalb 1 bis 1.000.000")]
    Count(usize),
    #[error("Punkt {0} ohne jeden Wert")]
    EmptyPoint(usize),
    #[error("Punkt {0}: Koordinate außerhalb des Bereichs")]
    Coordinate(usize),
    #[error("Punkt {0}: Herzfrequenz außerhalb 1 bis 300")]
    HeartRate(usize),
}

fn put_uvarint(out: &mut Vec<u8>, mut v: u64) {
    loop {
        let b = (v & 0x7F) as u8;
        v >>= 7;
        if v == 0 {
            out.push(b);
            return;
        }
        out.push(b | 0x80);
    }
}

fn zigzag(v: i64) -> u64 {
    ((v << 1) ^ (v >> 63)) as u64
}

fn unzigzag(u: u64) -> i64 {
    ((u >> 1) as i64) ^ -((u & 1) as i64)
}

/// Kodiert eine Punktfolge kanonisch: gleiche Punkte ergeben gleiche Bytes (RFC-0022 Abschnitt 5).
pub fn encode(points: &[Point]) -> Result<Vec<u8>, EncodeError> {
    let n = points.len();
    if n == 0 || n > MAX_POINTS {
        return Err(EncodeError::Count(n));
    }
    for (i, p) in points.iter().enumerate() {
        if p.is_empty() {
            return Err(EncodeError::EmptyPoint(i));
        }
        if let Some((la, lo)) = p.pos
            && (la.unsigned_abs() > LAT_MAX as u32 || lo.unsigned_abs() > LON_MAX as u32)
        {
            return Err(EncodeError::Coordinate(i));
        }
        if let Some(hr) = p.hr
            && !(HR_MIN..=HR_MAX).contains(&hr)
        {
            return Err(EncodeError::HeartRate(i));
        }
    }
    let mut out = Vec::with_capacity(8 * n + 16);
    out.push(FORMAT_VERSION);
    put_uvarint(&mut out, n as u64);
    column(&mut out, n, points.iter().map(|p| p.t_ms.map(|t| [t, 0])), 1);
    column(&mut out, n, points.iter().map(|p| p.pos.map(|(a, b)| [a as i64, b as i64])), 2);
    column(&mut out, n, points.iter().map(|p| p.ele_dm.map(|e| [e as i64, 0])), 1);
    column(&mut out, n, points.iter().map(|p| p.hr.map(|h| [h as i64, 0])), 1);
    Ok(out)
}

fn column(out: &mut Vec<u8>, n: usize, values: impl Iterator<Item = Option<[i64; 2]>>, streams: usize) {
    let values: Vec<Option<[i64; 2]>> = values.collect();
    let present: Vec<usize> = (0..n).filter(|&i| values[i].is_some()).collect();
    if present.is_empty() {
        out.push(0);
        return;
    }
    let mut payload = Vec::new();
    let mode = if present.len() == n {
        1
    } else {
        let mut bitmap = vec![0u8; n.div_ceil(8)];
        for &i in &present {
            bitmap[i >> 3] |= 1 << (i & 7);
        }
        payload.extend_from_slice(&bitmap);
        2
    };
    for s in 0..streams {
        let mut prev = 0i64;
        for &i in &present {
            let v = values[i].expect("vorhanden")[s];
            put_uvarint(&mut payload, zigzag(v.wrapping_sub(prev)));
            prev = v;
        }
    }
    out.push(mode);
    put_uvarint(out, payload.len() as u64);
    out.extend_from_slice(&payload);
}

struct Reader<'a> {
    b: &'a [u8],
    pos: usize,
    end: usize,
}

impl<'a> Reader<'a> {
    fn u8(&mut self) -> Result<u8, FormatError> {
        if self.pos >= self.end {
            return Err(FormatError::Truncated);
        }
        let v = self.b[self.pos];
        self.pos += 1;
        Ok(v)
    }

    fn uvarint(&mut self) -> Result<u64, FormatError> {
        let mut v: u64 = 0;
        let mut shift = 0u32;
        for n in 1..=10 {
            let c = self.u8()?;
            let low = (c & 0x7F) as u64;
            if shift == 63 && low > 1 {
                return Err(FormatError::VarintTooLong);
            }
            v |= low << shift;
            if c & 0x80 == 0 {
                if n > 1 && c == 0 {
                    return Err(FormatError::NonMinimalVarint);
                }
                return Ok(v);
            }
            shift += 7;
        }
        Err(FormatError::VarintTooLong)
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], FormatError> {
        if self.pos + n > self.end {
            return Err(FormatError::Truncated);
        }
        let r = &self.b[self.pos..self.pos + n];
        self.pos += n;
        Ok(r)
    }
}

/// Liest eine Spalte; liefert je Punkt die Werte der Ströme oder `None`.
fn read_column(r: &mut Reader, n: usize, streams: usize) -> Result<Vec<Option<[i64; 2]>>, FormatError> {
    let mode = r.u8()?;
    if mode == 0 {
        return Ok(vec![None; n]);
    }
    if mode != 1 && mode != 2 {
        return Err(FormatError::Mode(mode));
    }
    let len = r.uvarint()?;
    let end = r.pos.checked_add(usize::try_from(len).map_err(|_| FormatError::ColumnLength)?).ok_or(FormatError::ColumnLength)?;
    if end > r.end {
        return Err(FormatError::ColumnLength);
    }
    let mut sub = Reader { b: r.b, pos: r.pos, end };
    let idx: Vec<usize> = if mode == 1 {
        (0..n).collect()
    } else {
        let bm = sub.take(n.div_ceil(8))?;
        if !n.is_multiple_of(8) && bm[bm.len() - 1] >> (n % 8) != 0 {
            return Err(FormatError::BitmapOverhang);
        }
        let idx: Vec<usize> = (0..n).filter(|&i| bm[i >> 3] >> (i & 7) & 1 == 1).collect();
        if idx.is_empty() || idx.len() == n {
            return Err(FormatError::NonCanonicalBitmap);
        }
        idx
    };
    let mut res = vec![None; n];
    let mut vals = vec![[0i64; 2]; idx.len()];
    for s in 0..streams {
        let mut prev = 0i64;
        for v in vals.iter_mut() {
            prev = prev.wrapping_add(unzigzag(sub.uvarint()?));
            v[s] = prev;
        }
    }
    if sub.pos != end {
        return Err(FormatError::ColumnTrailing);
    }
    r.pos = end;
    for (k, &i) in idx.iter().enumerate() {
        res[i] = Some(vals[k]);
    }
    Ok(res)
}

/// Dekodiert und prüft streng (RFC-0022 Abschnitt 5). `expected` ist `point_count` aus der Tabelle.
pub fn decode(b: &[u8], expected: Option<usize>) -> Result<Vec<Point>, FormatError> {
    let mut r = Reader { b, pos: 0, end: b.len() };
    let version = r.u8()?;
    if version != FORMAT_VERSION {
        return Err(FormatError::Version(version));
    }
    let n64 = r.uvarint()?;
    if n64 == 0 || n64 > MAX_POINTS as u64 {
        return Err(FormatError::Count(n64));
    }
    let n = n64 as usize;
    if let Some(e) = expected
        && e != n
    {
        return Err(FormatError::CountMismatch { expected: e, found: n });
    }
    let t = read_column(&mut r, n, 1)?;
    let pos = read_column(&mut r, n, 2)?;
    let ele = read_column(&mut r, n, 1)?;
    let hr = read_column(&mut r, n, 1)?;
    if r.pos != r.end {
        return Err(FormatError::Trailing);
    }
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let pos = match pos[i] {
            Some([la, lo]) => {
                if la.unsigned_abs() > LAT_MAX as u64 || lo.unsigned_abs() > LON_MAX as u64 {
                    return Err(FormatError::Coordinate);
                }
                Some((la as i32, lo as i32))
            }
            None => None,
        };
        let hr = match hr[i] {
            Some([h, _]) => {
                if !(HR_MIN as i64..=HR_MAX as i64).contains(&h) {
                    return Err(FormatError::HeartRate);
                }
                Some(h as u16)
            }
            None => None,
        };
        let ele_dm = match ele[i] {
            Some([e, _]) => Some(i32::try_from(e).map_err(|_| FormatError::Elevation)?),
            None => None,
        };
        out.push(Point { t_ms: t[i].map(|v| v[0]), pos, ele_dm, hr });
    }
    Ok(out)
}

/// Rundet auf die nächste ganze Einheit, Hälften weg von null (RFC-0022 Abschnitt 2).
pub fn round_half_away(x: f64) -> i64 {
    (x.abs() + 0.5).floor().copysign(x) as i64
}

/// Grad in Einheiten von 10^-7 Grad.
pub fn deg_to_e7(deg: f64) -> i32 {
    round_half_away(deg * 1e7) as i32
}

pub fn e7_to_deg(v: i32) -> f64 {
    v as f64 / 1e7
}
