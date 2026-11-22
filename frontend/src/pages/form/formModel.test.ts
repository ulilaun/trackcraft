import { describe, expect, it } from "vitest";
import type { FormDay } from "../../api/client";
import {
  anaerobicFrom,
  chartPoints,
  formState,
  monday,
  newZoneForm,
  previewBpm,
  signed,
  weekBars,
  zeroOffset,
  zoneProblem,
  type ZoneForm,
} from "./formModel";

const day = (date: string, load: number, over: Partial<FormDay> = {}): FormDay => ({
  date,
  load,
  aerobic: load,
  anaerobic: 0,
  estimated: 0,
  fitness: 10,
  fatigue: 12,
  form: -2,
  ...over,
});

describe("Form", () => {
  it("Zustand nach Grenzen", () => {
    expect([30, 10, 0, -20, -40].map(formState)).toEqual(["rested", "fresh", "neutral", "building", "overload"]);
    expect(formState(5)).toBe("neutral");
    expect(formState(-10)).toBe("building");
  });

  it("Wochen beginnen am Montag", () => {
    expect(monday("2026-11-22")).toBe("2026-11-16"); // Sonntag
    expect(monday("2026-11-16")).toBe("2026-11-16");
    const w = weekBars([day("2026-11-15", 10), day("2026-11-16", 20, { anaerobic: 5, aerobic: 10, estimated: 5 }), day("2026-11-22", 30)]);
    expect(w.map((b) => [b.week, b.total])).toEqual([
      ["2026-11-09", 10],
      ["2026-11-16", 50],
    ]);
    expect([w[1]!.aerobic, w[1]!.anaerobic, w[1]!.estimated]).toEqual([40, 5, 5]);
  });

  it("Vorschau schließt an heute an", () => {
    const p = chartPoints([day("2026-11-21", 0), day("2026-11-22", 0)], [{ date: "2026-11-23", load: 0, fitness: 9, fatigue: 10, form: -1 }]);
    expect(p).toHaveLength(3);
    expect(p[1]).toMatchObject({ fitness: 10, fitnessForecast: 10, formForecast: -2 });
    expect(p[2]).toEqual({ date: "2026-11-23", fitnessForecast: 9, fatigueForecast: 10, formForecast: -1 });
    expect(p[0]!.fitnessForecast).toBeUndefined();
    expect(chartPoints([day("2026-11-22", 0)], [])[0]!.fitnessForecast).toBeUndefined();
  });

  it("Nulllinie im Farbverlauf", () => {
    expect(zeroOffset([{ date: "a", form: 10 }, { date: "b", formForecast: -30 }])).toBe(0.25);
    expect(zeroOffset([{ date: "a", form: -5 }])).toBe(0);
    expect(zeroOffset([])).toBe(0.5);
  });

  it("Vorzeichen", () => {
    expect([signed(3.4), signed(-2.6), signed(0.2)]).toEqual(["+3", "−3", "±0"]);
  });
});

describe("Zonensätze", () => {
  const base: ZoneForm = { valid_from: "2026-01-01", max_hr: "180", resting_hr: "50", threshold_hr: "", model: "reserve", bounds: ["50", "60", "70", "80", "90"] };

  it("rechnet Grenzen wie das Backend", () => {
    expect(previewBpm(base)).toEqual([115, 128, 141, 154, 167]);
    expect(previewBpm({ ...base, model: "max" })).toEqual([90, 108, 126, 144, 162]);
    const thr = { ...base, model: "threshold" as const, threshold_hr: "160", bounds: ["68", "85", "90", "95", "100"] };
    expect(previewBpm(thr)).toEqual([109, 136, 144, 152, 160]);
    expect(anaerobicFrom(base, previewBpm(base))).toBe(154);
    expect(anaerobicFrom(thr, previewBpm(thr))).toBe(160);
    expect(previewBpm({ ...base, model: "threshold" })).toBeNull();
  });

  it("prüft Eingaben", () => {
    expect(zoneProblem(base)).toBeNull();
    expect(zoneProblem({ ...base, max_hr: "90" })).toBe("max");
    expect(zoneProblem({ ...base, resting_hr: "170" })).toBe("rest");
    expect(zoneProblem({ ...base, threshold_hr: "40" })).toBe("threshold");
    expect(zoneProblem({ ...base, model: "threshold" })).toBe("thresholdRequired");
    expect(zoneProblem({ ...base, bounds: ["50", "60", "60", "80", "90"] })).toBe("bounds");
    expect(zoneProblem({ ...base, valid_from: "" })).toBe("date");
  });

  it("neuer Satz übernimmt den Vorgänger oder den Vorschlag", () => {
    expect(newZoneForm(undefined, "2026-11-22", 167)).toMatchObject({ valid_from: "2026-11-22", max_hr: "167", resting_hr: "60", model: "reserve" });
    const prev = {
      id: "z",
      sport_class: null,
      valid_from: "2020-01-01",
      max_hr: 181,
      resting_hr: 48,
      threshold_hr: 160,
      model: "threshold" as const,
      bounds: [68, 85, 90, 95, 100],
      zones_bpm: [],
      anaerobic_from_bpm: 160,
    };
    expect(newZoneForm(prev, "2026-11-22", 167)).toMatchObject({ valid_from: "2026-11-22", max_hr: "181", threshold_hr: "160", model: "threshold" });
  });
});
