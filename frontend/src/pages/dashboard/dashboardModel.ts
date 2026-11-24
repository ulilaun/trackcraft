// Dashboard nach RFC-0018: Woche, Monat und Jahr aus GET /stats/dashboard, Woche mit Plan (E8), ohne React.
import type { DashboardStatsResponse, PeriodBlock, PeriodSummary, PlannedUnit, SportClass } from "../../api/client";
import { addDays, dayRange, monthYear, weekdayShort } from "../../utils/format";
import { SPORT_CLASSES, zeroBySport } from "../../utils/sports";

/** Reihenfolge der Stapel von unten nach oben, wie die Filter-Pills der Liste. */
export const STACK_ORDER: SportClass[] = SPORT_CLASSES;

const EMPTY: PeriodSummary = {
  period: "",
  sport_class: "all",
  count: 0,
  distance_m: 0,
  total_time_ms: 0,
  elevation_gain_m: 0,
  activities_without_distance: 0,
  activities_without_elevation: 0,
};

/** Summe über alle Sportklassen einschließlich `unknown` (RFC-0018). */
export function totalOf(block: PeriodBlock): PeriodSummary {
  return block.summaries.find((s) => s.sport_class === "all") ?? { ...EMPTY, period: block.period };
}

/** Ein Balken je Kalendertag; Distanz je Sportklasse in km, 0 an Tagen ohne Aktivität (dann kein Balken). */
export interface DayBar extends Record<SportClass, number> {
  date: string;
  /** Achsenbeschriftung: Wochentag (Woche) oder Tag des Monats (Monat) */
  label: string;
  /** Aktivitäten an dem Tag (alle Sportklassen) */
  count: number;
  /** Überschrift im Tooltip statt des Datums, z. B. „KW 12 · 16.03.–22.03.2026“ bei Wochenbalken */
  title?: string;
}

export function dayBars(block: PeriodBlock, kind: "week" | "month"): DayBar[] {
  const out: DayBar[] = [];
  for (let day = block.from, i = 0; day <= block.to && i < 31; day = addDays(day, 1), i++) {
    out.push({
      date: day,
      label: kind === "week" ? weekdayShort(i) : String(Number(day.slice(8, 10))),
      ...zeroBySport(),
      count: 0,
    });
  }
  for (const b of block.by_day) {
    const bar = out.find((d) => d.date === b.date);
    if (!bar) continue;
    bar[b.sport_class] += b.distance_m / 1000;
    bar.count += b.count;
  }
  return out;
}

/** Ein Tag im Wochenstreifen: erledigte Minuten je Sportklasse und geplante Minuten aus dem Plan. */
export interface WeekDay {
  date: string;
  /** „Mo 23“ */
  label: string;
  /** Erledigte Minuten je Sportklasse */
  done: Record<SportClass, number>;
  doneTotal: number;
  /** Geplante Minuten (ohne ausgelassene Einheiten) */
  planned: number;
  plannedCount: number;
  /** Symbol unter dem Balken: erste erledigte Sportklasse, sonst die der ersten geplanten Einheit */
  sport: SportClass | null;
}

/** Geplante Dauer einer Einheit: Vorgabe, sonst aus den Zonenminuten. */
export const plannedMinutes = (u: PlannedUnit) => u.duration_min ?? u.load.minutes;

/** Sieben Tage der Woche mit Ist (aus `by_day`) und Plan; ausgelassene Einheiten zählen nicht. */
export function weekDays(block: PeriodBlock, units: PlannedUnit[]): WeekDay[] {
  const out: WeekDay[] = [];
  for (let day = block.from, i = 0; i < 7; day = addDays(day, 1), i++) {
    const done = zeroBySport();
    for (const b of block.by_day.filter((x) => x.date === day)) done[b.sport_class] += b.total_time_ms / 60_000;
    const plan = units.filter((u) => u.date === day && u.status !== "skipped");
    const first = STACK_ORDER.find((c) => done[c] > 0);
    out.push({
      date: day,
      label: `${weekdayShort(i)} ${Number(day.slice(8, 10))}`,
      done,
      doneTotal: STACK_ORDER.reduce((s, c) => s + done[c], 0),
      planned: plan.reduce((s, u) => s + plannedMinutes(u), 0),
      plannedCount: plan.length,
      sport: first ?? plan[0]?.sport_class ?? null,
    });
  }
  return out;
}

/** Zeitraum der Woche kurz: „23.–29.11.“, über den Monatswechsel „30.11.–06.12.“ (Muster de-DE). */
export function weekRange(from: string, to: string): string {
  return dayRange(from, to);
}

/** Sportklassen, die im Zeitraum vorkommen (für Stapel und Legende). */
export function classesIn(block: PeriodBlock): SportClass[] {
  return STACK_ORDER.filter((c) => block.by_day.some((b) => b.sport_class === c));
}

/** Monatsname mit Jahr aus `2026-11`. */
export function monthTitle(period: string): string {
  return monthYear(`${period}-15`);
}

export type Dashboard = DashboardStatsResponse["data"];
