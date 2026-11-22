import { formProblem, initialForm, NEW_SUB_SPORT, patchOf, subSportOptions, toLocalInput, withSportClass } from "./editModel";
import { activity } from "./testData";

const tz = "Europe/Berlin";

describe("Bearbeiten-Modell", () => {
  it("lokale Zeit mit Sekunden in der Zeitzone der TOML", () => {
    expect(toLocalInput("2026-11-01T09:00:05.000Z", tz)).toBe("2026-11-01T10:00:05");
    expect(toLocalInput("2026-07-01T09:00:00.000Z", tz)).toBe("2026-07-01T11:00:00");
  });

  it("Ausgangsformular aus der Aktivität, unverändert kein PATCH", () => {
    const a = activity({ location: "Taunus" });
    const f = initialForm(a, tz);
    expect(f).toMatchObject({ title: "Runde Wald", description: "", location: "Taunus", subSport: "mtb", durationText: "1:00:00" });
    expect(formProblem(f)).toBeNull();
    expect(patchOf(a, f, tz)).toBeNull();
  });

  it("nur geänderte Felder, leere Freitexte werden null", () => {
    const a = activity({ location: "Taunus", description: "alt" });
    const f = { ...initialForm(a, tz), title: "  Neu ", location: " ", description: "", subSport: "Gravel", sportClass: "ebike" as const };
    expect(patchOf(a, f, tz)).toEqual({ title: "Neu", location: null, description: null, sub_sport: "Gravel", sport_class: "ebike" });
  });

  it("Zeitkorrektur nur bei echter Änderung, Sekunden ergänzt", () => {
    const a = activity({ total_time_ms: 3_600_400 });
    const same = initialForm(a, tz);
    expect(patchOf(a, same, tz)).toBeNull();
    const f = { ...same, startLocal: "2026-11-01T11:30", durationText: "1:05:00" };
    expect(patchOf(a, f, tz)).toEqual({ start_time: { local: "2026-11-01T11:30:00" }, total_time_ms: 3_900_000 });
  });

  it("doppelte Herbststunde: Auftreten nötig und wird mitgeschickt", () => {
    const a = activity();
    const f = { ...initialForm(a, tz), startLocal: "2026-10-25T02:30:00", ambiguous: true };
    expect(formProblem(f)).toBe("occurrence_missing");
    const g = { ...f, occurrence: "second" as const };
    expect(formProblem(g)).toBeNull();
    expect(patchOf(a, g, tz)).toEqual({ start_time: { local: "2026-10-25T02:30:00", occurrence: "second" } });
  });

  it("Eingabefehler sperren das Speichern", () => {
    const f = initialForm(activity(), tz);
    expect(formProblem({ ...f, title: " " })).toBe("title_empty");
    expect(formProblem({ ...f, startLocal: "" })).toBe("start_invalid");
    expect(formProblem({ ...f, durationText: "eins" })).toBe("duration_invalid");
  });

  it("Untertyp-Vorschläge je Sportklasse", () => {
    const road = { key: "road", name: null, builtin: true };
    const gravel = { key: "custom_1", name: "Gravel", builtin: false };
    const catalog = { cycling: [road, gravel], running: [] };
    expect(subSportOptions(catalog, "cycling")).toEqual([road, gravel]);
    expect(subSportOptions(catalog, "running")).toEqual([]);
    // Klassenwechsel: unpassender Schlüssel entfällt, passender und „neu“ bleiben
    const f = { ...initialForm(activity(), tz), subSport: "road" };
    expect(withSportClass(f, "running", catalog).subSport).toBe("");
    expect(withSportClass({ ...f, subSport: "custom_1" }, "cycling", catalog).subSport).toBe("custom_1");
    const n = { ...f, subSport: NEW_SUB_SPORT, newSubSport: " " };
    expect(withSportClass(n, "running", catalog).subSport).toBe(NEW_SUB_SPORT);
    expect(formProblem(n)).toBe("sub_sport_name_empty");
    expect(patchOf(activity(), { ...f, subSport: "" }, tz)).toMatchObject({ sub_sport: null });
    expect(subSportOptions(undefined, "cycling")).toEqual([]);
  });
});
