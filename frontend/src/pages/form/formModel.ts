// Seite „Form“ (E7): Zustand der Form, Wochensummen und Diagrammdaten; reine Logik ohne React.
import type { FormDay, FormForecastDay, ForecastMode, HrZoneSet, ZoneModel } from "../../api/client";
import { addDays } from "../../utils/format";

/** Zustand der Form nach festen Grenzen (Uli 22.11.2026). */
export type FormState = "rested" | "fresh" | "neutral" | "building" | "overload";

export function formState(form: number): FormState {
  if (form > 25) return "rested";
  if (form > 5) return "fresh";
  if (form > -10) return "neutral";
  if (form > -30) return "building";
  return "overload";
}

/** Farbe des Zustands als Tailwind-Klassen (Schrift und leichter Hintergrund). */
export const STATE_CLASS: Record<FormState, string> = {
  rested: "text-muted bg-muted/15",
  fresh: "text-good bg-good/15",
  neutral: "text-accent bg-accent/15",
  building: "text-warn bg-warn/15",
  overload: "text-danger bg-danger/15",
};

export interface WeekBar {
  /** Montag der Woche */
  week: string;
  aerobic: number;
  anaerobic: number;
  estimated: number;
  total: number;
}

/** Montag zu einem Tag (`JJJJ-MM-TT`). */
export function monday(day: string): string {
  const wd = new Date(`${day}T12:00:00Z`).getUTCDay();
  return addDays(day, -((wd + 6) % 7));
}

/** Tageswerte zu Wochensummen (Montag bis Sonntag). */
export function weekBars(days: FormDay[]): WeekBar[] {
  const out: WeekBar[] = [];
  for (const d of days) {
    const w = monday(d.date);
    let bar = out[out.length - 1];
    if (!bar || bar.week !== w) {
      bar = { week: w, aerobic: 0, anaerobic: 0, estimated: 0, total: 0 };
      out.push(bar);
    }
    bar.aerobic += d.aerobic;
    bar.anaerobic += d.anaerobic;
    bar.estimated += d.estimated;
    bar.total += d.load;
  }
  return out;
}

/** Punkt im Diagramm: gemessene Werte oder (gestrichelte) Vorschau; heute trägt beide, damit die Linien anschließen. */
export interface ChartPoint {
  date: string;
  load?: number;
  fitness?: number;
  fatigue?: number;
  form?: number;
  fitnessForecast?: number;
  fatigueForecast?: number;
  formForecast?: number;
  /** Zwei-Kurven-Modell: Teile der Ermüdung */
  fatigueAerobic?: number;
  fatigueAnaerobic?: number;
  fatigueAerobicForecast?: number;
  fatigueAnaerobicForecast?: number;
  monotony?: number;
  strain?: number;
  monotonyForecast?: number;
  strainForecast?: number;
}

const part = (v: number | null | undefined) => (v == null ? undefined : v);

export function chartPoints(days: FormDay[], forecast: FormForecastDay[]): ChartPoint[] {
  const out: ChartPoint[] = days.map((d) => ({
    date: d.date,
    load: d.load,
    fitness: d.fitness,
    fatigue: d.fatigue,
    form: d.form,
    fatigueAerobic: part(d.fatigue_aerobic),
    fatigueAnaerobic: part(d.fatigue_anaerobic),
    monotony: part(d.monotony),
    strain: part(d.strain),
  }));
  if (forecast.length === 0) return out;
  const last = out[out.length - 1];
  if (last)
    Object.assign(last, {
      fitnessForecast: last.fitness,
      fatigueForecast: last.fatigue,
      formForecast: last.form,
      fatigueAerobicForecast: last.fatigueAerobic,
      fatigueAnaerobicForecast: last.fatigueAnaerobic,
      monotonyForecast: last.monotony,
      strainForecast: last.strain,
    });
  for (const f of forecast)
    out.push({
      date: f.date,
      fitnessForecast: f.fitness,
      fatigueForecast: f.fatigue,
      formForecast: f.form,
      fatigueAerobicForecast: part(f.fatigue_aerobic),
      fatigueAnaerobicForecast: part(f.fatigue_anaerobic),
      monotonyForecast: part(f.monotony),
      strainForecast: part(f.strain),
    });
  return out;
}

/** Obere Grenze der Monotonie-Achse; höhere Werte werden dort abgeschnitten und markiert (#25). */
export const MONOTONY_AXIS_MAX = 4;

/** Punkte für das Diagramm „Monotonie & Strain“: Monotonie gekappt, Abschnitte über der Warnschwelle getrennt. */
export interface FosterPoint {
  date: string;
  monotony?: number;
  monotonyForecast?: number;
  /** Nur über der Warnschwelle (rote Linie), Nachbarn mit, damit die Linie anschließt */
  monotonyHigh?: number;
  /** Ungekappter Wert, wenn gekappt */
  monotonyRaw?: number;
  strain?: number;
  strainForecast?: number;
}

export function fosterPoints(points: ChartPoint[], warn: number): FosterPoint[] {
  const cap = (v: number | undefined) => (v == null ? undefined : Math.min(v, MONOTONY_AXIS_MAX));
  const value = (p: ChartPoint) => p.monotony ?? p.monotonyForecast;
  const high = points.map((p) => (value(p) ?? 0) > warn);
  return points.map((p, i) => {
    const v = value(p);
    const near = high[i] || high[i - 1] || high[i + 1];
    return {
      date: p.date,
      monotony: cap(p.monotony),
      monotonyForecast: cap(p.monotonyForecast),
      monotonyHigh: near && v != null ? cap(v) : undefined,
      monotonyRaw: v != null && v > MONOTONY_AXIS_MAX ? v : undefined,
      strain: p.strain,
      strainForecast: p.strainForecast,
    };
  });
}

/** Richtung und Betrag einer Änderung für die Kachelreihen (#25), z. B. ↓ 138. */
export function trend(delta: number): { dir: "up" | "down" | "flat"; abs: number } {
  const r = Math.round(delta * 10) / 10;
  if (Math.abs(r) < 0.05) return { dir: "flat", abs: 0 };
  return { dir: r > 0 ? "up" : "down", abs: Math.abs(r) };
}

/**
 * Feste Teilstriche der x-Achse bei langen Zeiträumen: Jahresanfänge (über drei Jahre) oder Monatserste (über 120 Tage);
 * `undefined` überlässt die Wahl dem Diagramm. Bei vielen Monaten nur jeder zweite bzw. dritte.
 */
export function axisTicks(dates: string[]): string[] | undefined {
  const span = dates.length;
  if (span <= 120) return undefined;
  if (span > 3 * 365) return dates.filter((d) => d.endsWith("-01-01"));
  const firsts = dates.filter((d) => d.endsWith("-01"));
  const step = firsts.length > 24 ? 3 : firsts.length > 12 ? 2 : 1;
  return firsts.filter((d) => (Number(d.slice(5, 7)) - 1) % step === 0);
}

/** Anteil von oben, an dem die Form-Fläche die Nulllinie kreuzt (für den Farbverlauf grün/orange). */
export function zeroOffset(points: ChartPoint[]): number {
  const values = points.flatMap((p) => [p.form, p.formForecast]).filter((v): v is number => v != null);
  if (values.length === 0) return 0.5;
  const max = Math.max(0, ...values);
  const min = Math.min(0, ...values);
  return max === min ? 0.5 : max / (max - min);
}

/** Tage der Vorschau je Modus; die Ruhe zeigt, wann man frisch ist, „wie bisher“ wohin der Umfang führt. */
export const FORECAST_DAYS = 28;
export const FORECAST_MODES: ForecastMode[] = ["rest", "steady"];

/** Differenz mit Vorzeichen, z. B. „+3“ oder „−2“. */
export function signed(v: number): string {
  const r = Math.round(v);
  if (r === 0) return "±0";
  return r > 0 ? `+${r}` : `−${Math.abs(r)}`;
}

// ---------------------------------------------------------------------------------------------------------
// Zonensätze (Kontoseite)

export const ZONE_MODELS: ZoneModel[] = ["reserve", "max", "threshold"];

export const DEFAULT_BOUNDS: Record<ZoneModel, number[]> = {
  max: [50, 60, 70, 80, 90],
  reserve: [50, 60, 70, 80, 90],
  threshold: [68, 85, 90, 95, 100],
};

export interface ZoneForm {
  valid_from: string;
  max_hr: string;
  resting_hr: string;
  threshold_hr: string;
  model: ZoneModel;
  bounds: string[];
}

export function zoneFormOf(s: HrZoneSet): ZoneForm {
  return {
    valid_from: s.valid_from,
    max_hr: String(s.max_hr),
    resting_hr: String(s.resting_hr),
    threshold_hr: s.threshold_hr == null ? "" : String(s.threshold_hr),
    model: s.model,
    bounds: s.bounds.map(String),
  };
}

/** Neuer Satz: Werte des Vorgängers oder Vorschlag aus dem Alter, Ruhepuls 60. */
export function newZoneForm(base: HrZoneSet | undefined, today: string, suggestedMax: number | null): ZoneForm {
  if (base) return { ...zoneFormOf(base), valid_from: today };
  return {
    valid_from: today,
    max_hr: suggestedMax ? String(suggestedMax) : "",
    resting_hr: "60",
    threshold_hr: "",
    model: "reserve",
    bounds: DEFAULT_BOUNDS.reserve.map(String),
  };
}

const int = (s: string): number | null => (/^\d{1,3}$/.test(s.trim()) ? Number(s.trim()) : null);

export type ZoneProblem = "date" | "max" | "rest" | "threshold" | "thresholdRequired" | "bounds";

/** Prüfung wie im Backend; `null`, wenn speicherbar. */
export function zoneProblem(f: ZoneForm): ZoneProblem | null {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(f.valid_from)) return "date";
  const max = int(f.max_hr);
  if (max == null || max < 100 || max > 240) return "max";
  const rest = int(f.resting_hr);
  if (rest == null || rest < 25 || rest > 120 || rest + 20 > max) return "rest";
  const thr = f.threshold_hr.trim() ? int(f.threshold_hr) : null;
  if (f.threshold_hr.trim() && (thr == null || thr <= rest || thr > max || thr < 60)) return "threshold";
  if (f.model === "threshold" && thr == null) return "thresholdRequired";
  const b = f.bounds.map(int);
  if (b.length !== 5 || b.some((v) => v == null || v < 30 || v > 120)) return "bounds";
  for (let i = 1; i < b.length; i++) if ((b[i] as number) <= (b[i - 1] as number)) return "bounds";
  return null;
}

export function zoneInput(f: ZoneForm) {
  return {
    valid_from: f.valid_from,
    max_hr: Number(f.max_hr),
    resting_hr: Number(f.resting_hr),
    threshold_hr: f.threshold_hr.trim() ? Number(f.threshold_hr) : null,
    model: f.model,
    bounds: f.bounds.map(Number),
  };
}

/** Untere Grenzen in Schlägen je Zone, gerechnet wie im Backend; `null` bei ungültigen Eingaben. */
export function previewBpm(f: ZoneForm): number[] | null {
  const max = int(f.max_hr);
  const rest = int(f.resting_hr);
  const thr = int(f.threshold_hr);
  const b = f.bounds.map(int);
  if (max == null || rest == null || b.some((v) => v == null)) return null;
  if (f.model === "threshold" && thr == null) return null;
  return (b as number[]).map((p) => {
    const x = p / 100;
    if (f.model === "max") return Math.round(max * x);
    if (f.model === "reserve") return Math.round(rest + (max - rest) * x);
    return Math.round((thr as number) * x);
  });
}

/** Ab dieser HF zählt die Belastung als anaerob: Schwellen-HF, sonst Beginn von Zone 4. */
export function anaerobicFrom(f: ZoneForm, lower: number[] | null): number | null {
  const thr = int(f.threshold_hr);
  return thr ?? lower?.[3] ?? null;
}
