import { inputProblem, parseMinutes, requestInput, weeklyHours } from "./suggestModel";

describe("Planungshilfe: Eingaben", () => {
  it("Minuten je Wochentag prüfen", () => {
    expect(parseMinutes("")).toBe(0);
    expect(parseMinutes(" 90 ")).toBe(90);
    expect(parseMinutes("600")).toBe(600);
    expect(parseMinutes("601")).toBeNull();
    expect(parseMinutes("1,5")).toBeNull();
    expect(inputProblem(["0", "60", "", "", "", "", ""])).toBeNull();
    expect(inputProblem(["0", "x", "", "", "", "", ""])).toBe("plan.suggest.problem.minutes");
    expect(inputProblem(["20", "29", "", "", "", "", ""])).toBe("plan.suggest.problem.noDay");
  });

  it("Anfrage und Wochenstunden (Tage unter 30 min zählen nicht)", () => {
    const base = { sport_class: "cycling" as const, max_gain: 5, recovery_weeks: true, intensity: "base" as const };
    expect(requestInput(base, ["", "60", "20", "", "", "120", "90"]).minutes).toEqual([0, 60, 20, 0, 0, 120, 90]);
    expect(weeklyHours(["", "60", "20", "", "", "120", "90"])).toBe(4.5);
  });
});
