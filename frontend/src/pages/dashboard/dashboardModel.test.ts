import type { PeriodBlock } from "../../api/client";
import { classesIn, dayBars, isoWeek, monthTitle, totalOf, weekBars } from "./dashboardModel";

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

  it("Saison: ein Balken je ISO-Woche, angeschnittene Wochen nur mit Saisontagen", () => {
    expect([isoWeek("2026-01-01"), isoWeek("2025-12-29"), isoWeek("2026-12-31"), isoWeek("2027-01-03")]).toEqual([1, 1, 53, 53]);
    const season: PeriodBlock = {
      period: "Herbst",
      from: "2026-11-12",
      to: "2026-11-24",
      summaries: [],
      by_day: [
        { date: "2026-11-12", sport_class: "cycling", count: 1, distance_m: 20_000, total_time_ms: 1 },
        { date: "2026-11-15", sport_class: "cycling", count: 1, distance_m: 30_000, total_time_ms: 1 },
        { date: "2026-11-16", sport_class: "running", count: 1, distance_m: 8_000, total_time_ms: 1 },
      ],
    };
    const bars = weekBars(season);
    expect(bars.map((b) => [b.label, b.title, b.cycling, b.running, b.count])).toEqual([
      ["46", "KW 46 · 12.11.–15.11.2026", 50, 0, 2],
      ["47", "KW 47 · 16.11.–22.11.2026", 0, 8, 1],
      ["48", "KW 48 · 23.11.–24.11.2026", 0, 0, 0],
    ]);
  });
});
