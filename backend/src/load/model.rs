//! Reine Rechnungen ohne Datenbank: Zonengrenzen in Schlägen, Banister-TRIMP, Fitness/Ermüdung/Form.

use std::collections::BTreeMap;

use chrono::NaiveDate;

use crate::domain::track::Point;

/// Version der Belastungsformel; eine Änderung rechnet alle Aktivitäten neu.
pub const FORMULA_VERSION: u32 = 1;

/// Längere Lücken zwischen zwei Messpunkten gelten als Pause und zählen nicht.
pub const MAX_GAP_MS: i64 = 60_000;

pub const ZONES: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Model {
    /// % der maximalen HF
    Max,
    /// % der HF-Reserve (Karvonen)
    Reserve,
    /// % der Schwellen-HF (Friel)
    Threshold,
}

impl Model {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "max" => Some(Self::Max),
            "reserve" => Some(Self::Reserve),
            "threshold" => Some(Self::Threshold),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Max => "max",
            Self::Reserve => "reserve",
            Self::Threshold => "threshold",
        }
    }

    /// Vorgabe der unteren Grenzen in Prozent.
    pub fn default_bounds(self) -> [u32; ZONES] {
        match self {
            Self::Max | Self::Reserve => [50, 60, 70, 80, 90],
            Self::Threshold => [68, 85, 90, 95, 100],
        }
    }
}

/// Die Werte eines Zonensatzes, die die Rechnung braucht.
#[derive(Debug, Clone, PartialEq)]
pub struct Zones {
    pub max_hr: u32,
    pub resting_hr: u32,
    pub threshold_hr: Option<u32>,
    pub model: Model,
    pub bounds: [u32; ZONES],
}

impl Zones {
    fn bpm(&self, pct: u32) -> u32 {
        let p = pct as f64 / 100.0;
        let v = match self.model {
            Model::Max => self.max_hr as f64 * p,
            Model::Reserve => self.resting_hr as f64 + (self.max_hr - self.resting_hr) as f64 * p,
            Model::Threshold => self.threshold_hr.unwrap_or(self.max_hr) as f64 * p,
        };
        v.round() as u32
    }

    /// Untere Grenze jeder Zone in Schlägen pro Minute.
    pub fn lower_bpm(&self) -> [u32; ZONES] {
        self.bounds.map(|p| self.bpm(p))
    }

    /// Ab dieser HF zählt die Belastung als anaerob: Schwellen-HF, sonst Beginn von Zone 4.
    pub fn anaerobic_from(&self) -> u32 {
        self.threshold_hr.unwrap_or_else(|| self.lower_bpm()[3])
    }

    /// Zone einer HF (0-basiert); alles unter Zone 2 zählt zu Zone 1.
    pub fn zone_of(&self, hr: u32) -> usize {
        let lower = self.lower_bpm();
        (1..ZONES).rev().find(|&i| hr >= lower[i]).unwrap_or(0)
    }

    /// HF für die Planung (E8): Mitte der Zone, für die oberste Zone die Mitte zwischen Untergrenze und max. HF.
    pub fn zone_mid_bpm(&self, zone: usize) -> f64 {
        let lower = self.lower_bpm();
        let hi = if zone + 1 < ZONES { lower[zone + 1] as f64 } else { self.max_hr as f64 };
        (lower[zone] as f64 + hi) / 2.0
    }

    fn reserve_fraction(&self, hr: f64) -> f64 {
        let span = self.max_hr as f64 - self.resting_hr as f64;
        if span <= 0.0 { 0.0 } else { ((hr - self.resting_hr as f64) / span).clamp(0.0, 1.0) }
    }
}

/// Banister-TRIMP für `minutes` bei Anteil `x` der HF-Reserve.
pub fn banister(minutes: f64, x: f64, sex_factor: f64) -> f64 {
    minutes * x * 0.64 * (sex_factor * x).exp()
}

#[derive(Debug, Clone, PartialEq)]
pub struct Computed {
    pub trimp: f64,
    pub aerobic: f64,
    pub anaerobic: f64,
    pub zone_seconds: Option<[f64; ZONES]>,
}

/// Belastung aus den Messpunkten; `None`, wenn keine zwei zeitlich aufeinanderfolgenden HF-Werte vorliegen.
pub fn from_samples(sections: &[Vec<Point>], z: &Zones, sex_factor: f64) -> Option<Computed> {
    let threshold = z.anaerobic_from();
    let mut out = Computed { trimp: 0.0, aerobic: 0.0, anaerobic: 0.0, zone_seconds: Some([0.0; ZONES]) };
    let mut counted = false;
    for s in sections {
        for w in s.windows(2) {
            let (Some(t0), Some(t1), Some(hr)) = (w[0].t_ms, w[1].t_ms, w[0].hr) else { continue };
            let dt = t1 - t0;
            if dt <= 0 || dt > MAX_GAP_MS {
                continue;
            }
            counted = true;
            let hr = hr as u32;
            let load = banister(dt as f64 / 60_000.0, z.reserve_fraction(hr as f64), sex_factor);
            out.trimp += load;
            if hr >= threshold {
                out.anaerobic += load;
            } else {
                out.aerobic += load;
            }
            if let Some(zs) = out.zone_seconds.as_mut() {
                zs[z.zone_of(hr)] += dt as f64 / 1000.0;
            }
        }
    }
    counted.then_some(out)
}

/// Belastung aus Durchschnitts-HF und Dauer, wenn die Datei keine HF je Punkt hat.
pub fn from_average(avg_hr: u32, total_ms: i64, z: &Zones, sex_factor: f64) -> Computed {
    let load = banister(total_ms.max(0) as f64 / 60_000.0, z.reserve_fraction(avg_hr as f64), sex_factor);
    let ana = avg_hr >= z.anaerobic_from();
    Computed { trimp: load, aerobic: if ana { 0.0 } else { load }, anaerobic: if ana { load } else { 0.0 }, zone_seconds: None }
}

/// Geplante Minuten je Zone (E8): Banister-TRIMP mit der HF in der Zonenmitte; Zonen, deren Mitte ab der
/// Anaerob-Grenze liegt, zählen anaerob.
pub fn from_zone_minutes(minutes: &[f64; ZONES], z: &Zones, sex_factor: f64) -> Computed {
    let threshold = z.anaerobic_from() as f64;
    let mut out = Computed { trimp: 0.0, aerobic: 0.0, anaerobic: 0.0, zone_seconds: Some(minutes.map(|m| m * 60.0)) };
    for (i, m) in minutes.iter().enumerate() {
        let hr = z.zone_mid_bpm(i);
        let load = banister(*m, z.reserve_fraction(hr), sex_factor);
        out.trimp += load;
        if hr >= threshold {
            out.anaerobic += load;
        } else {
            out.aerobic += load;
        }
    }
    out
}

/// Schätzung ohne HF: Dauer mal Belastung je Stunde.
pub fn estimated(total_ms: i64, per_hour: f64) -> Computed {
    let load = total_ms.max(0) as f64 / 3_600_000.0 * per_hour;
    Computed { trimp: load, aerobic: load, anaerobic: 0.0, zone_seconds: None }
}

/// Summe der Belastung eines Tages.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct DayLoad {
    pub total: f64,
    pub aerobic: f64,
    pub anaerobic: f64,
    /// Anteil ohne HF (geschätzt), in `total` enthalten, nicht in aerob/anaerob
    pub estimated: f64,
}

impl DayLoad {
    /// Anteil für die aerobe Ermüdung im Zwei-Kurven-Modell: aerob plus geschätzt.
    fn aerobic_part(&self) -> f64 {
        (self.total - self.anaerobic).max(0.0)
    }
}

/// Rechenmodell der Formkurve.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CurveModel {
    /// Fitness und Ermüdung als gleitende Mittel (τ₁, τ₂)
    Standard { fatigue_days: f64 },
    /// Ermüdung getrennt aerob und anaerob; an Tagen ohne Belastung klingt sie um `rest_bonus` schneller ab
    TwoCurve { aerobic_days: f64, anaerobic_days: f64, rest_bonus: f64 },
}

/// Parameter der Formkurve.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CurveParams {
    pub fitness_days: f64,
    /// Form = Fitness − Gewicht × Ermüdung (#24); 1,0 = gleich gewichtet
    pub fatigue_weight: f64,
    pub model: CurveModel,
    /// Aufschlag auf die Ermüdung an Tagen mit Monotonie über [`MONOTONY_WARN`] (Foster), z. B. 0,15; `None`: aus
    pub foster_surcharge: Option<f64>,
}

/// Ab dieser Monotonie warnt die Seite (Foster).
pub const MONOTONY_WARN: f64 = 2.0;
const WINDOW: usize = 7;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DayValue {
    pub date: NaiveDate,
    pub load: DayLoad,
    /// Fitness nach dem Training des Tages
    pub fitness: f64,
    /// Ermüdung nach dem Training des Tages (im Zwei-Kurven-Modell die Summe beider Teile)
    pub fatigue: f64,
    /// Nur im Zwei-Kurven-Modell: aerober und anaerober Teil der Ermüdung
    pub fatigue_parts: Option<(f64, f64)>,
    /// Form zu Beginn des Tages (Fitness − Ermüdung des Vortags)
    pub form: f64,
    /// Monotonie der letzten 7 Tage (Mittel / Standardabweichung); `None` ohne Belastung oder ohne Streuung
    pub monotony: Option<f64>,
    /// Strain der letzten 7 Tage (Wochensumme × Monotonie)
    pub strain: Option<f64>,
}

/// Monotonie und Strain (Foster) für bis zu sieben Tageswerte; fehlende Tage zählen als 0.
pub fn foster(values: impl IntoIterator<Item = f64>) -> (Option<f64>, Option<f64>) {
    let v: Vec<f64> = values.into_iter().take(WINDOW).collect();
    let n = v.len() as f64;
    let sum: f64 = v.iter().sum();
    if sum <= 0.0 {
        return (None, None);
    }
    // Wie Foster: Mittel über 7 Tage, fehlende Tage zählen als 0
    let mean = sum / WINDOW as f64;
    let var = (v.iter().map(|x| (x - mean).powi(2)).sum::<f64>() + (WINDOW as f64 - n) * mean * mean) / WINDOW as f64;
    let sd = var.sqrt();
    if sd < 1e-9 {
        return (None, None);
    }
    let m = mean / sd;
    (Some(m), Some(sum * m))
}

fn decay(days: f64) -> f64 {
    1.0 - (-1.0 / days).exp()
}

/// Zustand der Kurve, Tag für Tag fortgeschrieben; Vergangenheit und Vorschau rechnen gleich.
#[derive(Debug, Clone)]
pub struct Curve {
    p: CurveParams,
    fitness: f64,
    /// Standard: Ermüdung; Zwei-Kurven: aerober Teil
    fatigue_a: f64,
    /// Zwei-Kurven: anaerober Teil
    fatigue_b: f64,
    window: std::collections::VecDeque<f64>,
}

impl Curve {
    pub fn new(p: CurveParams) -> Self {
        Self { p, fitness: 0.0, fatigue_a: 0.0, fatigue_b: 0.0, window: std::collections::VecDeque::with_capacity(WINDOW) }
    }

    fn fatigue(&self) -> f64 {
        self.fatigue_a + self.fatigue_b
    }

    /// Monotonie und Strain der letzten sieben Tage einschließlich heute.
    fn foster(&self) -> (Option<f64>, Option<f64>) {
        foster(self.window.iter().copied())
    }

    /// Fitness nach dem zuletzt gerechneten Tag.
    pub fn fitness(&self) -> f64 {
        self.fitness
    }

    pub fn step(&mut self, date: NaiveDate, load: DayLoad) -> DayValue {
        let form = self.fitness - self.p.fatigue_weight * self.fatigue();
        if self.window.len() == WINDOW {
            self.window.pop_front();
        }
        self.window.push_back(load.total);
        let (monotony, strain) = self.foster();
        let surcharge = match (self.p.foster_surcharge, monotony) {
            (Some(s), Some(m)) if m > MONOTONY_WARN => 1.0 + s,
            _ => 1.0,
        };
        self.fitness += (load.total - self.fitness) * decay(self.p.fitness_days);
        let parts = match self.p.model {
            CurveModel::Standard { fatigue_days } => {
                self.fatigue_a += (load.total * surcharge - self.fatigue_a) * decay(fatigue_days);
                None
            }
            CurveModel::TwoCurve { aerobic_days, anaerobic_days, rest_bonus } => {
                // Ruhetag: Zeitkonstanten kürzer, die Ermüdung klingt schneller ab
                let f = if load.total <= 0.0 { 1.0 - rest_bonus } else { 1.0 };
                self.fatigue_a += (load.aerobic_part() * surcharge - self.fatigue_a) * decay(aerobic_days * f);
                self.fatigue_b += (load.anaerobic * surcharge - self.fatigue_b) * decay(anaerobic_days * f);
                Some((self.fatigue_a, self.fatigue_b))
            }
        };
        DayValue { date, load, fitness: self.fitness, fatigue: self.fatigue(), fatigue_parts: parts, form, monotony, strain }
    }
}

/// Kurve Tag für Tag von `start` bis `end`; fehlende Tage haben die Belastung 0. Liefert auch den Zustand am Ende.
pub fn series(loads: &BTreeMap<NaiveDate, DayLoad>, start: NaiveDate, end: NaiveDate, p: CurveParams) -> (Vec<DayValue>, Curve) {
    let mut curve = Curve::new(p);
    let mut out = Vec::new();
    let mut d = start;
    while d <= end {
        out.push(curve.step(d, loads.get(&d).copied().unwrap_or_default()));
        d = d.succ_opt().expect("Datum");
    }
    (out, curve)
}

/// Führt die Kurve ab dem Tag nach `last` mit gleicher Tagesbelastung `daily` für `days` Tage fort.
pub fn forecast(curve: &Curve, last: NaiveDate, daily: DayLoad, days: u32) -> Vec<DayValue> {
    forecast_with(curve, last, |_| daily, days)
}

/// Wie [`forecast`], aber mit einer Belastung je Tag, etwa aus dem Trainingsplan (E8).
pub fn forecast_with(curve: &Curve, last: NaiveDate, load: impl Fn(NaiveDate) -> DayLoad, days: u32) -> Vec<DayValue> {
    let mut c = curve.clone();
    let mut d = last;
    (0..days)
        .map(|_| {
            d = d.succ_opt().expect("Datum");
            c.step(d, load(d))
        })
        .collect()
}

/// Maximale HF nach Alter (Tanaka 2001).
pub fn tanaka(age_years: u32) -> u32 {
    (208.0 - 0.7 * age_years as f64).round() as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Datelike;

    fn zones() -> Zones {
        Zones { max_hr: 180, resting_hr: 50, threshold_hr: None, model: Model::Reserve, bounds: [50, 60, 70, 80, 90] }
    }

    fn pt(t_s: i64, hr: u16) -> Point {
        Point { t_ms: Some(t_s * 1000), hr: Some(hr), ..Default::default() }
    }

    #[test]
    fn zone_bounds_per_model() {
        let z = zones();
        // 50 + 130 × p
        assert_eq!(z.lower_bpm(), [115, 128, 141, 154, 167]);
        assert_eq!(z.anaerobic_from(), 154);
        let m = Zones { model: Model::Max, ..zones() };
        assert_eq!(m.lower_bpm(), [90, 108, 126, 144, 162]);
        let t = Zones { model: Model::Threshold, threshold_hr: Some(160), bounds: Model::Threshold.default_bounds(), ..zones() };
        assert_eq!(t.lower_bpm(), [109, 136, 144, 152, 160]);
        assert_eq!(t.anaerobic_from(), 160);
        assert_eq!(z.zone_of(100), 0);
        assert_eq!(z.zone_of(128), 1);
        assert_eq!(z.zone_of(200), 4);
    }

    #[test]
    fn banister_one_hour() {
        // 60 min bei 60 % Reserve: 60 × 0,6 × 0,64 × e^(1,92 × 0,6) ≈ 72,9
        assert!((banister(60.0, 0.6, 1.92) - 72.89).abs() < 0.05);
    }

    #[test]
    fn samples_skip_pauses_and_split_aerobic() {
        let z = zones();
        // 10 min bei 128 (Z2), dann Pause von 5 min, dann 5 min bei 167 (Z5)
        let mut s: Vec<Point> = (0..=60).map(|i| pt(i * 10, 128)).collect();
        s.extend((0..=30).map(|i| pt(900 + i * 10, 167)));
        let c = from_samples(&[s], &z, 1.92).unwrap();
        let zs = c.zone_seconds.unwrap();
        assert_eq!(zs, [0.0, 600.0, 0.0, 0.0, 300.0]);
        let aer = banister(10.0, 0.6, 1.92);
        let ana = banister(5.0, 0.9, 1.92);
        assert!((c.aerobic - aer).abs() < 1e-9 && (c.anaerobic - ana).abs() < 1e-9);
        assert!((c.trimp - aer - ana).abs() < 1e-9);
        assert!(from_samples(&[vec![pt(0, 120)]], &z, 1.92).is_none());
    }

    const STD: CurveParams =
        CurveParams { fitness_days: 42.0, fatigue_weight: 1.0, model: CurveModel::Standard { fatigue_days: 7.0 }, foster_surcharge: None };

    fn steady(days: u64, load: DayLoad) -> BTreeMap<NaiveDate, DayLoad> {
        let d0 = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        (0..days).map(|i| (d0 + chrono::Days::new(i), load)).collect()
    }

    #[test]
    fn steady_load_converges_and_rest_recovers() {
        let d0 = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        let loads = steady(200, DayLoad { total: 50.0, ..Default::default() });
        let (s, curve) = series(&loads, d0, d0 + chrono::Days::new(199), STD);
        let last = s.last().unwrap();
        assert!((last.fatigue - 50.0).abs() < 0.01);
        assert!(last.fitness > 49.0 && last.fitness < 50.0);
        assert!(s[0].form == 0.0 && s[1].form < 0.0);
        assert_eq!(last.fatigue_parts, None);
        // Gleichmäßige Belastung: keine Streuung, keine Monotonie
        assert_eq!(last.monotony, None);
        // Ruhe: Form steigt, Fitness sinkt
        let f = forecast(&curve, last.date, DayLoad::default(), 14);
        assert_eq!(f.len(), 14);
        assert!(f[13].fitness < last.fitness && f[13].form > 20.0);
        assert_eq!(f[0].date, last.date.succ_opt().unwrap());
    }

    #[test]
    fn two_curve_splits_fatigue_and_rests_faster() {
        let d0 = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        let two = CurveParams { model: CurveModel::TwoCurve { aerobic_days: 5.0, anaerobic_days: 12.0, rest_bonus: 0.18 }, ..STD };
        let loads = steady(120, DayLoad { total: 60.0, aerobic: 30.0, anaerobic: 20.0, estimated: 10.0 });
        let end = d0 + chrono::Days::new(119);
        let (s, curve) = series(&loads, d0, end, two);
        let last = s.last().unwrap();
        let (a, b) = last.fatigue_parts.unwrap();
        // aerob = aerob + geschätzt
        assert!((a - 40.0).abs() < 0.01 && (b - 20.0).abs() < 0.05, "{a} {b}");
        assert!((last.fatigue - a - b).abs() < 1e-9);
        // Der anaerobe Teil klingt langsamer ab als der aerobe
        let rest = forecast(&curve, end, DayLoad::default(), 5);
        let (a5, b5) = rest[4].fatigue_parts.unwrap();
        assert!(a5 / a < b5 / b);
        // Ruhetags-Bonus: schneller als ohne
        let no_bonus = CurveParams { model: CurveModel::TwoCurve { aerobic_days: 5.0, anaerobic_days: 12.0, rest_bonus: 0.0 }, ..STD };
        let (_, c2) = series(&loads, d0, end, no_bonus);
        let slow = forecast(&c2, end, DayLoad::default(), 5);
        assert!(rest[4].fatigue < slow[4].fatigue);
    }

    #[test]
    fn foster_monotony_strain_and_surcharge() {
        let d0 = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        // Woche: 100, 0, 100, 0, 100, 0, 100 → Mittel 57,14, sd 49,49, Monotonie 1,155
        let mut loads = BTreeMap::new();
        for i in 0..7u64 {
            loads.insert(d0 + chrono::Days::new(i), DayLoad { total: if i % 2 == 0 { 100.0 } else { 0.0 }, ..Default::default() });
        }
        let end = d0 + chrono::Days::new(6);
        let (s, _) = series(&loads, d0, end, STD);
        let m = s[6].monotony.unwrap();
        assert!((m - 1.1547).abs() < 1e-3, "{m}");
        assert!((s[6].strain.unwrap() - 400.0 * m).abs() < 1e-6);
        // Fast gleich jeden Tag → hohe Monotonie; der Aufschlag erhöht nur die Ermüdung
        let mut mono = BTreeMap::new();
        for i in 0..7u64 {
            mono.insert(d0 + chrono::Days::new(i), DayLoad { total: 100.0 + i as f64, ..Default::default() });
        }
        let (plain, _) = series(&mono, d0, end, STD);
        let (more, _) = series(&mono, d0, end, CurveParams { foster_surcharge: Some(0.15), ..STD });
        assert!(plain[6].monotony.unwrap() > MONOTONY_WARN);
        assert!(more[6].fatigue > plain[6].fatigue);
        assert_eq!(more[6].fitness, plain[6].fitness);
    }

    #[test]
    fn planned_zone_minutes() {
        let z = zones();
        // 50 + 130 × p: Zonen ab 115, 128, 141, 154, 167; Mitte Z2 = 134,5, Z5 = (167 + 180) / 2 = 173,5
        assert_eq!(z.zone_mid_bpm(1), 134.5);
        assert_eq!(z.zone_mid_bpm(4), 173.5);
        let c = from_zone_minutes(&[0.0, 60.0, 0.0, 10.0, 0.0], &z, 1.92);
        let aer = banister(60.0, (134.5 - 50.0) / 130.0, 1.92);
        let ana = banister(10.0, (160.5 - 50.0) / 130.0, 1.92);
        assert!((c.aerobic - aer).abs() < 1e-9 && (c.anaerobic - ana).abs() < 1e-9, "{c:?}");
        assert_eq!(c.zone_seconds.unwrap()[1], 3600.0);
    }

    #[test]
    fn forecast_with_daily_values() {
        let d0 = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        let (_, curve) = series(&BTreeMap::new(), d0, d0, STD);
        let plan = |d: NaiveDate| DayLoad { total: if d.day() == 3 { 100.0 } else { 0.0 }, ..Default::default() };
        let f = forecast_with(&curve, d0, plan, 4);
        assert_eq!(f.iter().map(|v| v.load.total).collect::<Vec<_>>(), vec![0.0, 100.0, 0.0, 0.0]);
        assert!(f[1].fatigue > 0.0 && f[0].fatigue == 0.0);
    }

    #[test]
    fn tanaka_formula() {
        assert_eq!(tanaka(58), 167);
        assert_eq!(tanaka(30), 187);
    }
}
