//! Planungshilfe (E8 Stufe 2, #12): schlägt bis zum Zieltermin geplante Einheiten vor (RFC-0009 „Trainingsplanung“).
//!
//! **Regeln** legen den Aufbau fest: Blöcke zu sieben Tagen rückwärts ab dem Tapering, jeder vierte als Entlastung
//! (3:1); je Block der längste freie Tag (Wochenende bevorzugt) als lange Einheit, bis zu zwei Intervalltage, nie zwei
//! harte Tage hintereinander, der Rest Grundlage, mindestens ein Ruhetag je Woche.
//! **Optimierung**: In Aufbaublöcken gilt der größte Anteil der verfügbaren Zeit, bei dem die Fitness höchstens um den
//! erlaubten Anstieg wächst; Entlastung = 60 % davon. Für das Tapering sucht TrackCraft Länge (7–14 Tage) und Umfang so,
//! dass die Form am Ziel „frisch“ ist (bevorzugt +10 bis +20) und die Fitness dort möglichst hoch bleibt. Gerechnet wird
//! mit derselben Formkurve wie auf der Formseite, feste Einheiten gehen mit ein.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{Datelike, NaiveDate, TimeDelta, Weekday};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::config::{SPORT_CLASSES, Validated};
use crate::db::{Db, bump_revision, new_id};
use crate::load::model::{self, Curve, DayLoad, DayValue, ZONES};
use crate::load::{self, Settings, ZoneSet};
use crate::plan::{self, Error, Mode, Unit, round1};

/// Kürzeste vorgeschlagene Einheit in Minuten
const MIN_UNIT: u32 = 30;
/// Kürzeste Intervalleinheit (Einfahren, mindestens zwei Wiederholungen, Ausfahren)
const MIN_INTERVALS: u32 = 45;
/// Ab dieser verfügbaren Zeit kann ein Tag die lange Einheit sein
const MIN_LONG: u32 = 90;
const TAPER_MIN: i64 = 7;
const TAPER_MAX: i64 = 14;
/// Entlastungswoche: Anteil des vorherigen Aufbaublocks
const RECOVERY_FACTOR: f64 = 0.6;
/// Im Tapering sinkt der Umfang bis zum letzten Tag um diesen Anteil
const TAPER_DECLINE: f64 = 0.4;
/// Form am Ziel: bevorzugt und Bereich „frisch“ (RFC-0009)
const FORM_PREFERRED: (f64, f64) = (10.0, 20.0);
const FORM_FRESH: (f64, f64) = (5.0, 25.0);
/// Kleinster Anteil der verfügbaren Zeit in Aufbaublöcken
const MIN_FACTOR: f64 = 0.1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Intensity {
    /// Etwa 80 % locker, Intervalle in Zone 4
    Polarized,
    /// Nur Grundlage
    Base,
}

/// Eingaben der Planungshilfe; die zuletzt übernommenen merkt sich TrackCraft.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    pub sport_class: String,
    /// Höchstens so viele Minuten je Wochentag, Montag zuerst; unter 30 = Ruhetag
    pub minutes: [u32; 7],
    /// Höchster Fitnessanstieg je Woche (3 bis 7)
    pub max_gain: u32,
    /// Jede vierte Woche Entlastung (3:1)
    pub recovery_weeks: bool,
    pub intensity: Intensity,
}

fn invalid(field: &'static str, code: &'static str, message: &'static str) -> Error {
    Error::Validation { field, code, message }
}

impl Input {
    pub fn validate(&self) -> Result<(), Error> {
        if !SPORT_CLASSES.contains(&self.sport_class.as_str()) {
            return Err(invalid("sport_class", "invalid", "Unknown sport class"));
        }
        if self.minutes.iter().any(|m| *m > 600) {
            return Err(invalid("minutes", "out_of_range", "Minutes per weekday must be 0 to 600"));
        }
        if self.minutes.iter().all(|m| *m < MIN_UNIT) {
            return Err(invalid("minutes", "required", "At least one weekday needs 30 minutes or more"));
        }
        if !(3..=7).contains(&self.max_gain) {
            return Err(invalid("max_gain", "out_of_range", "Fitness gain per week must be 3 to 7"));
        }
        Ok(())
    }

    fn avail(&self, d: NaiveDate) -> u32 {
        self.minutes[d.weekday().num_days_from_monday() as usize]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Build,
    Recovery,
    Taper,
}

impl Phase {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Build => "build",
            Self::Recovery => "recovery",
            Self::Taper => "taper",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Easy,
    Long,
    Intervals,
}

#[derive(Debug, Clone)]
struct Slot {
    date: NaiveDate,
    avail: u32,
    kind: Kind,
}

#[derive(Debug, Clone)]
struct Block {
    phase: Phase,
    from: NaiveDate,
    to: NaiveDate,
    slots: Vec<Slot>,
}

impl Block {
    fn days(&self) -> i64 {
        (self.to - self.from).num_days() + 1
    }
}

/// Alles, was die Rechnung braucht, ohne Datenbank.
pub struct Context<'a> {
    pub today: NaiveDate,
    pub goal: NaiveDate,
    /// Zustand der Formkurve nach heute
    pub curve: Curve,
    /// Feste offene Einheiten nach heute
    pub fixed: Vec<Unit>,
    pub sets: &'a [ZoneSet],
    pub settings: &'a Settings,
}

fn day(d: NaiveDate, n: i64) -> NaiveDate {
    d + TimeDelta::days(n)
}

fn round5(m: f64) -> u32 {
    ((m / 5.0).round() * 5.0).max(0.0) as u32
}

/// Feste Einheit zählt als harter Tag, wenn sie intensiv oder lang ist.
fn is_hard(u: &Unit) -> bool {
    let intensive = match &u.mode {
        Mode::Zones(z) => z[3] + z[4] > 0,
        Mode::Target { intensive_pct, .. } => *intensive_pct > 0,
    };
    intensive || u.minutes() >= MIN_LONG
}

fn neighbours_hard(hard: &BTreeSet<NaiveDate>, d: NaiveDate) -> bool {
    hard.contains(&day(d, -1)) || hard.contains(&day(d, 1))
}

/// Blöcke und Tagesrollen nach den Regeln, für ein Tapering von `taper` Tagen.
fn layout(ctx: &Context, input: &Input, taper: i64) -> Vec<Block> {
    let first = day(ctx.today, 1);
    let last = day(ctx.goal, -1);
    let taper_from = day(ctx.goal, -taper);
    let fixed_days: BTreeSet<NaiveDate> = ctx.fixed.iter().map(|u| u.date).collect();
    let mut hard: BTreeSet<NaiveDate> = ctx.fixed.iter().filter(|u| is_hard(u)).map(|u| u.date).collect();

    // Aufbaublöcke rückwärts ab dem Tapering, jeder vierte Entlastung
    let mut blocks = Vec::new();
    let mut end = day(taper_from, -1);
    let mut k = 1;
    while end >= first {
        let start = day(end, -6).max(first);
        let phase = if input.recovery_weeks && k % 4 == 0 { Phase::Recovery } else { Phase::Build };
        blocks.push(Block { phase, from: start, to: end, slots: Vec::new() });
        end = day(start, -1);
        k += 1;
    }
    blocks.reverse();
    blocks.push(Block { phase: Phase::Taper, from: taper_from, to: last, slots: Vec::new() });

    // Freie Tage; ein voller Block mit sieben Trainingstagen bekommt einen Ruhetag (der kürzeste freie Tag)
    for b in &mut blocks {
        let mut free: Vec<(NaiveDate, u32)> = (0..b.days())
            .map(|i| day(b.from, i))
            .filter(|d| !fixed_days.contains(d))
            .map(|d| (d, input.avail(d)))
            .filter(|(_, m)| *m >= MIN_UNIT)
            .collect();
        let fixed_in = (0..b.days()).filter(|i| fixed_days.contains(&day(b.from, *i))).count();
        if b.days() == 7
            && free.len() + fixed_in >= 7
            && let Some(i) = free.iter().enumerate().min_by_key(|(_, (d, m))| (*m, std::cmp::Reverse(*d))).map(|(i, _)| i)
        {
            free.remove(i);
        }
        b.slots = free.into_iter().map(|(date, avail)| Slot { date, avail, kind: Kind::Easy }).collect();
    }

    // Lange Einheit: längster freier Tag, Wochenende bevorzugt, nicht neben einem anderen harten Tag
    for b in &mut blocks {
        let pick = b
            .slots
            .iter()
            .enumerate()
            .filter(|(_, s)| s.avail >= MIN_LONG && !neighbours_hard(&hard, s.date))
            .max_by_key(|(_, s)| (s.avail, matches!(s.date.weekday(), Weekday::Sat | Weekday::Sun), s.date))
            .map(|(i, _)| i);
        if let Some(i) = pick {
            b.slots[i].kind = Kind::Long;
            hard.insert(b.slots[i].date);
        }
    }

    // Intervalle: je nach Phase und freien Tagen, die längsten passenden Tage
    for b in &mut blocks {
        let n_free = b.slots.len();
        let n = match (input.intensity, b.phase) {
            (Intensity::Base, _) => 0,
            (Intensity::Polarized, Phase::Build) => match n_free {
                0 | 1 => 0,
                2 | 3 => 1,
                _ => 2,
            },
            (Intensity::Polarized, Phase::Recovery) => usize::from(n_free >= 3),
            (Intensity::Polarized, Phase::Taper) => {
                if n_free >= 2 {
                    (b.days() as usize).div_ceil(7).min(2)
                } else {
                    0
                }
            }
        };
        let mut order: Vec<usize> = (0..n_free).filter(|i| b.slots[*i].kind == Kind::Easy && b.slots[*i].avail >= MIN_INTERVALS).collect();
        order.sort_by_key(|i| (std::cmp::Reverse(b.slots[*i].avail), b.slots[*i].date));
        let mut chosen = 0;
        for i in order {
            if chosen == n {
                break;
            }
            let d = b.slots[i].date;
            if hard.contains(&d) || neighbours_hard(&hard, d) {
                continue;
            }
            b.slots[i].kind = Kind::Intervals;
            hard.insert(d);
            chosen += 1;
        }
    }
    blocks
}

/// Einheit für einen Tag bei Anteil `f` der verfügbaren Zeit; `None`, wenn sie zu kurz würde.
fn make_unit(ctx: &Context, input: &Input, slot: &Slot, phase: Phase, f: f64) -> Option<Unit> {
    let want = f * slot.avail as f64;
    if want < MIN_UNIT as f64 - 2.5 {
        return None;
    }
    let (title, zones): (String, [u32; ZONES]) = match slot.kind {
        Kind::Easy | Kind::Long => {
            let d = round5(want).clamp(MIN_UNIT, slot.avail);
            let (share, title) = match (slot.kind, phase) {
                (Kind::Long, _) => (0.1, "Lange Einheit"),
                (_, Phase::Recovery) => (0.4, "Locker"),
                _ => (0.15, "Grundlage"),
            };
            let z1 = round5(d as f64 * share).clamp(5, d);
            (title.to_string(), [z1, d - z1, 0, 0, 0])
        }
        Kind::Intervals => {
            let d = round5(want).clamp(MIN_INTERVALS, slot.avail);
            // 15 min Einfahren (5 Z1, 10 Z2), Wiederholungen 6 min Z4 mit 3 min Z2, 10 min Ausfahren Z1
            let main = d - 25;
            let max_reps = if phase == Phase::Taper { 4 } else { 6 };
            let reps = (main / 9).clamp(2, max_reps);
            let z4 = reps * 6;
            let z2 = 10 + (main - z4);
            (format!("Intervalle {reps}×6 min"), [15, z2, 0, z4, 0])
        }
    };
    let minutes: u32 = zones.iter().sum();
    let mode = if load::applicable(ctx.sets, &input.sport_class, slot.date).is_some() {
        Mode::Zones(zones)
    } else {
        // Ohne Zonensatz: Zielbelastung aus der Belastung je Stunde der Sportklasse
        let per_hour = ctx.settings.estimate_per_hour.get(&input.sport_class).copied().unwrap_or(0.0);
        let intensive = zones[3] + zones[4];
        let factor = if intensive > 0 { 1.4 } else { 1.0 };
        Mode::Target {
            minutes,
            load: round1(per_hour * minutes as f64 / 60.0 * factor),
            intensive_pct: (intensive * 100).div_ceil(minutes.max(1)),
        }
    };
    Some(Unit {
        id: String::new(),
        date: slot.date,
        sport_class: input.sport_class.clone(),
        sub_sport: None,
        title,
        note: None,
        mode,
        status: "planned".into(),
        activity_id: None,
        suggestion_id: None,
        series_id: None,
        series_following: 0,
    })
}

fn block_units(ctx: &Context, input: &Input, b: &Block, f: impl Fn(NaiveDate) -> f64) -> Vec<Unit> {
    b.slots.iter().filter_map(|s| make_unit(ctx, input, s, b.phase, f(s.date))).collect()
}

fn loads_of<'u>(ctx: &Context, units: impl IntoIterator<Item = &'u Unit>) -> BTreeMap<NaiveDate, DayLoad> {
    let mut out: BTreeMap<NaiveDate, DayLoad> = BTreeMap::new();
    for u in units {
        plan::add_load(out.entry(u.date).or_default(), u, ctx.sets, ctx.settings);
    }
    out
}

/// Schreibt die Kurve von `from` bis `to` fort.
fn advance(curve: &mut Curve, from: NaiveDate, to: NaiveDate, loads: &BTreeMap<NaiveDate, DayLoad>) -> Vec<DayValue> {
    let mut out = Vec::new();
    let mut d = from;
    while d <= to {
        out.push(curve.step(d, loads.get(&d).copied().unwrap_or_default()));
        d = day(d, 1);
    }
    out
}

/// Ein Aufbaublock: größter Anteil, bei dem die Fitness höchstens um `cap` steigt. Liefert auch, ob die Zeit begrenzt.
fn build_factor(ctx: &Context, input: &Input, b: &Block, curve: &Curve, fixed: &BTreeMap<NaiveDate, DayLoad>, cap: f64) -> (f64, bool) {
    let start = curve.fitness();
    let gain = |f: f64| {
        let units = block_units(ctx, input, b, |_| f);
        let mut loads = loads_of(ctx, &units);
        for (d, l) in fixed.range(b.from..=b.to) {
            let e = loads.entry(*d).or_default();
            e.total += l.total;
            e.aerobic += l.aerobic;
            e.anaerobic += l.anaerobic;
            e.estimated += l.estimated;
        }
        let mut c = curve.clone();
        advance(&mut c, b.from, b.to, &loads);
        c.fitness() - start
    };
    if gain(1.0) <= cap {
        return (1.0, true);
    }
    if gain(MIN_FACTOR) > cap {
        return (MIN_FACTOR, false);
    }
    let (mut lo, mut hi) = (MIN_FACTOR, 1.0);
    for _ in 0..18 {
        let mid = (lo + hi) / 2.0;
        if gain(mid) <= cap { lo = mid } else { hi = mid }
    }
    (lo, false)
}

fn merge(a: &mut BTreeMap<NaiveDate, DayLoad>, b: &BTreeMap<NaiveDate, DayLoad>) {
    for (d, l) in b {
        let e = a.entry(*d).or_default();
        e.total += l.total;
        e.aerobic += l.aerobic;
        e.anaerobic += l.anaerobic;
        e.estimated += l.estimated;
    }
}

#[derive(Debug, Clone)]
pub struct BlockInfo {
    pub phase: Phase,
    pub from: NaiveDate,
    pub to: NaiveDate,
    /// Anteil der verfügbaren Zeit (im Tapering zu Beginn)
    pub factor: f64,
}

#[derive(Debug, Clone)]
pub struct Suggestion {
    pub units: Vec<(Unit, Phase)>,
    pub blocks: Vec<BlockInfo>,
    pub taper_days: u32,
    pub fitness_today: f64,
    pub fitness_at_goal: f64,
    pub form_at_goal: f64,
    /// Mittlerer Fitnessanstieg je Woche bis zum Tapering
    pub avg_gain: Option<f64>,
    /// Mindestens ein Aufbaublock nutzt die ganze verfügbare Zeit und bleibt unter dem erlaubten Anstieg
    pub limited_by_time: bool,
    /// Vorschau ab morgen bis drei Tage nach dem Ziel, mit festen und vorgeschlagenen Einheiten
    pub forecast: Vec<DayValue>,
}

/// Rang einer Lösung: zuerst die Lage der Form am Ziel, dann die Fitness (größer ist besser).
fn rank(form: f64, fitness: f64) -> (u8, f64) {
    let inside = |r: (f64, f64)| form >= r.0 && form <= r.1;
    if inside(FORM_PREFERRED) {
        (0, fitness)
    } else if inside(FORM_FRESH) {
        (1, fitness)
    } else {
        (2, -(FORM_FRESH.0 - form).max(form - FORM_FRESH.1))
    }
}

fn better(a: (u8, f64), b: (u8, f64)) -> bool {
    a.0 < b.0 || (a.0 == b.0 && a.1 > b.1 + 1e-9)
}

/// Vorschlag bis zum Ziel. Das Ziel muss mindestens 8 Tage entfernt sein (7 Tage Tapering).
pub fn suggest(ctx: &Context, input: &Input) -> Result<Suggestion, Error> {
    let days_to_goal = (ctx.goal - ctx.today).num_days();
    if days_to_goal <= TAPER_MIN {
        return Err(invalid("goal", "too_close", "The goal must be at least 8 days ahead"));
    }
    let fixed = loads_of(ctx, &ctx.fixed);
    let cap_week = input.max_gain as f64;
    let mut best: Option<((u8, f64), Suggestion)> = None;

    for taper in TAPER_MIN..=TAPER_MAX.min(days_to_goal - 1) {
        let blocks = layout(ctx, input, taper);
        let mut curve = ctx.curve.clone();
        let mut units: Vec<(Unit, Phase)> = Vec::new();
        let mut infos = Vec::new();
        let mut f_build: Option<f64> = None;
        let mut limited = false;
        let mut build_days = 0;
        let (taper_block, build_blocks) = blocks.split_last().expect("Tapering");
        for b in build_blocks {
            let cap = cap_week * b.days() as f64 / 7.0;
            let f = match b.phase {
                Phase::Recovery => RECOVERY_FACTOR * f_build.unwrap_or_else(|| build_factor(ctx, input, b, &curve, &fixed, cap).0),
                _ => {
                    let (f, l) = build_factor(ctx, input, b, &curve, &fixed, cap);
                    limited |= l;
                    f_build = Some(f);
                    f
                }
            };
            build_days += b.days();
            let us = block_units(ctx, input, b, |_| f);
            let mut loads = loads_of(ctx, &us);
            merge(&mut loads, &fixed.range(b.from..=b.to).map(|(d, l)| (*d, *l)).collect());
            advance(&mut curve, b.from, b.to, &loads);
            infos.push(BlockInfo { phase: b.phase, from: b.from, to: b.to, factor: f });
            units.extend(us.into_iter().map(|u| (u, b.phase)));
        }
        let fitness_taper = curve.fitness();
        let f_ref = f_build.unwrap_or(1.0);
        let t = taper_block.days() as f64;
        for step in 4..=16 {
            let alpha = step as f64 * 0.05;
            let factor = |d: NaiveDate| {
                let i = (d - taper_block.from).num_days() as f64;
                (f_ref * alpha * (1.0 - TAPER_DECLINE * i / (t - 1.0).max(1.0))).min(1.0)
            };
            let tu = block_units(ctx, input, taper_block, factor);
            let mut loads = loads_of(ctx, &tu);
            merge(&mut loads, &fixed.range(taper_block.from..).map(|(d, l)| (*d, *l)).collect());
            let mut c = curve.clone();
            advance(&mut c, taper_block.from, taper_block.to, &loads);
            let fitness_goal = c.fitness();
            let goal_day = c.step(ctx.goal, loads.get(&ctx.goal).copied().unwrap_or_default());
            let r = rank(goal_day.form, fitness_goal);
            if best.as_ref().is_none_or(|(br, _)| better(r, *br)) {
                let mut all = units.clone();
                all.extend(tu.into_iter().map(|u| (u, Phase::Taper)));
                let mut blocks = infos.clone();
                blocks.push(BlockInfo { phase: Phase::Taper, from: taper_block.from, to: taper_block.to, factor: f_ref * alpha });
                best = Some((
                    r,
                    Suggestion {
                        units: all,
                        blocks,
                        taper_days: taper as u32,
                        fitness_today: ctx.curve.fitness(),
                        fitness_at_goal: fitness_goal,
                        form_at_goal: goal_day.form,
                        avg_gain: (build_days > 0).then(|| (fitness_taper - ctx.curve.fitness()) / (build_days as f64 / 7.0)),
                        limited_by_time: limited,
                        forecast: Vec::new(),
                    },
                ));
            }
        }
    }
    let (_, mut s) = best.expect("mindestens ein Tapering");
    // Vorschau ab morgen bis drei Tage nach dem Ziel
    let mut loads = loads_of(ctx, s.units.iter().map(|(u, _)| u));
    merge(&mut loads, &fixed);
    let mut c = ctx.curve.clone();
    s.forecast = advance(&mut c, day(ctx.today, 1), day(ctx.goal, 3), &loads);
    Ok(s)
}

/// Zustand der Form nach RFC-0009 (Schlüssel wie im Frontend).
pub fn form_state(form: f64) -> &'static str {
    if form > 25.0 {
        "rested"
    } else if form > 5.0 {
        "fresh"
    } else if form > -10.0 {
        "neutral"
    } else if form > -30.0 {
        "building"
    } else {
        "overload"
    }
}

impl Suggestion {
    /// Kalenderwochen (Montag) mit Belastung, Monotonie, Anteil Zone 1–2 und überwiegender Phase.
    fn weeks(&self, ctx: &Context) -> Vec<Value> {
        let phase_of = |d: NaiveDate| self.blocks.iter().find(|b| b.from <= d && d <= b.to).map(|b| b.phase);
        let loads: BTreeMap<NaiveDate, f64> = self.forecast.iter().map(|v| (v.date, v.load.total)).collect();
        let first = day(ctx.today, 1);
        let mut monday = day(first, -(first.weekday().num_days_from_monday() as i64));
        let mut out = Vec::new();
        while monday < ctx.goal {
            let days: Vec<NaiveDate> = (0..7).map(|i| day(monday, i)).collect();
            let values: Vec<f64> = days.iter().map(|d| loads.get(d).copied().unwrap_or(0.0)).collect();
            let (monotony, _) = model::foster(values.iter().copied());
            let mut counts = [0usize; 3];
            for d in &days {
                match phase_of(*d) {
                    Some(Phase::Build) => counts[0] += 1,
                    Some(Phase::Recovery) => counts[1] += 1,
                    Some(Phase::Taper) => counts[2] += 1,
                    None => {}
                }
            }
            let phase = [Phase::Build, Phase::Recovery, Phase::Taper]
                .into_iter()
                .zip(counts)
                .filter(|(_, n)| *n > 0)
                .max_by_key(|(p, n)| (*n, *p == Phase::Taper))
                .map(|(p, _)| p.as_str());
            let (mut low, mut all) = (0u32, 0u32);
            for (u, _) in self.units.iter().filter(|(u, _)| days.contains(&u.date)) {
                if let Mode::Zones(z) = &u.mode {
                    low += z[0] + z[1];
                    all += z.iter().sum::<u32>();
                }
            }
            out.push(json!({
                "start": monday.to_string(),
                "phase": phase,
                "load": round1(values.iter().sum()),
                "monotony": monotony.map(|m| (m * 100.0).round() / 100.0),
                "low_zone_pct": (all > 0).then(|| (low as f64 * 100.0 / all as f64).round()),
                "units": self.units.iter().filter(|(u, _)| days.contains(&u.date)).count(),
            }));
            monday = day(monday, 7);
        }
        out
    }

    pub fn to_json(&self, ctx: &Context) -> Value {
        json!({
            "today": ctx.today.to_string(),
            "goal_date": ctx.goal.to_string(),
            "units": self.units.iter().map(|(u, p)| {
                let mut v = u.to_json(ctx.sets, ctx.settings);
                v["phase"] = json!(p.as_str());
                v
            }).collect::<Vec<_>>(),
            "fixed_units": ctx.fixed.iter().filter(|u| u.date < ctx.goal).count(),
            "blocks": self.blocks.iter().map(|b| json!({
                "phase": b.phase.as_str(),
                "from": b.from.to_string(),
                "to": b.to.to_string(),
                "factor": (b.factor * 100.0).round() / 100.0,
            })).collect::<Vec<_>>(),
            "weeks": self.weeks(ctx),
            "summary": {
                "taper_days": self.taper_days,
                "fitness_today": round1(self.fitness_today),
                "fitness_at_goal": round1(self.fitness_at_goal),
                "form_at_goal": round1(self.form_at_goal),
                "form_state": form_state(self.form_at_goal),
                "avg_gain": self.avg_gain.map(round1),
                "max_gain": null,
                "limited_by_time": self.limited_by_time,
                "planned_load": round1(self.units.iter().map(|(u, _)| u.load(ctx.sets, ctx.settings).0.trimp).sum()),
            },
            "forecast": self.forecast.iter().map(|d| load::day_json(d, false)).collect::<Vec<_>>(),
        })
    }
}

// ---------------------------------------------------------------------------------------------------------
// Datenbank

/// Zuletzt übernommene Eingaben oder `null`.
pub async fn saved_input(db: &Db) -> anyhow::Result<Value> {
    let s: Option<String> = sqlx::query_scalar("SELECT assistant FROM plan_settings WHERE id = 1").fetch_one(&db.reader).await?;
    Ok(s.and_then(|s| serde_json::from_str::<Input>(&s).ok()).map(|i| json!(i)).unwrap_or(Value::Null))
}

struct Loaded {
    curve: Curve,
    settings: Settings,
    sets: Vec<ZoneSet>,
    fixed: Vec<Unit>,
    goal: NaiveDate,
}

async fn prepare(cfg: &Validated, db: &Db, input: &Input, today: NaiveDate) -> Result<Loaded, Error> {
    input.validate()?;
    let goal = plan::goal_date(db).await?.ok_or(invalid("goal", "required", "Set a goal first"))?;
    let (curve, settings) = load::curve_at(cfg, db, today).await?;
    let sets = load::zone_sets(db).await?;
    let per_hour = settings.estimate_per_hour.get(&input.sport_class).copied().unwrap_or(0.0);
    if load::applicable(&sets, &input.sport_class, goal).is_none() && per_hour <= 0.0 {
        return Err(invalid("sport_class", "no_load_basis", "No heart rate zones and no load per hour for this sport class"));
    }
    let fixed = plan::open_units(db, today, false).await?;
    Ok(Loaded { curve, settings, sets, fixed, goal })
}

fn context<'a>(l: &'a Loaded, today: NaiveDate) -> Context<'a> {
    Context { today, goal: l.goal, curve: l.curve.clone(), fixed: l.fixed.clone(), sets: &l.sets, settings: &l.settings }
}

/// Vorschlag berechnen, ohne zu speichern.
pub async fn preview(cfg: &Validated, db: &Db, input: &Input, today: NaiveDate) -> Result<Value, Error> {
    let l = prepare(cfg, db, input, today).await?;
    let ctx = context(&l, today);
    let s = suggest(&ctx, input)?;
    let mut v = s.to_json(&ctx);
    v["summary"]["max_gain"] = json!(input.max_gain);
    v["replaces"] = json!(replaceable(db, today).await?);
    Ok(v)
}

async fn replaceable(db: &Db, today: NaiveDate) -> anyhow::Result<i64> {
    Ok(sqlx::query_scalar(
        "SELECT COUNT(*) FROM planned_units WHERE suggestion_id IS NOT NULL AND status = 'planned' AND activity_id IS NULL AND date > ?",
    )
    .bind(today.to_string())
    .fetch_one(&db.reader)
    .await?)
}

/// Vorschlag neu berechnen und übernehmen: offene, unveränderte Einheiten eines früheren Vorschlags werden ersetzt.
pub async fn apply(cfg: &Validated, db: &Db, input: &Input, today: NaiveDate) -> Result<Value, Error> {
    let l = prepare(cfg, db, input, today).await?;
    let ctx = context(&l, today);
    let s = suggest(&ctx, input)?;
    let suggestion_id = new_id();
    let now = chrono::Utc::now().timestamp_millis();
    let mut tx = db.writer.begin().await?;
    let replaced = sqlx::query(
        "DELETE FROM planned_units WHERE suggestion_id IS NOT NULL AND status = 'planned' AND activity_id IS NULL AND date > ?",
    )
    .bind(today.to_string())
    .execute(&mut *tx)
    .await?
    .rows_affected();
    for (u, _) in &s.units {
        let u = Unit { id: new_id(), suggestion_id: Some(suggestion_id.clone()), ..u.clone() };
        plan::save(&mut tx, &u, true, now).await?;
    }
    sqlx::query("UPDATE plan_settings SET assistant = ?, updated_at_ms = ? WHERE id = 1")
        .bind(serde_json::to_string(input)?)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    bump_revision(&mut tx, now).await?;
    tx.commit().await?;
    Ok(json!({ "created": s.units.len(), "replaced": replaced, "suggestion_id": suggestion_id }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::load::model::{CurveModel, CurveParams, Model, Zones};

    const STD: CurveParams =
        CurveParams { fitness_days: 42.0, fatigue_weight: 1.0, model: CurveModel::Standard { fatigue_days: 7.0 }, foster_surcharge: None };

    fn settings() -> Settings {
        Settings {
            sex_factor: 1.92,
            fitness_days: 42,
            fatigue_days: 7,
            estimate_per_hour: [("cycling".to_string(), 50.0)].into_iter().collect(),
            curve_model: "standard".into(),
            aerobic_days: 5,
            anaerobic_days: 12,
            rest_bonus_pct: 18,
            foster_surcharge: false,
            foster_surcharge_pct: 15,
            fatigue_weight: 1.0,
        }
    }

    fn sets() -> Vec<ZoneSet> {
        vec![ZoneSet {
            id: "z".into(),
            sport_class: None,
            valid_from: NaiveDate::from_ymd_opt(2020, 1, 1).unwrap(),
            zones: Zones { max_hr: 180, resting_hr: 50, threshold_hr: None, model: Model::Reserve, bounds: [50, 60, 70, 80, 90] },
        }]
    }

    fn input() -> Input {
        Input {
            sport_class: "cycling".into(),
            minutes: [0, 60, 0, 75, 0, 180, 120],
            max_gain: 5,
            recovery_weeks: true,
            intensity: Intensity::Polarized,
        }
    }

    /// Kurve nach 90 Tagen mit gleichmäßig 40 je Tag.
    fn trained(today: NaiveDate) -> Curve {
        let mut c = Curve::new(STD);
        for i in (0..90).rev() {
            c.step(day(today, -i), DayLoad { total: 40.0, aerobic: 40.0, ..Default::default() });
        }
        c
    }

    fn hard_title(u: &Unit) -> bool {
        u.title.starts_with("Lange") || u.title.starts_with("Intervalle")
    }

    fn ctx<'a>(today: NaiveDate, goal: NaiveDate, fixed: Vec<Unit>, sets: &'a [ZoneSet], s: &'a Settings) -> Context<'a> {
        Context { today, goal, curve: trained(today), fixed, sets, settings: s }
    }

    #[test]
    fn suggestion_reaches_fresh_form_and_follows_rules() {
        let (s, z) = (settings(), sets());
        let today = NaiveDate::from_ymd_opt(2026, 11, 23).unwrap();
        let goal = NaiveDate::from_ymd_opt(2027, 2, 7).unwrap();
        let c = ctx(today, goal, Vec::new(), &z, &s);
        let r = suggest(&c, &input()).unwrap();
        assert!((7..=14).contains(&r.taper_days));
        assert!(r.form_at_goal >= 5.0 && r.form_at_goal <= 25.0, "Form {}", r.form_at_goal);
        assert!(r.fitness_at_goal > r.fitness_today, "{} {}", r.fitness_at_goal, r.fitness_today);
        // Nur verfügbare Tage, nie über der verfügbaren Zeit, alles Zonenminuten
        for (u, _) in &r.units {
            let avail = input().avail(u.date);
            assert!(u.date > today && u.date < goal);
            assert!(avail >= MIN_UNIT && u.minutes() <= avail, "{} {} {}", u.date, u.minutes(), avail);
            assert!(matches!(u.mode, Mode::Zones(_)));
        }
        // Nie zwei harte Tage (lange Einheit, Intervalle) hintereinander
        let hard: BTreeSet<NaiveDate> = r.units.iter().filter(|(u, _)| hard_title(u)).map(|(u, _)| u.date).collect();
        for d in &hard {
            assert!(!hard.contains(&day(*d, 1)), "harte Tage {d} und danach");
        }
        // 3:1: jeder vierte Block vor dem Tapering ist Entlastung
        let phases: Vec<Phase> = r.blocks.iter().map(|b| b.phase).collect();
        assert_eq!(*phases.last().unwrap(), Phase::Taper);
        assert!(phases.contains(&Phase::Recovery));
        // Anstieg je Aufbaublock nicht über der Vorgabe (Fitness nach dem Tag aus der Vorschau)
        let fitness: BTreeMap<NaiveDate, f64> = r.forecast.iter().map(|v| (v.date, v.fitness)).collect();
        let before = |d: NaiveDate| if d == day(today, 1) { c.curve.fitness() } else { fitness[&day(d, -1)] };
        for b in r.blocks.iter().filter(|b| b.phase == Phase::Build) {
            let gain = fitness[&b.to] - before(b.from);
            let days = (b.to - b.from).num_days() as f64 + 1.0;
            assert!(gain <= 5.0 * days / 7.0 + 0.05, "Anstieg {gain} in {days} Tagen");
        }
        let v = r.to_json(&c);
        assert_eq!(v["summary"]["form_state"], "fresh");
        assert!(!v["weeks"].as_array().unwrap().is_empty());
        assert_eq!(v["forecast"].as_array().unwrap().last().unwrap()["date"], day(goal, 3).to_string());
    }

    #[test]
    fn fixed_units_stay_and_block_their_day() {
        let (s, z) = (settings(), sets());
        let today = NaiveDate::from_ymd_opt(2026, 11, 23).unwrap();
        let goal = day(today, 40);
        let sunday = day(today, 6); // 29.11.2026 ist ein Sonntag
        assert_eq!(sunday.weekday(), Weekday::Sun);
        let fixed = Unit {
            id: "f".into(),
            date: sunday,
            sport_class: "cycling".into(),
            sub_sport: None,
            title: "Vereinsausfahrt".into(),
            note: None,
            mode: Mode::Zones([10, 100, 10, 0, 0]),
            status: "planned".into(),
            activity_id: None,
            suggestion_id: None,
            series_id: None,
            series_following: 0,
        };
        let c = ctx(today, goal, vec![fixed], &z, &s);
        let r = suggest(&c, &input()).unwrap();
        assert!(r.units.iter().all(|(u, _)| u.date != sunday));
        // Die feste lange Einheit ist ein harter Tag: Samstag davor keine lange Einheit und keine Intervalle
        assert!(r.units.iter().filter(|(u, _)| u.date == day(sunday, -1)).all(|(u, _)| !hard_title(u)));
        assert!(r.forecast.iter().find(|v| v.date == sunday).unwrap().load.total > 0.0);
    }

    #[test]
    fn base_only_has_no_intervals_and_target_without_zones() {
        let s = settings();
        let today = NaiveDate::from_ymd_opt(2026, 11, 23).unwrap();
        let c = ctx(today, day(today, 30), Vec::new(), &[], &s);
        let r = suggest(&c, &Input { intensity: Intensity::Base, ..input() }).unwrap();
        assert!(!r.units.is_empty());
        for (u, _) in &r.units {
            match &u.mode {
                Mode::Target { intensive_pct, load, .. } => assert!(*intensive_pct == 0 && *load > 0.0),
                Mode::Zones(_) => panic!("ohne Zonensatz Zielbelastung"),
            }
        }
    }

    #[test]
    fn goal_too_close_and_validation() {
        let (s, z) = (settings(), sets());
        let today = NaiveDate::from_ymd_opt(2026, 11, 23).unwrap();
        let c = ctx(today, day(today, 7), Vec::new(), &z, &s);
        assert!(matches!(suggest(&c, &input()), Err(Error::Validation { code: "too_close", .. })));
        let c = ctx(today, day(today, 8), Vec::new(), &z, &s);
        let r = suggest(&c, &input()).unwrap();
        assert_eq!(r.taper_days, 7);
        assert!(r.blocks.len() == 1 && r.avg_gain.is_none());
        assert!(Input { max_gain: 8, ..input() }.validate().is_err());
        assert!(Input { minutes: [20; 7], ..input() }.validate().is_err());
        assert!(Input { sport_class: "unknown".into(), ..input() }.validate().is_err());
    }

    #[test]
    fn seven_free_days_keep_one_rest_day() {
        let (s, z) = (settings(), sets());
        let today = NaiveDate::from_ymd_opt(2026, 11, 23).unwrap();
        let c = ctx(today, day(today, 60), Vec::new(), &z, &s);
        let blocks = layout(&c, &Input { minutes: [60; 7], ..input() }, 10);
        for b in blocks.iter().filter(|b| b.days() == 7) {
            assert!(b.slots.len() <= 6);
        }
    }
}
