// Dashboard nach RFC-0018: Woche und Monat aus GET /stats/dashboard, ohne React.
import type { DashboardStatsResponse, PeriodBlock, PeriodSummary, SportClass } from "../../api/client";
import { t } from "../../i18n";
import { addDays } from "../../utils/format";
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

const WEEKDAYS = ["Mo", "Di", "Mi", "Do", "Fr", "Sa", "So"];

export function dayBars(block: PeriodBlock, kind: "week" | "month"): DayBar[] {
  const out: DayBar[] = [];
  for (let day = block.from, i = 0; day <= block.to && i < 31; day = addDays(day, 1), i++) {
    out.push({
      date: day,
      label: kind === "week" ? (WEEKDAYS[i] ?? "") : String(Number(day.slice(8, 10))),
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

/** Montag der ISO-Woche eines Tages. */
function mondayOf(day: string): string {
  const wd = new Date(`${day}T12:00:00Z`).getUTCDay();
  return addDays(day, -((wd + 6) % 7));
}

/** ISO-Kalenderwoche eines Tages (Donnerstag der Woche bestimmt das Jahr). */
export function isoWeek(day: string): number {
  const thursday = new Date(`${addDays(mondayOf(day), 3)}T12:00:00Z`);
  const jan1 = Date.UTC(thursday.getUTCFullYear(), 0, 1, 12);
  return Math.floor((thursday.getTime() - jan1) / 86_400_000 / 7) + 1;
}

const short = (day: string) => `${day.slice(8, 10)}.${day.slice(5, 7)}.`;

/**
 * Ein Balken je ISO-Woche der Saison (RFC-0018): erste und letzte Woche nur mit den Tagen in der Saison,
 * Wochen ohne Aktivität ohne Balken.
 */
export function weekBars(block: PeriodBlock): DayBar[] {
  const out: DayBar[] = [];
  for (let monday = mondayOf(block.from); monday <= block.to && out.length < 60; monday = addDays(monday, 7)) {
    const from = monday < block.from ? block.from : monday;
    const sunday = addDays(monday, 6);
    const to = sunday > block.to ? block.to : sunday;
    const week = isoWeek(monday);
    out.push({
      date: monday,
      label: String(week),
      title: t("dashboard.weekTitle", { n: week, from: short(from), to: `${short(to)}${to.slice(0, 4)}` }),
      ...zeroBySport(),
      count: 0,
    });
  }
  for (const b of block.by_day) {
    const bar = out.find((d) => d.date === mondayOf(b.date));
    if (!bar) continue;
    bar[b.sport_class] += b.distance_m / 1000;
    bar.count += b.count;
  }
  return out;
}

/** Sportklassen, die im Zeitraum vorkommen (für Stapel und Legende). */
export function classesIn(block: PeriodBlock): SportClass[] {
  return STACK_ORDER.filter((c) => block.by_day.some((b) => b.sport_class === c));
}

/** Monatsname mit Jahr aus `2026-11`. */
export function monthTitle(period: string): string {
  const d = new Date(`${period}-15T12:00:00Z`);
  return new Intl.DateTimeFormat("de-DE", { month: "long", year: "numeric", timeZone: "UTC" }).format(d);
}

export type Dashboard = DashboardStatsResponse["data"];
