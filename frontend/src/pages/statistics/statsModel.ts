// Statistikseite nach RFC-0006 „Statistik“: Auswahl, Kachelreihen und Balken, reine Logik ohne React.
import type { MonthBucket, RecordItem, Season, SportClass, StatisticsSummary, WeekBucket } from "../../api/client";
import { t } from "../../i18n";
import { addDays, num, pace } from "../../utils/format";
import { seasonRange } from "../seasons/seasonModel";
import { SPORT_CLASSES, zeroBySport } from "../../utils/sports";

/** Gewählter Zeitraum: eine Saison (`id` null = Vorgabesaison), ein Kalenderjahr oder Von/Bis. */
export type Period = { kind: "season"; id: string | null } | { kind: "year"; year: number } | { kind: "range"; from: string; to: string };

export interface Selection {
  period: Period;
  sport: SportClass | null;
  sub: string | null;
}

const SPORTS: SportClass[] = SPORT_CLASSES;
const day = /^\d{4}-\d{2}-\d{2}$/;

/** Auswahl aus der Adresse: `?saison=`, `?jahr=`, `?von=&bis=`, `?sport=`, `?sub=`; ohne Angabe die Vorgabesaison. */
export function readSelection(p: URLSearchParams): Selection {
  const raw = p.get("sport");
  const sport = SPORTS.includes(raw as SportClass) ? (raw as SportClass) : null;
  const sub = sport ? p.get("sub") || null : null;
  const from = p.get("von") ?? "";
  const to = p.get("bis") ?? "";
  const year = Number(p.get("jahr"));
  let period: Period = { kind: "season", id: p.get("saison") || null };
  if ((day.test(from) || day.test(to)) && !(day.test(from) && day.test(to) && to < from)) {
    period = { kind: "range", from: day.test(from) ? from : "", to: day.test(to) ? to : "" };
  } else if (Number.isInteger(year) && year >= 1970 && year <= 9999) period = { kind: "year", year };
  return { period, sport, sub };
}

export function writeSelection(s: Selection): URLSearchParams {
  const p = new URLSearchParams();
  if (s.period.kind === "season" && s.period.id) p.set("saison", s.period.id);
  if (s.period.kind === "year") p.set("jahr", String(s.period.year));
  if (s.period.kind === "range") {
    if (s.period.from) p.set("von", s.period.from);
    if (s.period.to) p.set("bis", s.period.to);
  }
  if (s.sport) p.set("sport", s.sport);
  if (s.sport && s.sub) p.set("sub", s.sub);
  return p;
}

/** Von/Bis setzen; beide leer heißt zurück zur Vorgabesaison. */
export function withRange(s: Selection, from: string, to: string): Selection {
  return { ...s, period: from || to ? { kind: "range", from, to } : { kind: "season", id: null } };
}

/** Sportklasse wählen setzt den Untertyp zurück; `null` = alle. */
export function withSport(s: Selection, sport: SportClass | null): Selection {
  return { ...s, sport, sub: sport === s.sport ? s.sub : null };
}

/** Saison-Reihe: Vorgabesaison zuerst, dann die übrigen neueste zuerst, ohne `all` höchstens vier davon. */
export function seasonRow(seasons: Season[], defaultId: string | null, all: boolean, max = 4): Season[] {
  const first = seasons.find((s) => s.id === defaultId) ?? null;
  const rest = seasons.filter((s) => s !== first).sort((a, b) => b.start_date.localeCompare(a.start_date));
  return [...(first ? [first] : []), ...(all ? rest : rest.slice(0, max))];
}

export interface Resolved {
  from: string | null;
  to: string | null;
  /** Gewählte Saison, auch wenn sie als Vorgabe gewählt ist */
  season: Season | null;
  /** Jahr der Jahres-Kachel (gewählt oder laufend) */
  year: number;
  title: string;
}

/** Zeitraum der Auswahl; ohne Saisons oder bei unbekannter Saison das laufende Jahr. */
export function resolve(period: Period, seasons: Season[], defaultId: string | null, currentYear: number): Resolved {
  if (period.kind === "range") {
    const title = `${period.from ? germanDay(period.from) : "…"} – ${period.to ? germanDay(period.to) : "…"}`;
    return { from: period.from || null, to: period.to || null, season: null, year: currentYear, title };
  }
  if (period.kind === "season") {
    const s = seasons.find((x) => x.id === (period.id ?? defaultId)) ?? null;
    if (s) return { from: s.start_date, to: s.end_date, season: s, year: currentYear, title: `${s.name} · ${seasonRange(s)}` };
  }
  const year = period.kind === "year" ? period.year : currentYear;
  return { from: `${year}-01-01`, to: `${year}-12-31`, season: null, year, title: t("stats.yearTitle", { year }) };
}

function germanDay(d: string): string {
  const [y, m, dd] = d.split("-");
  return `${dd}.${m}.${y}`;
}

/** Query für `GET /stats` bzw. `GET /stats/periods`. */
export function statsQuery(r: { from: string | null; to: string | null } | null, sel: Selection): string {
  const p = new URLSearchParams();
  if (r?.from) p.set("from", r.from);
  if (r?.to) p.set("to", r.to);
  if (sel.sport) p.set("sport_class", sel.sport);
  if (sel.sport && sel.sub) p.set("sub_sport", sel.sub);
  return p.toString();
}

export function totalOf(summaries: StatisticsSummary[]): StatisticsSummary {
  return (
    summaries.find((s) => s.sport_class === "all") ?? {
      sport_class: "all",
      count: 0,
      distance_m: 0,
      total_time_ms: 0,
      elevation_gain_m: 0,
      activities_without_distance: 0,
      activities_without_elevation: 0,
      segment_success_count: 0,
    }
  );
}

/** Ein Balken je Monat oder Woche, Werte je Sportklasse (km bzw. Stunden). */
export interface StackBar extends Record<SportClass, number> {
  key: string;
  label: string;
  title: string;
}

const MONTHS = ["Jan", "Feb", "Mär", "Apr", "Mai", "Jun", "Jul", "Aug", "Sep", "Okt", "Nov", "Dez"];
const LONG_MONTHS = ["Januar", "Februar", "März", "April", "Mai", "Juni", "Juli", "August", "September", "Oktober", "November", "Dezember"];

function nextMonth(m: string): string {
  const [y, mm] = m.split("-").map(Number) as [number, number];
  return mm === 12 ? `${y + 1}-01` : `${y}-${String(mm + 1).padStart(2, "0")}`;
}

const empty = (key: string, label: string, title: string): StackBar => ({ key, label, title, ...zeroBySport() });

/**
 * Monatsbalken von `from` bis `to` (sonst erster bis letzter Monat mit Aktivität), lückenlos;
 * `value` wählt Distanz (km) oder Zeit (h). Höchstens 120 Monate.
 */
export function monthBars(buckets: MonthBucket[], value: "distance" | "time", from?: string | null, to?: string | null): StackBar[] {
  const months = buckets.map((b) => b.month).sort();
  const first = from?.slice(0, 7) ?? months[0];
  const last = to?.slice(0, 7) ?? months[months.length - 1];
  if (!first || !last) return [];
  const out: StackBar[] = [];
  for (let m = first; m <= last && out.length < 120; m = nextMonth(m)) {
    const idx = Number(m.slice(5)) - 1;
    const sameYear = first.slice(0, 4) === last.slice(0, 4);
    out.push(empty(m, sameYear ? (MONTHS[idx] ?? m) : `${MONTHS[idx] ?? ""} ${m.slice(2, 4)}`, `${LONG_MONTHS[idx] ?? ""} ${m.slice(0, 4)}`));
  }
  for (const b of buckets) {
    const bar = out.find((x) => x.key === b.month);
    if (bar) bar[b.sport_class] += value === "distance" ? b.distance_m / 1000 : b.total_time_ms / 3_600_000;
  }
  return out;
}

function mondayOf(d: string): string {
  const wd = new Date(`${d}T12:00:00Z`).getUTCDay();
  return addDays(d, -((wd + 6) % 7));
}

/** Zeit je ISO-Woche in Stunden, lückenlos von der ersten bis zur letzten Woche (höchstens 160 Wochen). */
export function weekBars(buckets: WeekBucket[], from?: string | null, to?: string | null): StackBar[] {
  const starts = buckets.map((b) => b.week_start).sort();
  const first = from ? mondayOf(from) : starts[0];
  const last = to ? mondayOf(to) : starts[starts.length - 1];
  if (!first || !last) return [];
  const byStart = new Map(buckets.map((b) => [b.week_start, b.week] as const));
  const out: StackBar[] = [];
  for (let m = first; m <= last && out.length < 160; m = addDays(m, 7)) {
    const week = byStart.get(m) ?? isoWeekName(m);
    const n = week.split("-W")[1] ?? "";
    out.push(empty(m, String(Number(n)), t("stats.weekTitle", { n: Number(n), from: germanDay(m), to: germanDay(addDays(m, 6)) })));
  }
  for (const b of buckets) {
    const bar = out.find((x) => x.key === b.week_start);
    if (bar) bar[b.sport_class] += b.total_time_ms / 3_600_000;
  }
  return out;
}

/** ISO-Woche `JJJJ-Www` eines Montags. */
export function isoWeekName(monday: string): string {
  const thursday = new Date(`${addDays(monday, 3)}T12:00:00Z`);
  const y = thursday.getUTCFullYear();
  const n = Math.floor((thursday.getTime() - Date.UTC(y, 0, 1, 12)) / 86_400_000 / 7) + 1;
  return `${y}-W${String(n).padStart(2, "0")}`;
}

/** Sportklassen, die in den Balken vorkommen, in Stapelreihenfolge. */
export function classesIn(bars: StackBar[]): SportClass[] {
  return SPORTS.filter((c) => bars.some((b) => b[c] > 0));
}

/** Anteile je Sportklasse für den Ring. */
export function shares(bars: StackBar[]): { sport: SportClass; value: number; share: number }[] {
  const totals = SPORTS.map((c) => ({ sport: c, value: bars.reduce((s, b) => s + b[c], 0) }));
  const sum = totals.reduce((s, x) => s + x.value, 0);
  return totals.filter((x) => x.value > 0).map((x) => ({ ...x, share: sum > 0 ? x.value / sum : 0 }));
}

/** Rekordwert zur Anzeige: Distanz km, Aufstieg m, Zeit h:mm:ss, Schnitt km/h bzw. beim Laufen Pace. */
export function recordValue(r: RecordItem): { value: string; unit: string } {
  switch (r.kind) {
    case "distance":
      return { value: num(Math.round(r.value / 10) / 100, 2), unit: "km" };
    case "elevation":
      return { value: num(Math.round(r.value)), unit: "m" };
    case "time": {
      const s = Math.round(r.value / 1000);
      return { value: `${Math.floor(s / 3600)}:${String(Math.floor((s % 3600) / 60)).padStart(2, "0")}:${String(s % 60).padStart(2, "0")}`, unit: "h" };
    }
    case "speed":
      return r.sport_class === "running" ? { value: pace(r.value), unit: "/km" } : { value: num(r.value, 1), unit: "km/h" };
  }
}
