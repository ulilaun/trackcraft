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

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DayValue {
    pub date: NaiveDate,
    pub load: DayLoad,
    /// Fitness nach dem Training des Tages
    pub fitness: f64,
    /// Ermüdung nach dem Training des Tages
    pub fatigue: f64,
    /// Form zu Beginn des Tages (Fitness − Ermüdung des Vortags)
    pub form: f64,
}

/// Fitness und Ermüdung als exponentiell gleitende Mittel (Standardmodell), Tag für Tag von `start` bis `end`.
/// Fehlende Tage haben die Belastung 0.
pub fn series(
    loads: &BTreeMap<NaiveDate, DayLoad>,
    start: NaiveDate,
    end: NaiveDate,
    fitness_days: f64,
    fatigue_days: f64,
) -> Vec<DayValue> {
    let k1 = 1.0 - (-1.0 / fitness_days).exp();
    let k2 = 1.0 - (-1.0 / fatigue_days).exp();
    let (mut ctl, mut atl) = (0.0_f64, 0.0_f64);
    let mut out = Vec::new();
    let mut d = start;
    while d <= end {
        let load = loads.get(&d).copied().unwrap_or_default();
        let form = ctl - atl;
        ctl += (load.total - ctl) * k1;
        atl += (load.total - atl) * k2;
        out.push(DayValue { date: d, load, fitness: ctl, fatigue: atl, form });
        d = d.succ_opt().expect("Datum");
    }
    out
}

/// Führt die Kurve ab dem Tag nach `last` mit gleicher Tagesbelastung `daily` für `days` Tage fort.
pub fn forecast(last: &DayValue, daily: f64, days: u32, fitness_days: f64, fatigue_days: f64) -> Vec<DayValue> {
    let k1 = 1.0 - (-1.0 / fitness_days).exp();
    let k2 = 1.0 - (-1.0 / fatigue_days).exp();
    let (mut ctl, mut atl) = (last.fitness, last.fatigue);
    let mut d = last.date;
    (0..days)
        .map(|_| {
            d = d.succ_opt().expect("Datum");
            let form = ctl - atl;
            ctl += (daily - ctl) * k1;
            atl += (daily - atl) * k2;
            DayValue { date: d, load: DayLoad { total: daily, ..Default::default() }, fitness: ctl, fatigue: atl, form }
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

    #[test]
    fn steady_load_converges_and_rest_recovers() {
        let d0 = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        let mut loads = BTreeMap::new();
        for i in 0..200 {
            loads.insert(d0 + chrono::Days::new(i), DayLoad { total: 50.0, ..Default::default() });
        }
        let s = series(&loads, d0, d0 + chrono::Days::new(199), 42.0, 7.0);
        let last = s.last().unwrap();
        assert!((last.fatigue - 50.0).abs() < 0.01);
        assert!(last.fitness > 49.0 && last.fitness < 50.0);
        assert!(s[0].form == 0.0 && s[1].form < 0.0);
        // Ruhe: Form steigt, Fitness sinkt
        let f = forecast(last, 0.0, 14, 42.0, 7.0);
        assert_eq!(f.len(), 14);
        assert!(f[13].fitness < last.fitness && f[13].form > 20.0);
        assert_eq!(f[0].date, last.date.succ_opt().unwrap());
    }

    #[test]
    fn tanaka_formula() {
        assert_eq!(tanaka(58), 167);
        assert_eq!(tanaka(30), 187);
    }
}
