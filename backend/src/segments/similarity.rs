//! Ähnliche Strecken über den Verlauf: Bei Strecken liegen Start und Ziel am Aufzeichnungsrand und
//! schwanken von Fahrt zu Fahrt um einige hundert Meter. Deshalb vergleicht TrackCraft, wie viel einer Linie
//! innerhalb der Matching-Toleranz der anderen liegt, und zwar in beide Richtungen.

use std::collections::HashMap;

use crate::domain::track::e7_to_deg;

/// Ab diesem Anteil in beide Richtungen gilt eine Strecke als ähnlich.
pub const SIMILAR_SHARE: f64 = 0.9;
/// Probenabstand auf der geprüften Linie
const SAMPLE_STEP_M: f64 = 25.0;
/// Dichte der Vergleichslinie: Punktabstand statt Kantenabstand, Fehler höchstens die Hälfte davon
const DENSE_STEP_M: f64 = 5.0;

/// Ebene Näherung in Metern um einen Bezugspunkt (für Abstände bis einige 100 km genau genug).
struct Plane {
    lat0: f64,
    lon0: f64,
    kx: f64,
}

impl Plane {
    const KY: f64 = 110_574.0;

    fn new(p: (i32, i32)) -> Self {
        let (lat0, lon0) = (e7_to_deg(p.0), e7_to_deg(p.1));
        Self { lat0, lon0, kx: lat0.to_radians().cos() * 111_320.0 }
    }

    fn xy(&self, p: (i32, i32)) -> (f64, f64) {
        ((e7_to_deg(p.1) - self.lon0) * self.kx, (e7_to_deg(p.0) - self.lat0) * Self::KY)
    }
}

/// Punkte entlang der Linie etwa alle `step` Meter, inklusive Anfang und Ende.
fn resample(line: &[(f64, f64)], step: f64) -> Vec<(f64, f64)> {
    let Some(&first) = line.first() else { return vec![] };
    let mut out = vec![first];
    let mut rest = step;
    for w in line.windows(2) {
        let (a, b) = (w[0], w[1]);
        let len = (b.0 - a.0).hypot(b.1 - a.1);
        let mut at = rest;
        while at <= len {
            let f = at / len;
            out.push((a.0 + (b.0 - a.0) * f, a.1 + (b.1 - a.1) * f));
            at += step;
        }
        rest = at - len;
    }
    if let Some(&last) = line.last()
        && out.last() != Some(&last)
    {
        out.push(last);
    }
    out
}

/// Gitter über die dichten Punkte der Vergleichslinie, Zellgröße = Toleranz.
struct Grid {
    cell: f64,
    cells: HashMap<(i64, i64), Vec<(f64, f64)>>,
}

impl Grid {
    fn new(points: &[(f64, f64)], cell: f64) -> Self {
        let mut cells: HashMap<(i64, i64), Vec<(f64, f64)>> = HashMap::new();
        for &p in points {
            cells.entry(Self::key(p, cell)).or_default().push(p);
        }
        Self { cell, cells }
    }

    fn key(p: (f64, f64), cell: f64) -> (i64, i64) {
        ((p.0 / cell).floor() as i64, (p.1 / cell).floor() as i64)
    }

    fn near(&self, p: (f64, f64), tol: f64) -> bool {
        let (cx, cy) = Self::key(p, self.cell);
        (-1..=1).any(|dx| {
            (-1..=1).any(|dy| self.cells.get(&(cx + dx, cy + dy)).is_some_and(|v| v.iter().any(|q| (q.0 - p.0).hypot(q.1 - p.1) <= tol)))
        })
    }
}

/// Anteil der Proben von `a`, die höchstens `tol_m` neben `b` liegen.
fn share_near(a: &[(f64, f64)], b: &Grid, tol_m: f64) -> f64 {
    let samples = resample(a, SAMPLE_STEP_M);
    if samples.is_empty() {
        return 0.0;
    }
    samples.iter().filter(|p| b.near(**p, tol_m)).count() as f64 / samples.len() as f64
}

/// Überdeckung zweier Linien (Positionen Grad·10^7) in beide Richtungen: (Anteil von `a` nahe `b`, umgekehrt).
pub fn overlap(a: &[(i32, i32)], b: &[(i32, i32)], tol_m: f64) -> (f64, f64) {
    let Some(&origin) = a.first() else { return (0.0, 0.0) };
    let plane = Plane::new(origin);
    let pa: Vec<_> = a.iter().map(|p| plane.xy(*p)).collect();
    let pb: Vec<_> = b.iter().map(|p| plane.xy(*p)).collect();
    let ga = Grid::new(&resample(&pa, DENSE_STEP_M), tol_m);
    let gb = Grid::new(&resample(&pb, DENSE_STEP_M), tol_m);
    (share_near(&pa, &gb, tol_m), share_near(&pb, &ga, tol_m))
}

/// Gleiche Runde: beide Richtungen mindestens `SIMILAR_SHARE`.
pub fn is_similar(a: &[(i32, i32)], b: &[(i32, i32)], tol_m: f64) -> bool {
    let (ab, ba) = overlap(a, b, tol_m);
    ab >= SIMILAR_SHARE && ba >= SIMILAR_SHARE
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::track::deg_to_e7;

    /// Linie aus (Ost, Nord) in Metern um 48° N, 8° O.
    fn line(xy: &[(f64, f64)]) -> Vec<(i32, i32)> {
        let k = 48f64.to_radians().cos() * 111_320.0;
        xy.iter().map(|&(x, y)| (deg_to_e7(48.0 + y / 110_574.0), deg_to_e7(8.0 + x / k))).collect()
    }

    /// Rechteckige Runde 5 × 3 km, beginnend bei `from` Metern und endend `gap` Meter vor dem Anfang, um `dy` versetzt.
    fn lap(from: f64, gap: f64, dy: f64) -> Vec<(i32, i32)> {
        line(&[(from, dy), (5000.0, dy), (5000.0, 3000.0 + dy), (0.0, 3000.0 + dy), (0.0, gap + dy)])
    }

    #[test]
    fn same_lap_with_shifted_ends_is_similar() {
        // Wie die beiden Zollhaus-Einträge: Starts 316 m, Ziele 391 m auseinander, GPS 12 m versetzt
        let a = lap(0.0, 155.0, 0.0);
        let b = lap(316.0, 461.0, 12.0);
        let (ab, ba) = overlap(&a, &b, 35.0);
        assert!(ab >= 0.9 && ba >= 0.9, "{ab} {ba}");
        assert!(is_similar(&a, &b, 35.0));
    }

    #[test]
    fn different_way_from_same_start_is_not_similar() {
        // Gleicher Startort, ähnliche Länge, aber anderer Weg: Rückweg 1 km weiter nördlich
        let a = lap(0.0, 100.0, 0.0);
        let b = line(&[(0.0, 0.0), (5000.0, 0.0), (5000.0, 4000.0), (0.0, 4000.0), (0.0, 100.0)]);
        assert!(!is_similar(&a, &b, 35.0));
    }

    #[test]
    fn part_of_a_longer_route_is_not_similar() {
        let long = line(&[(0.0, 0.0), (10_000.0, 0.0)]);
        let half = line(&[(0.0, 0.0), (5000.0, 0.0)]);
        let (ab, ba) = overlap(&half, &long, 35.0);
        assert!(ab > 0.99 && ba < 0.55, "{ab} {ba}");
        assert!(!is_similar(&half, &long, 35.0));
    }

    #[test]
    fn resample_keeps_ends_and_spacing() {
        let r = resample(&[(0.0, 0.0), (100.0, 0.0)], 25.0);
        assert_eq!(r, vec![(0.0, 0.0), (25.0, 0.0), (50.0, 0.0), (75.0, 0.0), (100.0, 0.0)]);
    }
}
