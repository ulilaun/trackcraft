//! Segmentmatching nach RFC-0002: zweistufig, rein (ohne Datenbank), je Aufzeichnungsabschnitt.
//!
//! Rechnet in einer lokalen ebenen Projektion um den Segmentstart (Meter, Ost/Nord). Über die wenigen Kilometer
//! eines Segments weicht sie von der WGS84-Distanz um weniger als 0,1 % ab. Präzisierungen gegenüber RFC-0002,
//! die dort in Abschnitt 4.7 beschrieben sind:
//!
//! - Ein Kandidat ist ein Paar aus Start- und späterer Zielpassage. Zu jeder Zielpassage gehört die letzte
//!   Startpassage davor (4.4), die nicht vor dem Ziel des vorigen Versuchs liegt. So ergeben n Runden mit
//!   gemeinsamer Passage n Versuche (4.5).
//! - Richtung an Start und Ziel: die Trackkante, die die Querlinie kreuzt (nicht eine Sehne über drei Punkte;
//!   liegt das Ziel in einer Kurve, weicht die Sehne zu stark ab, Golden-Segment R2).
//! - Abdeckung und innere Lücke werden auf Referenzproben im Abstand von 1 m gemessen.
//! - Reihenfolge und Rückschritt: Jeder Trackpunkt erhält die früheste Referenzstelle innerhalb der Toleranz,
//!   die nicht mehr als `max_backstep_m` hinter dem bisherigen Stand liegt. Ein Rückschritt liegt vor, wenn ein
//!   Punkt nur noch zu Stellen weiter hinten passt; auf gerader Strecke erst etwa ab 2 · Toleranz plus
//!   `max_backstep_m` (31 m). Die Reihenfolge gilt, wenn sie mit oder ohne Versatzausgleich besteht.
//! - Ein Trackstück außerhalb der Toleranz, das um mehr als 2 · `max_inner_gap_m` länger ist als das Stück
//!   Referenz, das es überspringt (Schleife, Umweg mit Rückkehr), ist mehrdeutig (4.6).

use std::collections::HashMap;
use std::ops::{Add, Mul, Sub};

use crate::config::MatchingSection;
use crate::domain::track::{Point, e7_to_deg};

/// Meter je Grad auf dem Äquator (WGS84, a·π/180).
const M_PER_DEG: f64 = 111_319.490_793_273_57;
/// Abstand der Referenzproben für Abdeckung und Lücken.
const SAMPLE_STEP_M: f64 = 1.0;
/// Abstand der Referenzproben für den Versatzausgleich.
const OFFSET_STEP_M: f64 = 5.0;
/// Durchläufe des Versatzausgleichs (RFC-0002 4.2).
const OFFSET_ITERATIONS: usize = 3;
/// Spielraum in Trackpunkten, in dem Start und Ziel nach dem Versatzausgleich neu bestimmt werden.
const RECROSS_WINDOW: usize = 30;
/// Zellgröße des Rasters über Trackkanten (Meter).
const GRID_CELL_M: f64 = 32.0;
/// Abstand der Grobproben für die schnelle Absage vor dem Versatzausgleich.
const PRECHECK_STEP_M: f64 = 25.0;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
struct V {
    x: f64,
    y: f64,
}

impl V {
    fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
    fn dot(self, o: V) -> f64 {
        self.x * o.x + self.y * o.y
    }
    fn cross(self, o: V) -> f64 {
        self.x * o.y - self.y * o.x
    }
    fn len(self) -> f64 {
        self.x.hypot(self.y)
    }
}

impl Add for V {
    type Output = V;
    fn add(self, o: V) -> V {
        V::new(self.x + o.x, self.y + o.y)
    }
}

impl Sub for V {
    type Output = V;
    fn sub(self, o: V) -> V {
        V::new(self.x - o.x, self.y - o.y)
    }
}

impl Mul<f64> for V {
    type Output = V;
    fn mul(self, k: f64) -> V {
        V::new(self.x * k, self.y * k)
    }
}

/// Nächster Punkt auf der Strecke a–b und Anteil entlang der Strecke.
fn nearest_on_edge(p: V, a: V, b: V) -> (V, f64) {
    let ab = b - a;
    let l2 = ab.dot(ab);
    let t = if l2 == 0.0 { 0.0 } else { ((p - a).dot(ab) / l2).clamp(0.0, 1.0) };
    (a + ab * t, t)
}

/// Lokale ebene Projektion (Meter) um einen Ursprung.
#[derive(Debug, Clone, Copy)]
struct Projection {
    lat0: f64,
    lon0: f64,
    kx: f64,
}

impl Projection {
    fn new(lat0: f64, lon0: f64) -> Self {
        Self { lat0, lon0, kx: lat0.to_radians().cos() * M_PER_DEG }
    }
    fn xy(&self, lat: f64, lon: f64) -> V {
        V::new((lon - self.lon0) * self.kx, (lat - self.lat0) * M_PER_DEG)
    }
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ReferenceError {
    #[error("Referenz braucht mindestens zwei verschiedene GPS-Punkte")]
    TooFewPoints,
    #[error("Richtung an Start oder Ziel nicht bestimmbar")]
    Direction,
}

/// Referenzgeometrie eines Segments in Aufzeichnungsrichtung.
#[derive(Debug, Clone)]
pub struct Reference {
    proj: Projection,
    pts: Vec<V>,
    /// Bogenlänge bis zu jedem Punkt
    cum: Vec<f64>,
    length: f64,
    dir_start: V,
    dir_end: V,
    min: V,
    max: V,
    /// Strecke: Beginn und Ende einer Aufzeichnung nahe Start bzw. Ziel zählen als Passage (RFC-0002 4.3)
    open_ends: bool,
}

impl Reference {
    /// Aus Positionen in Grad (Breite, Länge). Aufeinanderfolgende gleiche Punkte zählen einmal.
    pub fn new(points: &[(f64, f64)], params: &MatchingSection) -> Result<Self, ReferenceError> {
        let first = points.first().ok_or(ReferenceError::TooFewPoints)?;
        let proj = Projection::new(first.0, first.1);
        let mut pts: Vec<V> = Vec::with_capacity(points.len());
        for &(lat, lon) in points {
            let p = proj.xy(lat, lon);
            if pts.last() != Some(&p) {
                pts.push(p);
            }
        }
        if pts.len() < 2 {
            return Err(ReferenceError::TooFewPoints);
        }
        let mut cum = Vec::with_capacity(pts.len());
        let mut acc = 0.0;
        cum.push(0.0);
        for w in pts.windows(2) {
            acc += (w[1] - w[0]).len();
            cum.push(acc);
        }
        let length = acc;
        let mut min = pts[0];
        let mut max = pts[0];
        for p in &pts {
            min = V::new(min.x.min(p.x), min.y.min(p.y));
            max = V::new(max.x.max(p.x), max.y.max(p.y));
        }
        let mut r = Self { proj, pts, cum, length, dir_start: V::default(), dir_end: V::default(), min, max, open_ends: false };
        // Richtungsbezug höchstens die Segmentlänge (RFC-0002 Abschnitt 3)
        let d = params.direction_reference_m.min(length);
        let ds = r.point_at(d) - r.pts[0];
        let de = *r.pts.last().expect("zwei Punkte") - r.point_at(length - d);
        if ds.len() < 1e-6 || de.len() < 1e-6 {
            return Err(ReferenceError::Direction);
        }
        r.dir_start = ds * (1.0 / ds.len());
        r.dir_end = de * (1.0 / de.len());
        Ok(r)
    }

    /// Für Strecken (`kind = route`): Aufzeichnungsrand innerhalb `recording_edge_m` gilt als Passage.
    pub fn with_open_ends(mut self, open: bool) -> Self {
        self.open_ends = open;
        self
    }

    /// Länge in der lokalen Ebene (Meter).
    pub fn length_m(&self) -> f64 {
        self.length
    }

    fn start(&self) -> V {
        self.pts[0]
    }

    fn end(&self) -> V {
        *self.pts.last().expect("zwei Punkte")
    }

    /// Punkt bei Bogenlänge `s`.
    fn point_at(&self, s: f64) -> V {
        let s = s.clamp(0.0, self.length);
        let i = match self.cum.binary_search_by(|c| c.total_cmp(&s)) {
            Ok(i) => return self.pts[i],
            Err(i) => i.clamp(1, self.pts.len() - 1),
        };
        let (a, b) = (self.pts[i - 1], self.pts[i]);
        let seg = self.cum[i] - self.cum[i - 1];
        if seg == 0.0 { a } else { a + (b - a) * ((s - self.cum[i - 1]) / seg) }
    }

    /// Proben im Abstand `step` einschließlich des Endpunkts.
    fn samples(&self, step: f64) -> Vec<V> {
        let n = (self.length / step).ceil().max(1.0) as usize;
        (0..=n).map(|k| self.point_at(self.length * k as f64 / n as f64)).collect()
    }

    /// Früheste Bogenlänge in [lo, hi], deren Referenzpunkt höchstens `tol` von `p` entfernt ist.
    fn first_within(&self, p: V, lo: f64, hi: f64, tol: f64) -> Option<f64> {
        if hi < lo {
            return None;
        }
        let first = match self.cum.binary_search_by(|c| c.total_cmp(&lo)) {
            Ok(i) | Err(i) => i.saturating_sub(1),
        };
        for i in first..self.pts.len() - 1 {
            if self.cum[i] > hi {
                break;
            }
            let (a, b) = (self.pts[i], self.pts[i + 1]);
            let ab = b - a;
            let l2 = ab.dot(ab);
            if l2 == 0.0 {
                continue;
            }
            // |a + t·ab − p|² ≤ tol²: quadratische Ungleichung in t
            let ap = a - p;
            let bq = 2.0 * ap.dot(ab);
            let cq = ap.dot(ap) - tol * tol;
            let disc = bq * bq - 4.0 * l2 * cq;
            if disc < 0.0 {
                continue;
            }
            let root = disc.sqrt();
            let t0 = ((-bq - root) / (2.0 * l2)).max(0.0);
            let t1 = ((-bq + root) / (2.0 * l2)).min(1.0);
            if t0 > t1 {
                continue;
            }
            let len = self.cum[i + 1] - self.cum[i];
            let (s0, s1) = (self.cum[i] + t0 * len, self.cum[i] + t1 * len);
            if s1 < lo || s0 > hi {
                continue;
            }
            return Some(s0.max(lo));
        }
        None
    }
}

/// Grund, warum ein Versuch nicht gewertet wird (RFC-0002 Abschnitt 6, Schema `not_ranked_reason`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotRanked {
    AmbiguousGeometry,
    TimeInvalid,
}

impl NotRanked {
    pub fn code(self) -> &'static str {
        match self {
            Self::AmbiguousGeometry => "ambiguous_geometry",
            Self::TimeInvalid => "time_invalid",
        }
    }
}

/// Ein Versuch eines Abschnitts auf einem Segment.
#[derive(Debug, Clone, PartialEq)]
pub struct Attempt {
    /// Stabile Position innerhalb des Abschnitts (0, 1, …), Tie-Break der Rangliste
    pub position: u32,
    /// `None`: gewertet
    pub not_ranked: Option<NotRanked>,
    /// Start und Ziel auf ms gerundet, halbe ms aufgerundet; nur mit belastbarer Zeit
    pub start_ms: Option<i64>,
    pub end_ms: Option<i64>,
    pub duration_ms: Option<i64>,
    /// Indizes der Messpunkte des Abschnitts vor der Start- und nach der Zielpassage
    pub start_index: usize,
    pub end_index: usize,
    pub start_deviation_m: f64,
    pub end_deviation_m: f64,
    pub start_angle_deg: f64,
    pub end_angle_deg: f64,
    pub coverage_ratio: f64,
    pub offset_m: f64,
    pub avg_heart_rate_bpm: Option<u16>,
    pub max_heart_rate_bpm: Option<u16>,
}

impl Attempt {
    pub fn is_valid(&self) -> bool {
        self.not_ranked.is_none()
    }
}

/// Trackpunkt mit Position; `idx` ist der Index im Abschnitt.
#[derive(Debug, Clone, Copy)]
struct Tp {
    p: V,
    t: Option<i64>,
    idx: usize,
}

/// Passage einer Start- oder Zielquerlinie.
#[derive(Debug, Clone, Copy)]
struct Crossing {
    /// Kante (Index in der Trackpunktliste) und Anteil darauf
    edge: usize,
    frac: f64,
    lateral: f64,
    angle_deg: f64,
}

impl Crossing {
    fn key(&self) -> f64 {
        self.edge as f64 + self.frac
    }
}

/// Wertet einen Aufzeichnungsabschnitt gegen ein Segment aus.
pub fn match_section(reference: &Reference, section: &[Point], params: &MatchingSection) -> Vec<Attempt> {
    run(reference, section, params, &mut None)
}

/// Wie `match_section`, schreibt aber zu jedem Kandidaten, warum er (k)ein Versuch wurde (Fehlersuche, Tests).
pub fn diagnose(reference: &Reference, section: &[Point], params: &MatchingSection) -> (Vec<Attempt>, Vec<String>) {
    let mut log = Some(vec![]);
    let attempts = run(reference, section, params, &mut log);
    (attempts, log.unwrap_or_default())
}

fn run(reference: &Reference, section: &[Point], params: &MatchingSection, log: &mut Option<Vec<String>>) -> Vec<Attempt> {
    let track = positioned(reference, section);
    if track.len() < 2 || !near_bbox(reference, &track, params) {
        return vec![];
    }
    let none = V::default();
    let mut starts = crossings(&track, none, reference.start(), reference.dir_start, 0, track.len() - 1, params);
    let mut ends = crossings(&track, none, reference.end(), reference.dir_end, 0, track.len() - 1, params);
    if reference.open_ends {
        if let Some(c) = recording_edge(&track, reference.start(), reference.dir_start, true, params) {
            starts.insert(0, c);
        }
        if let Some(c) = recording_edge(&track, reference.end(), reference.dir_end, false, params) {
            ends.push(c);
        }
    }
    if let Some(l) = log.as_mut() {
        l.push(format!("Startpassagen {}, Zielpassagen {}", starts.len(), ends.len()));
        let loose = MatchingSection { matching_radius_meters: 100.0, max_vector_angle_deg: 90.0, ..params.clone() };
        for (name, origin, dir) in [("Start", reference.start(), reference.dir_start), ("Ziel", reference.end(), reference.dir_end)] {
            for c in crossings(&track, none, origin, dir, 0, track.len() - 1, &loose) {
                let ed = track[(c.edge + 1).min(track.len() - 1)].p - track[c.edge].p;
                let edge_deg = (ed.dot(dir) / ed.len()).clamp(-1.0, 1.0).acos().to_degrees();
                let gap = (track[(c.edge + 1).min(track.len() - 1)].p - track[c.edge].p).len();
                l.push(format!(
                    "  {name}linie bei Punkt {}: seitlich {:.1} m, Winkel Sehne {:.0}°, Kante {:.0}° ({:.0} m)",
                    track[c.edge].idx, c.lateral, c.angle_deg, edge_deg, gap
                ));
            }
        }
    }
    let mut out = vec![];
    let mut used = f64::NEG_INFINITY;
    for e in &ends {
        let Some(s) = starts.iter().rev().find(|s| s.key() >= used && s.key() < e.key()) else {
            continue;
        };
        match evaluate(reference, &track, section, *s, *e, params) {
            Ok(mut a) => {
                a.position = out.len() as u32;
                if let Some(l) = log.as_mut() {
                    l.push(format!(
                        "Punkte {}–{}: Versuch {:?}, Abdeckung {:.3}, Versatz {:.1} m",
                        track[s.edge].idx, track[e.edge].idx, a.not_ranked, a.coverage_ratio, a.offset_m
                    ));
                }
                out.push(a);
                used = e.key();
            }
            Err(why) => {
                if let Some(l) = log.as_mut() {
                    l.push(format!("Punkte {}–{}: kein Versuch, {why}", track[s.edge].idx, track[e.edge].idx));
                }
            }
        }
    }
    out
}

/// Punkte mit Position in Referenzprojektion; bei gleichem Zeitstempel zählt der erste (RFC-0002 Abschnitt 5).
fn positioned(reference: &Reference, section: &[Point]) -> Vec<Tp> {
    let mut out: Vec<Tp> = Vec::with_capacity(section.len());
    let mut last_t: Option<i64> = None;
    for (idx, pt) in section.iter().enumerate() {
        let Some((lat, lon)) = pt.pos else { continue };
        if pt.t_ms.is_some() && pt.t_ms == last_t {
            continue;
        }
        if pt.t_ms.is_some() {
            last_t = pt.t_ms;
        }
        out.push(Tp { p: reference.proj.xy(e7_to_deg(lat), e7_to_deg(lon)), t: pt.t_ms, idx });
    }
    out
}

/// Schnelle Vorprüfung: liegt der Track überhaupt in der Nähe der Referenz?
fn near_bbox(r: &Reference, track: &[Tp], params: &MatchingSection) -> bool {
    let m = params.matching_radius_meters + params.max_offset_m + params.tolerance_m;
    track.iter().any(|q| q.p.x >= r.min.x - m && q.p.x <= r.max.x + m && q.p.y >= r.min.y - m && q.p.y <= r.max.y + m)
}

/// Passagen der Querlinie durch `origin` senkrecht zu `dir` in Richtung `dir`, auf Kanten lo..=hi.
///
/// Gezählt wird nur ein echter Seitenwechsel von hinten (negativ) nach vorn (positiv). Punkte genau auf der
/// Linie werden übersprungen: Setzt der Track danach vorn fort, ist es eine Passage am ersten Punkt auf der
/// Linie, kehrt er zurück, nur ein Streifen (RFC-0002 4.3).
fn crossings(track: &[Tp], shift: V, origin: V, dir: V, lo: usize, hi: usize, params: &MatchingSection) -> Vec<Crossing> {
    let side = |i: usize| (track[i].p - shift - origin).dot(dir);
    let cos_max = params.max_vector_angle_deg.to_radians().cos();
    let mut out = vec![];
    let mut last_neg: Option<usize> = None;
    for i in lo..=hi.min(track.len() - 1) {
        let s = side(i);
        if s < 0.0 {
            last_neg = Some(i);
        } else if s > 0.0
            && let Some(j) = last_neg.take()
        {
            let (edge, frac) = if i == j + 1 {
                let sj = side(j);
                (j, -sj / (s - sj))
            } else {
                (j + 1, 0.0)
            };
            let at = point_on(track, shift, edge, frac);
            let lateral = (at - origin).cross(dir).abs();
            if lateral > params.matching_radius_meters {
                continue;
            }
            // Trackvektor: Richtung der Kante, die die Linie kreuzt (RFC-0002 Abschnitt 3); bei einer Passage
            // an einem Punkt auf der Linie die Kante vom letzten Punkt davor zum ersten danach
            let chord = track[i].p - track[j].p;
            if chord.len() == 0.0 {
                continue;
            }
            let cos = (chord.dot(dir) / chord.len()).clamp(-1.0, 1.0);
            if cos < cos_max {
                continue;
            }
            out.push(Crossing { edge, frac, lateral, angle_deg: cos.acos().to_degrees() });
        }
    }
    out
}

/// Strecken: Beginn (`first`) bzw. Ende der Aufzeichnung als Passage, wenn der Punkt höchstens
/// `recording_edge_m` vom Kontrollpunkt entfernt liegt und die erste bzw. letzte Kante in Segmentrichtung zeigt.
fn recording_edge(track: &[Tp], origin: V, dir: V, first: bool, params: &MatchingSection) -> Option<Crossing> {
    let n = track.len();
    let (k, a, b) = if first { (0, track[0].p, track[1].p) } else { (n - 1, track[n - 2].p, track[n - 1].p) };
    let dist = (track[k].p - origin).len();
    let edge = b - a;
    if dist > params.recording_edge_m || edge.len() == 0.0 {
        return None;
    }
    let cos = (edge.dot(dir) / edge.len()).clamp(-1.0, 1.0);
    if cos < params.max_vector_angle_deg.to_radians().cos() {
        return None;
    }
    Some(Crossing { edge: k, frac: 0.0, lateral: dist, angle_deg: cos.acos().to_degrees() })
}

fn point_on(track: &[Tp], shift: V, edge: usize, frac: f64) -> V {
    let a = track[edge].p - shift;
    if frac == 0.0 || edge + 1 >= track.len() {
        return a;
    }
    a + (track[edge + 1].p - shift - a) * frac
}

/// Schnelle Absage ohne Versatzausgleich, ohne je einen echten Versuch abzulehnen: Ein Referenzpunkt, der weiter
/// als `tolerance_m + max_offset_m` von allen Kanten um den Kandidaten liegt, bleibt bei jeder erlaubten
/// Verschiebung unbedeckt, und mit ihm jeder Punkt der Referenz innerhalb des Überschusses (der Abstand ändert sich
/// je Meter Referenz um höchstens einen Meter). Grobproben alle 25 m zählen so sicher unbedeckte 1-m-Proben; sind
/// das mehr, als die Mindestabdeckung erlaubt, kann der Kandidat kein Versuch werden. Lange Fahrten, die Start und
/// Ziel einer Strecke kreuzen und dazwischen woanders fahren, kosteten sonst Sekunden.
fn surely_uncovered(r: &Reference, track: &[Tp], s0: Crossing, e0: Crossing, params: &MatchingSection) -> bool {
    // Obermenge der Kanten, die nach dem Versatzausgleich zählen können (Start und Ziel verschieben sich höchstens
    // um RECROSS_WINDOW)
    let lo = s0.edge.saturating_sub(RECROSS_WINDOW);
    let hi = (e0.edge + 1 + RECROSS_WINDOW).min(track.len() - 1);
    let edges: Vec<usize> = (lo..hi).collect();
    if edges.is_empty() {
        return false;
    }
    let grid = EdgeGrid::new(track, &edges);
    let reach = params.tolerance_m + params.max_offset_m;
    let samples = r.samples(SAMPLE_STEP_M).len();
    let step1 = r.length / (samples - 1).max(1) as f64;
    let n = (r.length / PRECHECK_STEP_M).ceil().max(1.0) as usize;
    // Halber Grobabstand minus eine Probe: die gezählten Bereiche zweier Grobproben überlappen nicht
    let half = r.length / n as f64 / 2.0 - step1;
    if half <= 0.0 {
        return false;
    }
    // Ab so vielen sicher unbedeckten Proben ist die Mindestabdeckung verfehlt
    let limit = samples as f64 * (1.0 - params.min_coverage_ratio);
    let mut out = 0usize;
    for k in 0..=n {
        let p = r.point_at(r.length * k as f64 / n as f64);
        // Nur Abstände bis reach + half zählen; weiter weg gilt der volle halbe Abstand
        let d = grid.distance_within(track, p, reach + half).unwrap_or(f64::INFINITY);
        let excess = (d - reach - 1e-6).min(half);
        if excess > 0.0 {
            let side = (excess / step1).floor() as usize;
            // Probe selbst und je Seite die vollen Schritte innerhalb des Überschusses (an den Enden nur nach innen)
            out += 1 + if k == 0 || k == n { side } else { 2 * side };
            if out as f64 > limit + 1e-9 {
                return true;
            }
        }
    }
    false
}

/// Größte erlaubte innere Lücke: mindestens `max_inner_gap_m`, bei langen Segmenten ein Anteil der Länge.
fn inner_gap(r: &Reference, params: &MatchingSection) -> f64 {
    params.max_inner_gap_m.max(params.max_inner_gap_ratio * r.length)
}

/// Stufe 2 für ein Kandidatenpaar; `Err` mit Grund, wenn es kein Versuch ist.
fn evaluate(
    r: &Reference,
    track: &[Tp],
    section: &[Point],
    s0: Crossing,
    e0: Crossing,
    params: &MatchingSection,
) -> Result<Attempt, String> {
    let conn_ms = params.coverage_connection_s as i64 * 1000;
    let connected = |i: usize| match (track[i].t, track[i + 1].t) {
        (Some(a), Some(b)) => b - a <= conn_ms,
        _ => true,
    };

    if surely_uncovered(r, track, s0, e0, params) {
        return Err("Abdeckung sicher zu gering (Vorprüfung)".into());
    }

    // Versatzausgleich (RFC-0002 4.2)
    let mut shift = V::default();
    let probes = r.samples(OFFSET_STEP_M);
    let offset_edges: Vec<usize> = (s0.edge..=e0.edge.min(track.len() - 2)).filter(|&i| connected(i)).collect();
    let offset_grid = EdgeGrid::new(track, &offset_edges);
    for _ in 0..OFFSET_ITERATIONS {
        if offset_edges.is_empty() {
            break;
        }
        let mut sum = V::default();
        for p in &probes {
            let q = offset_grid.nearest(track, shift, *p).expect("Kanten vorhanden");
            sum = sum + (q - *p);
        }
        shift = shift + sum * (1.0 / probes.len() as f64);
        let m = shift.len();
        if m > params.max_offset_m {
            shift = shift * (params.max_offset_m / m);
        }
    }

    // Start und Ziel für den verschobenen Track neu bestimmen
    let lo = s0.edge.saturating_sub(RECROSS_WINDOW);
    let hi = (e0.edge + 1 + RECROSS_WINDOW).min(track.len() - 1);
    // jeweils die Passage nächst der ursprünglichen, höchstens RECROSS_WINDOW Kanten entfernt
    let closest = |list: Vec<Crossing>, to: f64, before: f64| {
        list.into_iter()
            .filter(|c| c.key() < before && (c.key() - to).abs() <= RECROSS_WINDOW as f64)
            .min_by(|a, b| (a.key() - to).abs().total_cmp(&(b.key() - to).abs()))
    };
    let (mut s, mut e) = (s0, e0);
    if let Some(e2) = closest(crossings(track, shift, r.end(), r.dir_end, lo, hi, params), e0.key(), f64::INFINITY)
        && let Some(s2) = closest(crossings(track, shift, r.start(), r.dir_start, lo, hi, params), s0.key(), e2.key())
    {
        s = s2;
        e = e2;
    }

    // Abdeckung und innere Lücke auf 1-m-Proben
    let edges: Vec<usize> = (s.edge..=e.edge.min(track.len() - 2)).filter(|&i| connected(i)).collect();
    if edges.is_empty() {
        return Err("keine zusammenhängende Kante".into());
    }
    let samples = r.samples(SAMPLE_STEP_M);
    let step = r.length / (samples.len() - 1) as f64;
    let grid = EdgeGrid::new(track, &edges);
    let mut covered = 0usize;
    let mut run = 0usize;
    let mut longest = 0usize;
    for p in &samples {
        let near = grid.any_within(track, shift, *p, params.tolerance_m);
        if near {
            covered += 1;
            run = 0;
        } else {
            run += 1;
            longest = longest.max(run);
        }
    }
    let coverage = covered as f64 / samples.len() as f64;
    let longest_m = longest as f64 * step;
    if coverage < params.min_coverage_ratio || longest_m > inner_gap(r, params) + 1e-9 {
        return Err(format!("Abdeckung {coverage:.3}, größte Lücke {longest_m:.0} m, Versatz {:.1} m", shift.len()));
    }

    // Reihenfolge, Rückschritt und Schleifen entlang der Referenz
    // Der Versatzausgleich ist ein Mittelwert über das ganze Segment; fährt der Track nur einen Teil parallel,
    // verzerrt er die Lage an Kehren. Die Reihenfolge gilt daher, wenn sie mit oder ohne Ausgleich besteht.
    let geometry = follow(r, track, shift, s, e, params)
        .or_else(|err| follow(r, track, V::default(), s, e, params).map_err(|err0| format!("{err}; ohne Versatzausgleich: {err0}")))?;

    // Zeit (RFC-0002 Abschnitt 5) vor Mehrdeutigkeit vor Wertung
    let time = attempt_time(track, s, e, params);
    let not_ranked = match (&time, geometry) {
        (None, _) => Some(NotRanked::TimeInvalid),
        (Some(_), Geometry::Ambiguous) => Some(NotRanked::AmbiguousGeometry),
        (Some(_), Geometry::Clear) => None,
    };
    let (start_ms, end_ms) = match time {
        Some((a, b)) => (Some(a), Some(b)),
        None => (None, None),
    };
    let (avg_hr, max_hr) = match (start_ms, end_ms) {
        (Some(a), Some(b)) => heart_rate(section, a, b),
        _ => (None, None),
    };
    let duration = match (start_ms, end_ms) {
        (Some(a), Some(b)) if b > a => Some(b - a),
        _ => None,
    };
    // Dauer 0 oder negativ ist keine belastbare Zeit
    let not_ranked = if not_ranked.is_none() && duration.is_none() { Some(NotRanked::TimeInvalid) } else { not_ranked };
    Ok(Attempt {
        position: 0,
        not_ranked,
        start_ms,
        end_ms,
        duration_ms: duration,
        start_index: track[s.edge].idx,
        end_index: track[(e.edge + 1).min(track.len() - 1)].idx,
        start_deviation_m: s.lateral,
        end_deviation_m: e.lateral,
        start_angle_deg: s.angle_deg,
        end_angle_deg: e.angle_deg,
        coverage_ratio: coverage.min(1.0),
        offset_m: shift.len(),
        avg_heart_rate_bpm: avg_hr,
        max_heart_rate_bpm: max_hr,
    })
}

/// Raster über ausgewählte Trackkanten (unverschoben) für Nachbarschaftsabfragen.
///
/// Jede Kante steht in allen Zellen ihres umschließenden Rechtecks. Abstände werden genau wie bei der
/// vollständigen Suche über die verschobenen Kanten berechnet; das Raster wählt nur die Kandidaten aus, die
/// Ergebnisse sind daher gleich (bei gleichem Abstand gewinnt wie dort die frühere Kante).
struct EdgeGrid {
    edges: Vec<usize>,
    cells: HashMap<(i64, i64), Vec<usize>>,
    lo: (i64, i64),
    hi: (i64, i64),
}

impl EdgeGrid {
    fn cell(p: V) -> (i64, i64) {
        ((p.x / GRID_CELL_M).floor() as i64, (p.y / GRID_CELL_M).floor() as i64)
    }

    fn new(track: &[Tp], edges: &[usize]) -> Self {
        let mut cells: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
        let (mut lo, mut hi) = ((i64::MAX, i64::MAX), (i64::MIN, i64::MIN));
        for &i in edges {
            let (a, b) = (Self::cell(track[i].p), Self::cell(track[i + 1].p));
            let (x0, x1) = (a.0.min(b.0), a.0.max(b.0));
            let (y0, y1) = (a.1.min(b.1), a.1.max(b.1));
            lo = (lo.0.min(x0), lo.1.min(y0));
            hi = (hi.0.max(x1), hi.1.max(y1));
            for x in x0..=x1 {
                for y in y0..=y1 {
                    cells.entry((x, y)).or_default().push(i);
                }
            }
        }
        Self { edges: edges.to_vec(), cells, lo, hi }
    }

    fn distance(track: &[Tp], shift: V, i: usize, p: V) -> (f64, V) {
        let (q, _) = nearest_on_edge(p, track[i].p - shift, track[i + 1].p - shift);
        ((q - p).len(), q)
    }

    /// Kleinster Abstand von `p` zu einer Kante, wenn er höchstens `limit` beträgt (unverschobener Track); sonst `None`.
    fn distance_within(&self, track: &[Tp], p: V, limit: f64) -> Option<f64> {
        let m = limit + 1.0;
        let (a, b) = (Self::cell(p - V::new(m, m)), Self::cell(p + V::new(m, m)));
        let mut best: Option<f64> = None;
        for x in a.0..=b.0 {
            for y in a.1..=b.1 {
                for &i in self.cells.get(&(x, y)).into_iter().flatten() {
                    let d = Self::distance(track, V::default(), i, p).0;
                    if d <= limit && best.is_none_or(|b| d < b) {
                        best = Some(d);
                    }
                }
            }
        }
        best
    }

    /// Liegt eine Kante höchstens `tol` von `p` entfernt (Track um `shift` verschoben)?
    fn any_within(&self, track: &[Tp], shift: V, p: V, tol: f64) -> bool {
        // 1 m Spielraum gegen Rundung beim Verschieben
        let m = tol + 1.0;
        let q = p + shift;
        let (a, b) = (Self::cell(q - V::new(m, m)), Self::cell(q + V::new(m, m)));
        for x in a.0..=b.0 {
            for y in a.1..=b.1 {
                if let Some(list) = self.cells.get(&(x, y))
                    && list.iter().any(|&i| Self::distance(track, shift, i, p).0 <= tol)
                {
                    return true;
                }
            }
        }
        false
    }

    /// Nächster Punkt auf den Kanten (Track um `shift` verschoben); `None` ohne Kanten.
    fn nearest(&self, track: &[Tp], shift: V, p: V) -> Option<V> {
        if self.cells.is_empty() {
            return None;
        }
        let c = Self::cell(p + shift);
        // Ringe um die Zelle des Punkts, bis keine weitere Zelle näher liegen kann
        let reach = (c.0 - self.lo.0).abs().max((self.hi.0 - c.0).abs()).max((c.1 - self.lo.1).abs()).max((self.hi.1 - c.1).abs());
        let mut best: Option<(f64, usize, V)> = None;
        let consider = |best: &mut Option<(f64, usize, V)>, i: usize| {
            let (d, q) = Self::distance(track, shift, i, p);
            if best.is_none_or(|(bd, bi, _)| d < bd || (d == bd && i < bi)) {
                *best = Some((d, i, q));
            }
        };
        // Bisher angesehene Zellen; die Ringe wachsen quadratisch, alle Kanten direkt kosten nur ihre Anzahl
        let mut scanned = 0usize;
        for k in 0..=reach {
            scanned += (8 * k as usize).max(1);
            // Weit entfernte Kanten (Punkt fern vom Kandidaten, lange Referenz): alle Kanten direkt prüfen
            if scanned > self.edges.len() {
                self.edges.iter().for_each(|&i| consider(&mut best, i));
                break;
            }
            let ring = (c.0 - k..=c.0 + k)
                .flat_map(|x| [(x, c.1 - k), (x, c.1 + k)])
                .chain((c.1 - k + 1..c.1 + k).flat_map(|y| [(c.0 - k, y), (c.0 + k, y)]));
            for cell in ring {
                if let Some(list) = self.cells.get(&cell) {
                    list.iter().for_each(|&i| consider(&mut best, i));
                }
            }
            // Zellen im nächsten Ring liegen mindestens k Zellbreiten entfernt (1 m Spielraum gegen Rundung)
            if best.is_some_and(|(d, _, _)| d + 1.0 < k as f64 * GRID_CELL_M) {
                break;
            }
        }
        best.map(|(_, _, q)| q)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Geometry {
    Clear,
    Ambiguous,
}

/// Folgt dem verschobenen Track vorwärts entlang der Referenz (RFC-0002 4.2 „Referenzreihenfolge“, 4.7).
///
/// Jeder Punkt erhält die früheste Referenzstelle innerhalb der Toleranz, die nicht mehr als `max_backstep_m`
/// hinter dem bisherigen Fortschritt liegt. Die Stelle hinkt der wahren Lage bis zu `tolerance_m` hinterher;
/// auf gerader Strecke wird ein Rückschritt daher erst ab etwa 2 · `tolerance_m` + `max_backstep_m` erkannt. An Kehren, deren Äste näher als die Toleranz beieinander liegen,
/// bleibt der Track so auf dem ersten Ast, bis nur noch der zweite passt. Ein Rückschritt liegt vor, wenn ein
/// Punkt nur noch zu Stellen weiter als `max_backstep_m` hinter dem bisherigen Stand passt. `Err`: Rückschritt
/// (kein Versuch); `Ambiguous`: Schleife oder Umweg außerhalb der Toleranz.
fn follow(r: &Reference, track: &[Tp], shift: V, s: Crossing, e: Crossing, params: &MatchingSection) -> Result<Geometry, String> {
    let tol = params.tolerance_m;
    let gap = inner_gap(r, params);
    let back = params.max_backstep_m;
    let mut pos = 0.0_f64;
    let mut furthest = 0.0_f64;
    let mut prev = point_on(track, shift, s.edge, s.frac);
    let mut travelled = 0.0_f64;
    let mut off = 0.0_f64;
    let mut off_from = 0.0_f64;
    let mut result = Geometry::Clear;
    let last = (e.edge + 1).min(track.len() - 1);
    let end_point = point_on(track, shift, e.edge, e.frac);
    let points = (s.edge + 1..=last).map(|i| (i, if i == last { end_point } else { track[i].p - shift }));
    for (i, p) in points {
        let step = (p - prev).len();
        travelled += step;
        prev = p;
        let lo = (furthest - back - 1e-9).max(0.0);
        // Fenster nach vorn: Schrittlänge plus abgekürzte Kehre (beide Äste in Toleranz, dazwischen bis
        // `max_inner_gap_m`); verhindert nur Sprünge auf weit entfernte Stellen (Achter, Lasso)
        let hi = pos.max(furthest) + travelled + 4.0 * tol + 2.0 * gap;
        let Some(first) = r.first_within(p, lo, hi, tol) else {
            if lo > 0.0 && r.first_within(p, 0.0, lo, tol).is_some() && r.first_within(p, lo, r.length, tol).is_none() {
                return Err(format!(
                    "Rückschritt über {back} m bei Punkt {} (zuletzt bei {furthest:.0} von {:.0} m)",
                    track[i].idx, r.length
                ));
            }
            if off == 0.0 {
                off_from = furthest;
            }
            off += step;
            continue;
        };

        let at = first;
        if off > (at - off_from).max(0.0) + 2.0 * gap {
            result = Geometry::Ambiguous;
        }
        off = 0.0;
        pos = at;
        furthest = furthest.max(at);
        travelled = 0.0;
    }
    Ok(result)
}

/// Interpolierte Start- und Zielzeit in ms (gerundet, halbe ms aufgerundet); `None` bei unbelastbarer Zeit.
fn attempt_time(track: &[Tp], s: Crossing, e: Crossing, params: &MatchingSection) -> Option<(i64, i64)> {
    let gap_ms = params.max_interpolation_gap_s as i64 * 1000;
    let last = (e.edge + 1).min(track.len() - 1);
    let mut prev: Option<i64> = None;
    for q in &track[s.edge..=last] {
        let t = q.t?;
        if prev.is_some_and(|p| t < p) {
            return None;
        }
        prev = Some(t);
    }
    let at = |c: Crossing| -> Option<f64> {
        let a = track[c.edge].t? as f64;
        if c.frac == 0.0 || c.edge + 1 >= track.len() {
            return Some(a);
        }
        let b = track[c.edge + 1].t? as f64;
        if b - a > gap_ms as f64 {
            return None;
        }
        Some(a + c.frac * (b - a))
    };
    let round = |x: f64| (x + 0.5).floor() as i64;
    Some((round(at(s)?), round(at(e)?)))
}

/// Ø (zeitgewichtet, Wert gilt bis zum nächsten Punkt) und max. Herzfrequenz zwischen Start und Ziel.
fn heart_rate(section: &[Point], start: i64, end: i64) -> (Option<u16>, Option<u16>) {
    let pts: Vec<(i64, u16)> = section.iter().filter_map(|p| Some((p.t_ms?, p.hr?))).collect();
    let mut sum = 0.0;
    let mut weight = 0.0;
    let mut max: Option<u16> = None;
    for (k, &(t, hr)) in pts.iter().enumerate() {
        let next = pts.get(k + 1).map_or(t, |n| n.0);
        let a = t.max(start);
        let b = next.min(end);
        if b > a {
            sum += hr as f64 * (b - a) as f64;
            weight += (b - a) as f64;
            max = max.max(Some(hr));
        } else if t >= start && t <= end {
            max = max.max(Some(hr));
        }
    }
    if weight == 0.0 {
        return (None, max);
    }
    (Some((sum / weight + 0.5).floor() as u16), max)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::track::deg_to_e7;

    const LAT0: f64 = 48.0;
    const LON0: f64 = 8.5;

    /// Feste Werte für die Grenztests unabhängig von den Vorgaben der TOML: 13 m Toleranz, 15 m Radius,
    /// 30 m Lücke ohne Anteil der Länge (so wurden die Grenzen in RFC-0002 hergeleitet).
    fn params() -> MatchingSection {
        MatchingSection {
            tolerance_m: 13.0,
            matching_radius_meters: 15.0,
            max_inner_gap_ratio: 0.0,
            recording_edge_m: 15.0,
            ..MatchingSection::default()
        }
    }

    /// Meter (Ost, Nord) um (LAT0, LON0) in Grad.
    fn deg(x: f64, y: f64) -> (f64, f64) {
        let kx = LAT0.to_radians().cos() * M_PER_DEG;
        (LAT0 + y / M_PER_DEG, LON0 + x / kx)
    }

    fn reference(xy: &[(f64, f64)]) -> Reference {
        let pts: Vec<(f64, f64)> = xy.iter().map(|&(x, y)| deg(x, y)).collect();
        Reference::new(&pts, &params()).unwrap()
    }

    /// Gerade Referenz nach Osten, `len` Meter, Punkte alle 10 m.
    fn straight(len: f64) -> Reference {
        let n = (len / 10.0) as usize;
        reference(&(0..=n).map(|i| (i as f64 * len / n as f64, 0.0)).collect::<Vec<_>>())
    }

    /// Track aus (x, y, Sekunde) mit HF 120.
    fn track(xyt: &[(f64, f64, f64)]) -> Vec<Point> {
        xyt.iter()
            .map(|&(x, y, s)| {
                let (lat, lon) = deg(x, y);
                Point { t_ms: Some((s * 1000.0).round() as i64), pos: Some((deg_to_e7(lat), deg_to_e7(lon))), ele_dm: None, hr: Some(120) }
            })
            .collect()
    }

    /// Fahrt entlang y = f(x) von x0 bis x1 mit 5 m/s, Punkt jede Sekunde.
    fn ride(x0: f64, x1: f64, f: impl Fn(f64) -> f64, t0: f64) -> Vec<(f64, f64, f64)> {
        let n = ((x1 - x0).abs() / 5.0) as usize;
        (0..=n)
            .map(|i| {
                let x = x0 + (x1 - x0) * i as f64 / n as f64;
                (x, f(x), t0 + i as f64)
            })
            .collect()
    }

    fn valid(a: &[Attempt]) -> Vec<&Attempt> {
        a.iter().filter(|a| a.is_valid()).collect()
    }

    #[test]
    fn straight_ride_gives_one_valid_attempt_with_interpolated_time() {
        let r = straight(500.0);
        let t = track(&ride(-52.5, 552.5, |_| 0.0, 0.0));
        let a = match_section(&r, &t, &params());
        assert_eq!(a.len(), 1);
        let a = &a[0];
        assert!(a.is_valid());
        // Start bei x = 0 nach 10,5 s, Ziel bei x = 500 nach 110,5 s
        assert_eq!(a.start_ms, Some(10_500));
        assert_eq!(a.duration_ms, Some(100_000));
        assert_eq!(a.avg_heart_rate_bpm, Some(120));
        assert_eq!(a.max_heart_rate_bpm, Some(120));
        assert!(a.coverage_ratio > 0.99);
    }

    #[test]
    fn opposite_direction_never_counts() {
        let r = straight(500.0);
        let t = track(&ride(552.5, -52.5, |_| 0.0, 0.0));
        assert!(match_section(&r, &t, &params()).is_empty());
    }

    #[test]
    fn constant_offset_is_compensated_up_to_max_offset() {
        let r = straight(500.0);
        // 12 m parallel versetzt: Querlinien noch im Radius, nach Ausgleich deckungsgleich
        let a = match_section(&r, &track(&ride(-52.5, 552.5, |_| 12.0, 0.0)), &params());
        assert_eq!(valid(&a).len(), 1);
        assert!((a[0].offset_m - 12.0).abs() < 0.5, "{}", a[0].offset_m);
        // 30 m versetzt: außerhalb des Radius, kein Kandidat
        assert!(match_section(&r, &track(&ride(-52.5, 552.5, |_| 30.0, 0.0)), &params()).is_empty());
    }

    #[test]
    fn finish_right_before_a_sharp_turn_counts() {
        // Ziel kurz vor einer Rechtskurve: Kante über die Linie in Richtung, danach sofort nach Süden
        let r = straight(500.0);
        let mut xyt = ride(-52.5, 502.5, |_| 0.0, 0.0);
        let t = xyt.last().unwrap().2;
        for k in 1..=10 {
            xyt.push((502.5, -(k as f64) * 5.0, t + k as f64));
        }
        assert_eq!(valid(&match_section(&r, &track(&xyt), &params())).len(), 1);
    }

    #[test]
    fn route_counts_recording_start_and_end_near_the_control_points() {
        let r = straight(500.0);
        // Aufzeichnung beginnt 4 m hinter dem Start und endet 6 m vor dem Ziel: keine echte Passage
        let t = track(&ride(4.0, 494.0, |_| 0.0, 0.0));
        assert!(match_section(&r, &t, &params()).is_empty(), "Segment: echte Passage nötig");
        let route = r.clone().with_open_ends(true);
        let a = match_section(&route, &t, &params());
        assert_eq!(valid(&a).len(), 1, "{a:?}");
        assert_eq!((a[0].start_ms, a[0].duration_ms), (Some(0), Some(98_000)));
        // weiter als der Radius (15 m) entfernt: kein Versuch
        let far = track(&ride(20.0, 494.0, |_| 0.0, 0.0));
        assert!(match_section(&route, &far, &params()).is_empty());
        // Gegenrichtung zählt auch am Rand nicht
        let back = track(&ride(494.0, 4.0, |_| 0.0, 0.0));
        assert!(match_section(&route, &back, &params()).is_empty());
    }

    #[test]
    fn candidate_between_two_points_outside_radius() {
        // Punkte 40 m auseinander, beide außerhalb des 15-m-Radius um Start und Ziel
        let r = straight(400.0);
        let xyt: Vec<(f64, f64, f64)> = (0..=12).map(|i| (-20.0 + i as f64 * 40.0, 0.0, i as f64 * 4.0)).collect();
        let a = match_section(&r, &track(&xyt), &params());
        assert_eq!(valid(&a).len(), 1);
    }

    #[test]
    fn joining_in_the_middle_is_no_attempt() {
        let r = straight(500.0);
        // kommt von Norden, mündet bei x = 250 ein und fährt bis über das Ziel
        let mut xyt: Vec<(f64, f64, f64)> = (0..=20).map(|i| (250.0, 200.0 - i as f64 * 10.0, i as f64 * 2.0)).collect();
        xyt.extend(ride(255.0, 560.0, |_| 0.0, 41.0));
        assert!(match_section(&r, &track(&xyt), &params()).is_empty());
    }

    #[test]
    fn inner_gap_boundary() {
        // 1000 m, damit die Abdeckung (95 %) bis 50 m Lücke erlaubt und die Lückenregel entscheidet
        let r = straight(1000.0);
        // Ausbuchtung 25 m nach Norden; die schrägen Zu- und Abfahrten decken je etwa 13 m ab,
        // die Lücke ist also etwa 26 m kürzer als die Ausbuchtung breit ist
        let bulge = |w: f64| move |x: f64| if (200.0..200.0 + w).contains(&x) { 25.0 } else { 0.0 };
        let ok = match_section(&r, &track(&ride(-52.5, 1052.5, bulge(50.0), 0.0)), &params());
        assert_eq!(valid(&ok).len(), 1, "Lücke um 25 m zulässig: {ok:?}");
        let bad = match_section(&r, &track(&ride(-52.5, 1052.5, bulge(65.0), 0.0)), &params());
        assert!(bad.is_empty(), "Lücke über 30 m: kein Versuch: {bad:?}");
    }

    #[test]
    fn interpolation_gap_boundary_makes_time_invalid() {
        let r = straight(500.0);
        // Start zwischen zwei Punkten mit 15 s bzw. 16 s Abstand
        // Punkte bei x = -2,5 und 2,5, die Startlinie liegt dazwischen
        let mut base = ride(-52.5, 552.5, |_| 0.0, 0.0);
        let shift_after = |pts: &mut Vec<(f64, f64, f64)>, gap: f64| {
            // Zeit zwischen den Punkten vor und nach der Startlinie auf `gap` strecken
            let i = pts.iter().position(|p| p.0 > 0.0).unwrap();
            let extra = gap - (pts[i].2 - pts[i - 1].2);
            for p in pts.iter_mut().skip(i) {
                p.2 += extra;
            }
        };
        shift_after(&mut base, 15.0);
        let a = match_section(&r, &track(&base), &params());
        assert_eq!(valid(&a).len(), 1, "15 s einschließlich");

        let mut late = ride(-52.5, 552.5, |_| 0.0, 0.0);
        shift_after(&mut late, 16.0);
        let a = match_section(&r, &track(&late), &params());
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].not_ranked, Some(NotRanked::TimeInvalid));
        assert_eq!(a[0].duration_ms, None);
    }

    #[test]
    fn coverage_connection_gap_breaks_coverage() {
        let r = straight(500.0);
        // Aufzeichnungslücke über 100 m mit 25 s: Abdeckung fehlt dort, kein Versuch
        let mut xyt = ride(-50.0, 200.0, |_| 0.0, 0.0);
        xyt.extend(ride(300.0, 550.0, |_| 0.0, 75.0));
        assert!(match_section(&r, &track(&xyt), &params()).is_empty());
        // dieselbe Lücke mit 15 s gilt als zusammenhängend
        let mut xyt = ride(-50.0, 200.0, |_| 0.0, 0.0);
        xyt.extend(ride(300.0, 550.0, |_| 0.0, 65.0));
        assert_eq!(valid(&match_section(&r, &track(&xyt), &params())).len(), 1);
    }

    #[test]
    fn missing_or_backward_time_makes_attempt_time_invalid() {
        let r = straight(500.0);
        let mut pts = track(&ride(-50.0, 550.0, |_| 0.0, 0.0));
        pts[50].t_ms = None;
        let a = match_section(&r, &pts, &params());
        assert_eq!(a[0].not_ranked, Some(NotRanked::TimeInvalid));

        let mut pts = track(&ride(-50.0, 550.0, |_| 0.0, 0.0));
        pts[50].t_ms = Some(pts[48].t_ms.unwrap() - 500);
        let a = match_section(&r, &pts, &params());
        assert_eq!(a[0].not_ranked, Some(NotRanked::TimeInvalid));
    }

    #[test]
    fn equal_timestamp_first_point_counts() {
        let r = straight(500.0);
        let mut pts = track(&ride(-50.0, 550.0, |_| 0.0, 0.0));
        // Doppelter Zeitstempel mit Ausreißerposition wird ignoriert
        let mut dup = pts[40];
        dup.pos = Some((deg_to_e7(deg(150.0, 500.0).0), deg_to_e7(deg(150.0, 500.0).1)));
        pts.insert(41, dup);
        assert_eq!(valid(&match_section(&r, &pts, &params())).len(), 1);
    }

    #[test]
    fn backstep_boundary() {
        let r = straight(500.0);
        let with_back = |back: f64| {
            let mut xyt = ride(-50.0, 250.0, |_| 0.0, 0.0);
            let t = xyt.last().unwrap().2;
            xyt.push((250.0 - back, 0.0, t + 2.0));
            xyt.extend(ride(250.0, 550.0, |_| 0.0, t + 4.0));
            match_section(&r, &track(&xyt), &params())
        };
        assert_eq!(valid(&with_back(5.0)).len(), 1, "genau 5 m zulässig");
        // auf gerader Strecke erst ab etwa 2 · 13 m + 5 m messbar (Toleranz um Lage und zugeordnete Stelle)
        assert_eq!(valid(&with_back(25.0)).len(), 1, "noch in Toleranz der bisherigen Stelle");
        assert!(with_back(40.0).is_empty(), "darüber: Rückschritt");
    }

    /// Weg aus Eckpunkten mit 5 m je Sekunde.
    fn walk(path: &[(f64, f64)]) -> Vec<(f64, f64, f64)> {
        let mut xyt = vec![];
        let mut t = 0.0;
        for w in path.windows(2) {
            let (a, b) = (w[0], w[1]);
            let n = ((b.0 - a.0).hypot(b.1 - a.1) / 5.0).round() as usize;
            for i in 0..n {
                let f = i as f64 / n as f64;
                xyt.push((a.0 + (b.0 - a.0) * f + 0.5, a.1 + (b.1 - a.1) * f, t));
                t += 1.0;
            }
        }
        let last = path.last().unwrap();
        xyt.push((last.0 + 0.5, last.1, t));
        xyt
    }

    #[test]
    fn laps_on_a_loop_give_one_attempt_per_lap() {
        // Quadratischer Rundkurs 4 x 100 m, Start = Ziel in der Mitte der Südseite, Richtung Osten
        let corners: [(f64, f64); 6] = [(50.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 100.0), (0.0, 0.0), (50.0, 0.0)];
        let mut xy = vec![];
        for w in corners.windows(2) {
            let n = ((w[1].0 - w[0].0).hypot(w[1].1 - w[0].1) / 10.0) as usize;
            for i in 0..n {
                let f = i as f64 / n as f64;
                xy.push((w[0].0 + (w[1].0 - w[0].0) * f, w[0].1 + (w[1].1 - w[0].1) * f));
            }
        }
        xy.push((50.0, 0.0));
        let r = reference(&xy);
        // Anfahrt, drei Runden, Abfahrt (0,5 m neben den Ecken, damit kein Punkt genau auf der Linie liegt)
        let lap = [(100.0, 0.0), (100.0, 100.0), (0.0, 100.0), (0.0, 0.0)];
        let mut path = vec![(20.0, 0.0)];
        for _ in 0..3 {
            path.extend_from_slice(&lap);
        }
        path.push((80.0, 0.0));
        let a = match_section(&r, &track(&walk(&path)), &params());
        let v = valid(&a);
        assert_eq!(v.len(), 3, "{a:?}");
        for x in &v {
            assert!((x.duration_ms.unwrap() - 80_000).abs() <= 1_000, "{:?}", x.duration_ms);
        }
        assert_eq!(a.iter().map(|x| x.position).collect::<Vec<_>>(), vec![0, 1, 2]);
    }

    #[test]
    fn loop_off_the_segment_is_ambiguous() {
        let r = straight(500.0);
        // bei x = 250 Abstecher 100 m nach Norden und zurück an dieselbe Stelle
        let mut xyt = ride(-50.0, 250.0, |_| 0.0, 0.0);
        let t = xyt.last().unwrap().2;
        for i in 1..=20 {
            xyt.push((250.0, i as f64 * 5.0, t + i as f64));
        }
        for i in 1..=20 {
            xyt.push((250.0, 100.0 - i as f64 * 5.0, t + 20.0 + i as f64));
        }
        xyt.extend(ride(250.0, 550.0, |_| 0.0, t + 41.0));
        let a = match_section(&r, &track(&xyt), &params());
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].not_ranked, Some(NotRanked::AmbiguousGeometry));
    }

    #[test]
    fn heart_rate_is_time_weighted() {
        let pts: Vec<Point> = [(0, 100), (10_000, 160), (20_000, 100)]
            .iter()
            .map(|&(t, hr)| Point { t_ms: Some(t), pos: None, ele_dm: None, hr: Some(hr) })
            .collect();
        // 0–10 s 100, 10–15 s 160: (100*10 + 160*5) / 15 = 120
        assert_eq!(heart_rate(&pts, 0, 15_000), (Some(120), Some(160)));
        assert_eq!(heart_rate(&[], 0, 1000), (None, None));
    }

    #[test]
    fn reference_direction_must_be_determinable() {
        assert_eq!(Reference::new(&[deg(0.0, 0.0), deg(0.0, 0.0)], &params()).unwrap_err(), ReferenceError::TooFewPoints);
        // Rundkurs, dessen Bezugspunkte zusammenfallen: Start = Ziel und nach 5 m wieder am Start
        let back = Reference::new(&[deg(0.0, 0.0), deg(2.5, 0.0), deg(0.0, 0.0)], &params());
        assert_eq!(back.unwrap_err(), ReferenceError::Direction);
    }

    fn route_params() -> MatchingSection {
        crate::config::SegmentsSection::default().params_for("route")
    }

    #[test]
    fn route_parameters_allow_gps_drift_and_recording_edges_further_away() {
        let pts: Vec<(f64, f64)> = (0..=300).map(|i| deg(i as f64 * 10.0, 0.0)).collect();
        let route = Reference::new(&pts, &route_params()).unwrap().with_open_ends(true);
        // Aufzeichnung beginnt 60 m nach dem Start, 150 m Waldstück mit 25 m seitlicher Abweichung
        let t = track(&ride(60.0, 2990.0, |x| if (1000.0..1150.0).contains(&x) { 25.0 } else { 0.0 }, 0.0));
        assert_eq!(valid(&match_section(&route, &t, &route_params())).len(), 1);
        let strict = Reference::new(&pts, &params()).unwrap().with_open_ends(true);
        assert!(valid(&match_section(&strict, &t, &params())).is_empty(), "Segmentwerte sind strenger");
        // Umweg von 400 m Länge ist mehr als die erlaubte Lücke
        let detour = track(&ride(60.0, 2990.0, |x| if (1000.0..1400.0).contains(&x) { 80.0 } else { 0.0 }, 0.0));
        assert!(valid(&match_section(&route, &detour, &route_params())).is_empty());
    }

    #[test]
    fn edge_grid_finds_the_same_nearest_point_as_a_full_search() {
        // Spirale mit unterschiedlich langen Kanten, Abfragen nah und weit weg
        let track: Vec<Tp> = (0..400)
            .map(|i| {
                let a = i as f64 * 0.11;
                let r = 20.0 + i as f64 * 3.0;
                Tp { p: V::new(r * a.cos(), r * a.sin()), t: None, idx: i }
            })
            .collect();
        let edges: Vec<usize> = (0..track.len() - 1).filter(|i| i % 7 != 3).collect();
        let grid = EdgeGrid::new(&track, &edges);
        let shift = V::new(3.3, -7.1);
        for k in 0..500 {
            let p = V::new((k as f64 * 37.7) % 2600.0 - 1300.0, (k as f64 * 91.3) % 2600.0 - 1300.0);
            let mut best = (f64::INFINITY, V::default());
            for &i in &edges {
                let (q, _) = nearest_on_edge(p, track[i].p - shift, track[i + 1].p - shift);
                if (q - p).len() < best.0 {
                    best = ((q - p).len(), q);
                }
            }
            assert_eq!(grid.nearest(&track, shift, p), Some(best.1), "Abfrage {k}");
            for tol in [5.0, 13.0, 35.0] {
                assert_eq!(grid.any_within(&track, shift, p, tol), best.0 <= tol, "Abfrage {k}, Toleranz {tol}");
            }
        }
    }
}
