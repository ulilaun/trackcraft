// Seite „Plan“ (E8): Wochen, Formular einer Einheit, Vorschau der Belastung; reine Logik ohne React.
import type { HrZoneSet, LoadSettings, PlanTemplate, PlannedUnit, SportClass } from "../../api/client";
import { addDays } from "../../utils/format";
import { monday } from "../form/formModel";

/** Angezeigte Wochen: die laufende, eine davor und zwei danach (Mockup). */
export const WEEKS = 4;

/** Erster Montag der Anzeige: eine Woche vor der laufenden. */
export function defaultStart(today: string): string {
  return monday(addDays(today, -7));
}

/** Tage je Woche (Montag bis Sonntag). */
export function weekDays(start: string, weeks = WEEKS): string[][] {
  return Array.from({ length: weeks }, (_, w) => Array.from({ length: 7 }, (_, d) => addDays(start, w * 7 + d)));
}

/** ISO-Kalenderwoche. */
export function isoWeek(day: string): number {
  const t = new Date(`${day}T12:00:00Z`);
  t.setUTCDate(t.getUTCDate() + 3 - ((t.getUTCDay() + 6) % 7));
  const jan4 = new Date(Date.UTC(t.getUTCFullYear(), 0, 4));
  return 1 + Math.round(((t.getTime() - jan4.getTime()) / 864e5 - 3 + ((jan4.getUTCDay() + 6) % 7)) / 7);
}

/** Monotonie nach Foster über eine Kalenderwoche (Mittel / Standardabweichung der 7 Tage); `null` ohne Streuung. */
export function weekMonotony(loads: number[]): number | null {
  const n = 7;
  const mean = loads.reduce((a, b) => a + b, 0) / n;
  if (mean <= 0) return null;
  const sd = Math.sqrt(loads.reduce((a, v) => a + (v - mean) ** 2, 0) / n);
  return sd < 1e-9 ? null : mean / sd;
}

/** Minuten als „h:mm“. */
export function hm(minutes: number): string {
  const m = Math.round(minutes);
  return `${Math.floor(m / 60)}:${String(m % 60).padStart(2, "0")}`;
}

/** „1:30“ oder „90“ → Minuten; `null` bei ungültiger Eingabe. */
export function parseDuration(s: string): number | null {
  const t = s.trim();
  const hmMatch = /^(\d{1,2}):([0-5]\d)$/.exec(t);
  const v = hmMatch ? Number(hmMatch[1]) * 60 + Number(hmMatch[2]) : /^\d{1,4}$/.test(t) ? Number(t) : NaN;
  return Number.isFinite(v) && v >= 1 && v <= 1440 ? v : null;
}

// ---------------------------------------------------------------------------------------------------------
// Formular

export type UnitMode = "zones" | "target";

export interface UnitForm {
  date: string;
  sportClass: SportClass;
  subSport: string;
  title: string;
  note: string;
  mode: UnitMode;
  duration: string;
  targetLoad: string;
  intensivePct: string;
  zones: string[];
  status: "planned" | "skipped";
}

export function formOf(u: PlannedUnit | null, date: string): UnitForm {
  return {
    date: u?.date ?? date,
    sportClass: u?.sport_class ?? "cycling",
    subSport: u?.sub_sport ?? "",
    title: u?.title ?? "",
    note: u?.note ?? "",
    mode: u?.mode ?? "zones",
    duration: u?.duration_min != null ? hm(u.duration_min) : "1:00",
    targetLoad: u?.target_load != null ? String(u.target_load) : "60",
    intensivePct: String(u?.intensive_pct ?? 0),
    zones: (u?.zone_minutes ?? [0, 60, 0, 0, 0]).map(String),
    status: u?.status === "skipped" ? "skipped" : "planned",
  };
}

const int = (s: string, lo: number, hi: number): number | null => {
  const t = s.trim();
  if (!/^\d{1,4}$/.test(t)) return null;
  const v = Number(t);
  return v >= lo && v <= hi ? v : null;
};

export type UnitProblem = "date" | "title" | "duration" | "load" | "intensive" | "zones";

export function unitProblem(f: UnitForm): UnitProblem | null {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(f.date)) return "date";
  if (!f.title.trim()) return "title";
  if (f.mode === "target") {
    if (parseDuration(f.duration) == null) return "duration";
    if (int(f.targetLoad, 0, 1000) == null) return "load";
    if (int(f.intensivePct, 0, 100) == null) return "intensive";
  } else {
    const z = f.zones.map((v) => int(v || "0", 0, 1440));
    const sum = z.reduce<number>((a, v) => a + (v ?? 0), 0);
    if (z.some((v) => v == null) || sum < 1 || sum > 1440) return "zones";
  }
  return null;
}

/** Formular aus einer Vorlage: übernimmt alles außer Datum und Status. */
export function formOfTemplate(t: PlanTemplate, keep: Pick<UnitForm, "date" | "status">): UnitForm {
  return { ...formOf({ ...t, date: keep.date, status: "planned", activity_id: null }, keep.date), status: keep.status };
}

/** Eingabe für POST/PATCH `/plan/templates`: wie eine Einheit ohne Datum und Status, dazu der Name. */
export function templateInput(f: UnitForm, name: string) {
  const { date: _date, ...rest } = unitInput(f, false);
  void _date;
  return { name: name.trim(), ...rest };
}

/** Eingabe für POST/PATCH `/plan/units`. */
export function unitInput(f: UnitForm, withStatus: boolean) {
  const base = {
    date: f.date,
    sport_class: f.sportClass,
    sub_sport: f.subSport || null,
    title: f.title.trim(),
    note: f.note.trim() || null,
    ...(withStatus ? { status: f.status } : {}),
  };
  return f.mode === "target"
    ? { ...base, mode: "target", duration_min: parseDuration(f.duration), target_load: Number(f.targetLoad), intensive_pct: Number(f.intensivePct) }
    : { ...base, mode: "zones", zone_minutes: f.zones.map((v) => Number(v || "0")) };
}

// ---------------------------------------------------------------------------------------------------------
// Belastung wie im Backend (Vorschau im Dialog)

/** Gültiger Zonensatz am Tag: eigener der Sportklasse ab seinem Datum, sonst der allgemeine (ältester rückwirkend). */
export function applicableSet(sets: HrZoneSet[], sportClass: SportClass, day: string): HrZoneSet | null {
  const own = sets.filter((s) => s.sport_class === sportClass && s.valid_from <= day).sort((a, b) => a.valid_from.localeCompare(b.valid_from));
  if (own.length) return own[own.length - 1]!;
  const general = sets.filter((s) => s.sport_class == null).sort((a, b) => a.valid_from.localeCompare(b.valid_from));
  const valid = general.filter((s) => s.valid_from <= day);
  return valid[valid.length - 1] ?? general[0] ?? null;
}

/** HF für die Planung: Mitte der Zone, oberste Zone bis zur maximalen HF. */
export function zoneMid(set: HrZoneSet, zone: number): number {
  const lo = set.zones_bpm[zone]![0];
  const hi = zone + 1 < set.zones_bpm.length ? set.zones_bpm[zone + 1]![0] : set.max_hr;
  return (lo + hi) / 2;
}

function banister(minutes: number, hr: number, set: HrZoneSet, sexFactor: number): number {
  const span = set.max_hr - set.resting_hr;
  const x = span <= 0 ? 0 : Math.min(1, Math.max(0, (hr - set.resting_hr) / span));
  return minutes * x * 0.64 * Math.exp(sexFactor * x);
}

export interface LoadPreview {
  total: number;
  aerobic: number;
  anaerobic: number;
  minutes: number;
  method: "target" | "zones" | "estimated";
}

export function previewLoad(f: UnitForm, set: HrZoneSet | null, s: Pick<LoadSettings, "sex_factor" | "estimate_per_hour"> | undefined): LoadPreview | null {
  if (unitProblem(f) && unitProblem(f) !== "title" && unitProblem(f) !== "date") return null;
  if (f.mode === "target") {
    const total = Number(f.targetLoad);
    const ana = (total * Number(f.intensivePct)) / 100;
    return { total, aerobic: total - ana, anaerobic: ana, minutes: parseDuration(f.duration) ?? 0, method: "target" };
  }
  const mins = f.zones.map((v) => Number(v || "0"));
  const minutes = mins.reduce((a, b) => a + b, 0);
  if (!set || !s) {
    const perHour = s?.estimate_per_hour[f.sportClass] ?? 0;
    const total = (minutes / 60) * perHour;
    return { total, aerobic: total, anaerobic: 0, minutes, method: "estimated" };
  }
  let aerobic = 0;
  let anaerobic = 0;
  mins.forEach((m, i) => {
    const hr = zoneMid(set, i);
    const load = banister(m, hr, set, s.sex_factor);
    if (hr >= set.anaerobic_from_bpm) anaerobic += load;
    else aerobic += load;
  });
  return { total: aerobic + anaerobic, aerobic, anaerobic, minutes, method: "zones" };
}

// ---------------------------------------------------------------------------------------------------------
// Wiederholen (E8)

/** Höchstens so viele Einheiten hat eine Serie (mit der ersten). */
export const MAX_SERIES = 26;

/** Daten einer Serie ab `date` (mit `date`) im Abstand von `everyWeeks` Wochen bis `until`, höchstens 27 zum Prüfen. */
export function seriesDates(date: string, everyWeeks: number, until: string): string[] {
  const out: string[] = [];
  if (!/^\d{4}-\d{2}-\d{2}$/.test(date) || !/^\d{4}-\d{2}-\d{2}$/.test(until) || everyWeeks < 1) return out;
  for (let d = date; d <= until && out.length <= MAX_SERIES; d = addDays(d, everyWeeks * 7)) out.push(d);
  return out;
}

/** Problem beim Wiederholen: nur eine Einheit oder mehr als 26. */
export function repeatProblem(dates: string[]): "repeatNone" | "repeatMany" | null {
  if (dates.length < 2) return "repeatNone";
  if (dates.length > MAX_SERIES) return "repeatMany";
  return null;
}

/** Nur die geänderten Felder (für „diese und folgende“); eine geänderte Belastung schickt alle Belastungsfelder. */
export function changedInput(before: ReturnType<typeof unitInput>, after: ReturnType<typeof unitInput>): Record<string, unknown> {
  const b = before as Record<string, unknown>;
  const a = after as Record<string, unknown>;
  const loadKeys = ["mode", "duration_min", "target_load", "intensive_pct", "zone_minutes"];
  const same = (k: string) => JSON.stringify(a[k]) === JSON.stringify(b[k]);
  const out: Record<string, unknown> = {};
  for (const k of Object.keys(a)) if (!loadKeys.includes(k) && !same(k)) out[k] = a[k];
  if (loadKeys.some((k) => !same(k))) for (const k of loadKeys) if (k in a) out[k] = a[k];
  return out;
}
