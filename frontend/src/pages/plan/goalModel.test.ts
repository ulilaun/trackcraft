import { describe, expect, it } from "vitest";
import type { PlanGoal } from "../../api/client";
import { goalFacts, goalFormOf, goalInput, goalProblem, hms, nextGoal, parseKm, parseTargetTime } from "./goalModel";

const goal: PlanGoal = {
  id: "g1",
  date: "2027-06-13",
  sport_class: "cycling",
  name: "Velothon",
  description: null,
  url: "https://example.org",
  distance_m: 42195,
  target_time_s: 3 * 3600 + 5 * 60,
};

describe("goalModel", () => {
  it("Formular aus Ziel und zurück", () => {
    const f = goalFormOf(goal, "2026-11-23");
    expect(f).toMatchObject({ distance: "42,195", targetTime: "3:05:00", url: "https://example.org", description: "" });
    expect(goalInput(f)).toEqual({
      date: "2027-06-13",
      sport_class: "cycling",
      name: "Velothon",
      description: null,
      url: "https://example.org",
      distance_m: 42195,
      target_time_s: 11100,
    });
    expect(goalFormOf(null, "2026-11-23", "running")).toMatchObject({ date: "2026-12-21", sportClass: "running", name: "" });
  });

  it("Prüfungen", () => {
    const f = goalFormOf(goal, "2026-11-23");
    expect(goalProblem(f)).toBeNull();
    expect(goalProblem({ ...f, name: " " })).toBe("name");
    expect(goalProblem({ ...f, url: "example.org" })).toBe("url");
    expect(goalProblem({ ...f, distance: "12km" })).toBe("distance");
    expect(goalProblem({ ...f, targetTime: "4:30" })).toBe("targetTime");
    expect(goalProblem({ ...f, url: "", distance: "", targetTime: "" })).toBeNull();
  });

  it("Distanz und Zielzeit", () => {
    expect([parseKm(""), parseKm("120"), parseKm("21,1"), parseKm("0"), parseKm("x")]).toEqual([null, 120000, 21100, undefined, undefined]);
    expect([parseTargetTime(""), parseTargetTime("0:45:00"), parseTargetTime("1:75:00")]).toEqual([null, 2700, undefined]);
    expect(hms(2700)).toBe("0:45:00");
    expect(goalFacts(goal)).toBe("42,2 km · 3:05:00");
    expect(goalFacts({ ...goal, distance_m: 120000, target_time_s: null })).toBe("120 km");
  });

  it("Nächstes Ziel ab morgen", () => {
    const later = { ...goal, id: "g2", date: "2027-08-01" };
    expect(nextGoal([{ ...goal, date: "2026-11-23" }, later], "2026-11-23")?.id).toBe("g2");
    expect(nextGoal([], "2026-11-23")).toBeNull();
  });
});
