import { describe, expect, it } from "vitest";
import type { HrZoneSet } from "../../api/client";
import {
  applicableSet,
  changedInput,
  defaultStart,
  formOf,
  isoWeek,
  parseDuration,
  previewLoad,
  repeatProblem,
  seriesDates,
  templateDates,
  unitInput,
  unitProblem,
  weekDays,
  weekMonotony,
  zoneMid,
} from "./planModel";

const set = (over: Partial<HrZoneSet> = {}): HrZoneSet => ({
  id: "z",
  sport_class: null,
  valid_from: "2020-01-01",
  max_hr: 180,
  resting_hr: 50,
  threshold_hr: null,
  model: "reserve",
  bounds: [50, 60, 70, 80, 90],
  zones_bpm: [
    [115, 127],
    [128, 140],
    [141, 153],
    [154, 166],
    [167, null],
  ],
  anaerobic_from_bpm: 154,
  ...over,
});

describe("Plan: Wochen und Hilfen", () => {
  it("Anzeige beginnt eine Woche vor der laufenden, vier Wochen Mo–So", () => {
    expect(defaultStart("2026-11-23")).toBe("2026-11-16");
    const w = weekDays("2026-11-16");
    expect(w).toHaveLength(4);
    expect(w[0]).toEqual(["2026-11-16", "2026-11-17", "2026-11-18", "2026-11-19", "2026-11-20", "2026-11-21", "2026-11-22"]);
    expect(isoWeek("2026-11-23")).toBe(48);
    expect(isoWeek("2027-01-01")).toBe(53);
  });

  it("Monotonie einer Kalenderwoche", () => {
    expect(weekMonotony([100, 0, 100, 0, 100, 0, 100])).toBeCloseTo(1.1547, 3);
    expect(weekMonotony([0, 0, 0, 0, 0, 0, 0])).toBeNull();
    expect(weekMonotony([50, 50, 50, 50, 50, 50, 50])).toBeNull();
  });

  it("Dauer als h:mm oder Minuten", () => {
    expect(parseDuration("1:30")).toBe(90);
    expect(parseDuration("45")).toBe(45);
    expect(parseDuration("0")).toBeNull();
    expect(parseDuration("1:75")).toBeNull();
  });
});

describe("Plan: Formular und Belastung wie im Backend", () => {
  it("Zonenmitte und Banister, ab der Anaerob-Grenze anaerob", () => {
    const z = set();
    expect(zoneMid(z, 1)).toBe(134.5);
    expect(zoneMid(z, 4)).toBe(173.5);
    const f = { ...formOf(null, "2026-11-24"), title: "Intervalle", zones: ["0", "60", "0", "10", "0"] };
    const p = previewLoad(f, z, { sex_factor: 1.92, estimate_per_hour: {} as never })!;
    const b = (min: number, hr: number) => {
      const x = (hr - 50) / 130;
      return min * x * 0.64 * Math.exp(1.92 * x);
    };
    expect(p.aerobic).toBeCloseTo(b(60, 134.5), 6);
    expect(p.anaerobic).toBeCloseTo(b(10, 160.5), 6);
    expect(p.minutes).toBe(70);
    // Ohne Zonensatz geschätzt
    const est = previewLoad(f, null, { sex_factor: 1.92, estimate_per_hour: { cycling: 60 } as never })!;
    expect(est).toMatchObject({ total: 70, method: "estimated", anaerobic: 0 });
  });

  it("Zielbelastung mit Anteil intensiv; Prüfung und Eingabe", () => {
    const f = { ...formOf(null, "2026-11-24"), title: " Lange Runde ", mode: "target" as const, duration: "2:30", targetLoad: "150", intensivePct: "20" };
    expect(previewLoad(f, null, undefined)).toMatchObject({ total: 150, anaerobic: 30, minutes: 150 });
    expect(unitProblem(f)).toBeNull();
    expect(unitInput(f, false)).toEqual({
      date: "2026-11-24",
      sport_class: "cycling",
      sub_sport: null,
      title: "Lange Runde",
      note: null,
      mode: "target",
      duration_min: 150,
      target_load: 150,
      intensive_pct: 20,
    });
    expect(unitProblem({ ...f, duration: "x" })).toBe("duration");
    expect(unitProblem({ ...f, mode: "zones", zones: ["0", "0", "0", "0", "0"] })).toBe("zones");
    expect(unitProblem({ ...f, title: " " })).toBe("title");
  });

  it("gültiger Zonensatz: eigener der Sportklasse, sonst allgemeiner, ältester rückwirkend", () => {
    const general = set({ id: "g", valid_from: "2025-01-01" });
    const cycling = set({ id: "c", sport_class: "cycling", valid_from: "2026-01-01" });
    expect(applicableSet([general, cycling], "cycling", "2026-05-01")?.id).toBe("c");
    expect(applicableSet([general, cycling], "cycling", "2025-05-01")?.id).toBe("g");
    expect(applicableSet([general, cycling], "running", "2020-05-01")?.id).toBe("g");
    expect(applicableSet([], "running", "2020-05-01")).toBeNull();
  });
});

describe("Serien und Vorlagen", () => {
  it("Termine einer Serie und Grenzen", () => {
    expect(seriesDates("2026-11-24", 2, "2026-12-22")).toEqual(["2026-11-24", "2026-12-08", "2026-12-22"]);
    expect(repeatProblem(seriesDates("2026-11-24", 1, "2026-11-30"))).toBe("repeatNone");
    expect(seriesDates("2026-11-24", 1, "2027-06-30")).toHaveLength(27);
    expect(repeatProblem(seriesDates("2026-11-24", 1, "2027-06-30"))).toBe("repeatMany");
    expect(repeatProblem(seriesDates("2026-11-24", 1, "2027-05-18"))).toBeNull();
  });

  it("Nur geänderte Felder, Belastung als Ganzes", () => {
    const before = unitInput(formOf(null, "2026-11-24"), false);
    expect(changedInput(before, unitInput({ ...formOf(null, "2026-11-24"), title: "Neu" }, false))).toEqual({ title: "Neu" });
    const zones = { ...formOf(null, "2026-11-24"), zones: ["10", "60", "0", "0", "0"] };
    expect(changedInput(before, unitInput(zones, false))).toEqual({ mode: "zones", zone_minutes: [10, 60, 0, 0, 0] });
  });

  it("Tage einer Vorlage ab heute", () => {
    expect(templateDates([1, 5], "2026-11-23", 2, "2026-11-25")).toEqual([
      { date: "2026-11-28", week: 0 },
      { date: "2026-12-01", week: 1 },
      { date: "2026-12-05", week: 1 },
    ]);
  });
});
