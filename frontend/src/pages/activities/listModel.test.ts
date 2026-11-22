import { describe, expect, it } from "vitest";
import { apiQuery, rangeFrom, readFilter, selectSport, selectSportSub, toggleSort, writeFilter, yearChoices, type ListFilter } from "./listModel";

const round = (f: ListFilter) => expect(readFilter(writeFilter(f))).toEqual(f);

describe("Filter der Aktivitätenliste (RFC-0006)", () => {
  it("Vorgabe: letzte 12 Wochen, neueste zuerst", () => {
    const f = readFilter(new URLSearchParams());
    expect(f).toEqual({ period: { kind: "last12" }, q: "", sport: null, sub: null, route: null, sort: "start_time", desc: true });
    expect(writeFilter(f).toString()).toBe("");
    expect(apiQuery(f, "2026-11-09", 0)).toBe("from=2026-08-18&sort=-start_time&limit=50&offset=0");
  });

  it("Adresse hin und zurück", () => {
    const p = new URLSearchParams("zeitraum=alle&q=Wald&sport=cycling&sub=MTB&strecke=r1&sort=distance_m");
    const f = readFilter(p);
    expect(f).toEqual({ period: { kind: "all" }, q: "Wald", sport: "cycling", sub: "MTB", route: "r1", sort: "distance_m", desc: false });
    round(f);
    expect(apiQuery(f, "2026-11-09", 50)).toBe("q=Wald&sport_class=cycling&sub_sport=MTB&route_id=r1&sort=distance_m&limit=50&offset=50");
  });

  it("Saison fragt ihren ganzen Zeitraum ab", () => {
    const f = readFilter(new URLSearchParams("saison=s26"));
    expect(f.period).toEqual({ kind: "season", id: "s26" });
    round(f);
    expect(apiQuery(f, "2026-11-09", 0, 50, { start_date: "2026-03-01", end_date: "2026-10-31" })).toBe(
      "from=2026-03-01&to=2026-10-31&sort=-start_time&limit=50&offset=0",
    );
  });

  it("Jahr: 1. Januar bis 31. Dezember", () => {
    const f = readFilter(new URLSearchParams("jahr=2025"));
    expect(f.period).toEqual({ kind: "year", year: 2025 });
    expect(writeFilter(f).toString()).toBe("jahr=2025");
    expect(apiQuery(f, "2026-11-09", 0)).toBe("from=2025-01-01&to=2025-12-31&sort=-start_time&limit=50&offset=0");
    expect(readFilter(new URLSearchParams("jahr=abc")).period).toEqual({ kind: "last12" });
  });

  it("Von–Bis: offene Seiten, vertauschte Grenzen, ungültige Tage fallen weg", () => {
    const f = readFilter(new URLSearchParams("zeitraum=frei&von=2026-03-01&bis=2026-05-31"));
    expect(f.period).toEqual({ kind: "range", from: "2026-03-01", to: "2026-05-31" });
    round(f);
    expect(apiQuery(f, "2026-11-09", 0)).toBe("from=2026-03-01&to=2026-05-31&sort=-start_time&limit=50&offset=0");
    const swapped = { ...f, period: { kind: "range" as const, from: "2026-05-31", to: "2026-03-01" } };
    expect(apiQuery(swapped, "2026-11-09", 0)).toContain("from=2026-03-01&to=2026-05-31");
    const open = readFilter(new URLSearchParams("zeitraum=frei&von=2026-03-01&bis=gestern"));
    expect(open.period).toEqual({ kind: "range", from: "2026-03-01", to: null });
    expect(writeFilter(open).toString()).toBe("zeitraum=frei&von=2026-03-01");
    expect(apiQuery(open, "2026-11-09", 0)).toBe("from=2026-03-01&sort=-start_time&limit=50&offset=0");
  });

  it("Von–Bis übernimmt den gewählten Zeitraum", () => {
    expect(rangeFrom({ kind: "last12" }, "2026-11-09")).toEqual({ kind: "range", from: "2026-08-18", to: "2026-11-09" });
    expect(rangeFrom({ kind: "year", year: 2025 }, "2026-11-09")).toEqual({ kind: "range", from: "2025-01-01", to: "2025-12-31" });
    expect(rangeFrom({ kind: "season", id: "s" }, "2026-11-09", { start_date: "2026-03-01", end_date: "2026-10-31" })).toEqual({
      kind: "range",
      from: "2026-03-01",
      to: "2026-10-31",
    });
    expect(rangeFrom({ kind: "all" }, "2026-11-09")).toEqual({ kind: "range", from: null, to: null });
  });

  it("Jahre zur Auswahl: mit Daten, laufendes und gewähltes, neueste zuerst", () => {
    expect(yearChoices([2024, 2025], 2026, null)).toEqual([2026, 2025, 2024]);
    expect(yearChoices([2025, 2026], 2026, 2019)).toEqual([2026, 2025, 2019]);
  });

  it("ungültige Werte fallen auf die Vorgabe", () => {
    const f = readFilter(new URLSearchParams("sport=boot&sub=X&sort=-foo"));
    expect(f.sport).toBeNull();
    expect(f.sub).toBeNull();
    expect(f.sort).toBe("start_time");
    expect(f.desc).toBe(true);
  });

  it("Sortierung erst absteigend, dann aufsteigend", () => {
    const f = readFilter(new URLSearchParams());
    const d = toggleSort(f, "distance_m");
    expect([d.sort, d.desc]).toEqual(["distance_m", true]);
    const a = toggleSort(d, "distance_m");
    expect([a.sort, a.desc]).toEqual(["distance_m", false]);
  });

  it("Sportklasse wechseln setzt den Untertyp zurück; Menü wählt Klasse mit oder ohne Untertyp", () => {
    const f = selectSportSub(readFilter(new URLSearchParams()), "cycling", "MTB");
    expect([f.sport, f.sub]).toEqual(["cycling", "MTB"]);
    expect(selectSport(f, "cycling").sub).toBe("MTB");
    expect(selectSport(f, "running").sub).toBeNull();
    expect(selectSportSub(f, "cycling", null).sub).toBeNull();
    expect(selectSportSub(f, "running", "trail")).toMatchObject({ sport: "running", sub: "trail" });
  });

  it("leere Suche geht nicht an die API", () => {
    const f = { ...readFilter(new URLSearchParams()), q: "  " };
    expect(apiQuery(f, "2026-11-09", 0)).not.toContain("q=");
    expect(writeFilter(f).has("q")).toBe(false);
  });
});
