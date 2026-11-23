import type { PeriodBlock, PlannedUnit } from "../../api/client";
import { classesIn, dayBars, monthTitle, totalOf, weekDays, weekRange } from "./dashboardModel";

const summary = (sport_class: PeriodBlock["summaries"][number]["sport_class"], count: number, distance_m: number) => ({
  period: "2026-W46",
  sport_class,
  count,
  distance_m,
  total_time_ms: count * 3_600_000,
  elevation_gain_m: 0,
  activities_without_distance: 0,
  activities_without_elevation: 0,
});

const week: PeriodBlock = {
  period: "2026-W46",
  from: "2026-11-09",
  to: "2026-11-15",
  summaries: [summary("cycling", 2, 60_000), summary("running", 1, 10_000), summary("unknown", 1, 0), summary("all", 4, 70_000)],
  by_day: [
    { date: "2026-11-09", sport_class: "cycling", count: 1, distance_m: 25_000, total_time_ms: 3_600_000 },
    { date: "2026-11-09", sport_class: "running", count: 1, distance_m: 10_000, total_time_ms: 3_000_000 },
    { date: "2026-11-11", sport_class: "cycling", count: 1, distance_m: 35_000, total_time_ms: 4_000_000 },
    { date: "2026-11-12", sport_class: "unknown", count: 1, distance_m: 0, total_time_ms: 1_000_000 },
  ],
};

describe("Dashboard-Modell", () => {
  it("Summe über alle Sportklassen, auch ohne Eintrag", () => {
    expect(totalOf(week)).toMatchObject({ count: 4, distance_m: 70_000 });
    expect(totalOf({ ...week, summaries: [] })).toMatchObject({ count: 0, distance_m: 0, period: "2026-W46" });
  });

  it("Woche: sieben Tage Mo–So, Distanz je Sportklasse in km, leere Tage 0", () => {
    const bars = dayBars(week, "week");
    expect(bars.map((b) => b.label)).toEqual(["Mo", "Di", "Mi", "Do", "Fr", "Sa", "So"]);
    expect(bars[0]).toMatchObject({ date: "2026-11-09", cycling: 25, running: 10, count: 2 });
    expect(bars[1]).toMatchObject({ cycling: 0, running: 0, count: 0 });
    expect(bars[3]).toMatchObject({ unknown: 0, count: 1 });
  });

  it("Monat: ein Balken je Tag, beschriftet mit dem Tag", () => {
    const bars = dayBars({ ...week, period: "2026-02", from: "2026-02-01", to: "2026-02-28" }, "month");
    expect(bars).toHaveLength(28);
    expect(bars[0]?.label).toBe("1");
    expect(bars[27]?.label).toBe("28");
  });

  it("Sportklassen im Zeitraum in fester Reihenfolge", () => {
    expect(classesIn(week)).toEqual(["cycling", "running", "unknown"]);
  });

  it("Überschriften", () => {
    expect(monthTitle("2026-11")).toBe("November 2026");
  });

  it("Woche: Ist je Sportklasse in Minuten, Plan ohne ausgelassene Einheiten", () => {
    const week: PeriodBlock = {
      period: "2026-W48",
      from: "2026-11-23",
      to: "2026-11-29",
      summaries: [],
      by_day: [
        { date: "2026-11-24", sport_class: "cycling", count: 1, distance_m: 39_400, total_time_ms: 108 * 60_000 },
        { date: "2026-11-26", sport_class: "running", count: 1, distance_m: 7_000, total_time_ms: 39 * 60_000 },
      ],
    };
    const unit = (date: string, sport_class: PlannedUnit["sport_class"], duration_min: number | null, status: PlannedUnit["status"] = "planned") =>
      ({ date, sport_class, duration_min, status, load: { minutes: 75 } }) as PlannedUnit;
    const days = weekDays(week, [unit("2026-11-24", "cycling", 105, "done"), unit("2026-11-27", "running", 50), unit("2026-11-28", "cycling", null), unit("2026-11-29", "cycling", 90, "skipped")]);
    expect(days.map((d) => [d.label, d.doneTotal, d.planned, d.plannedCount, d.sport])).toEqual([
      ["Mo 23", 0, 0, 0, null],
      ["Di 24", 108, 105, 1, "cycling"],
      ["Mi 25", 0, 0, 0, null],
      ["Do 26", 39, 0, 0, "running"],
      ["Fr 27", 0, 50, 1, "running"],
      ["Sa 28", 0, 75, 1, "cycling"],
      ["So 29", 0, 0, 0, null],
    ]);
    expect(days[1]?.done.cycling).toBe(108);
    expect([weekRange("2026-11-23", "2026-11-29"), weekRange("2026-11-30", "2026-12-06")]).toEqual(["23.–29.11.", "30.11.–06.12."]);
  });
});
