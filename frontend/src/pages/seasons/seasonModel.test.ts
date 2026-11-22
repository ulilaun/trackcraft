import { describe, expect, it } from "vitest";
import { ApiError, type Season } from "../../api/client";
import { conflictText, formProblem, inSeason, inputOf, localDate, seasonRange } from "./seasonModel";

const season = (p: Partial<Season> = {}): Season => ({
  id: "s1",
  name: "Saison 2026",
  start_date: "2026-03-01",
  end_date: "2026-10-31",
  state: "ended",
  created_at: "2026-11-13T08:00:00.000Z",
  updated_at: "2026-11-13T08:00:00.000Z",
  ...p,
});

describe("Saisons", () => {
  it("lokaler Tag in der Zeitzone der Installation, letzter Tag zählt mit", () => {
    expect(localDate("2026-10-31T22:30:00Z", "Europe/Berlin")).toBe("2026-10-31");
    expect(localDate("2026-10-31T23:30:00Z", "Europe/Berlin")).toBe("2026-11-01");
    expect(inSeason("2026-10-31", season())).toBe(true);
    expect(inSeason("2026-11-01", season())).toBe(false);
    expect(inSeason(null, season())).toBe(false);
  });

  it("Zeitraum zur Anzeige", () => {
    expect(seasonRange(season())).toBe("01.03.2026 – 31.10.2026");
  });

  it("Formular: Prüfung und nur geänderte Felder", () => {
    expect(formProblem({ name: " ", start_date: "2026-01-01", end_date: "2026-02-01" })).toMatch(/Namen/);
    expect(formProblem({ name: "x", start_date: "2026-03-01", end_date: "2026-02-01" })).toMatch(/vor dem ersten/);
    expect(formProblem({ name: "x", start_date: "2026-03-01", end_date: "" })).toMatch(/Tag/);
    const s = season();
    expect(inputOf({ name: " Saison 2026 ", start_date: "2026-03-01", end_date: "2026-10-31" }, s)).toBeNull();
    expect(inputOf({ name: "Sommer", start_date: "2026-03-01", end_date: "2026-11-30" }, s)).toEqual({ name: "Sommer", end_date: "2026-11-30" });
    expect(inputOf({ name: " Neu ", start_date: "2027-01-01", end_date: "2027-02-01" })).toEqual({
      name: "Neu",
      start_date: "2027-01-01",
      end_date: "2027-02-01",
    });
  });

  it("Konflikt nennt die andere Saison", () => {
    const e = new ApiError(409, "season_overlap", "x", [], { season: season({ name: "Winter", start_date: "2026-11-01", end_date: "2027-02-28" }) });
    expect(conflictText(e)).toBe("Überschneidet sich mit „Winter“ (01.11.2026 – 28.02.2027).");
    expect(conflictText(new ApiError(409, "season_name_taken", "x"))).toMatch(/Namen/);
    expect(conflictText(new ApiError(400, "validation_failed", "x"))).toBeNull();
  });
});
