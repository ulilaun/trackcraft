import type { MonthBucket, RecordItem, Season, WeekBucket } from "../../api/client";
import {
  isoWeekName,
  monthBars,
  readSelection,
  recordValue,
  resolve,
  seasonRow,
  shares,
  statsQuery,
  weekBars,
  withRange,
  withSport,
  writeSelection,
} from "./statsModel";

const season = (id: string, start: string, end: string, state: Season["state"] = "ended"): Season => ({
  id,
  name: id,
  start_date: start,
  end_date: end,
  state,
  created_at: "2026-11-13T08:00:00.000Z",
  updated_at: "2026-11-13T08:00:00.000Z",
});

describe("Auswahl der Statistikseite (RFC-0006)", () => {
  it("Adresse hin und zurück, Vorgabe ist die Vorgabesaison", () => {
    expect(readSelection(new URLSearchParams())).toEqual({ period: { kind: "season", id: null }, sport: null, sub: null });
    for (const q of ["saison=s1&sport=cycling&sub=MTB", "jahr=2025", "von=2026-03-01&bis=2026-04-30&sport=running", "von=2026-03-01"]) {
      expect(writeSelection(readSelection(new URLSearchParams(q))).toString()).toBe(q);
    }
    // Untertyp nur mit Sportklasse, ungültige Werte fallen weg, Bis vor Von zählt nicht
    expect(readSelection(new URLSearchParams("sub=MTB&sport=boot")).sub).toBeNull();
    expect(readSelection(new URLSearchParams("jahr=abc")).period).toEqual({ kind: "season", id: null });
    expect(readSelection(new URLSearchParams("von=2026-05-01&bis=2026-04-01")).period.kind).toBe("season");
  });

  it("Von/Bis hebt die Kachelwahl auf, leer zurück zur Vorgabe; Sportklasse setzt den Untertyp zurück", () => {
    const s = readSelection(new URLSearchParams("jahr=2025&sport=cycling&sub=MTB"));
    expect(withRange(s, "2026-01-01", "").period).toEqual({ kind: "range", from: "2026-01-01", to: "" });
    expect(withRange(s, "", "").period).toEqual({ kind: "season", id: null });
    expect(withSport(s, "running").sub).toBeNull();
    expect(withSport(s, "cycling").sub).toBe("MTB");
  });

  it("Saison-Reihe: Vorgabe zuerst, dann neueste, ohne „alle“ höchstens vier", () => {
    const list = [
      season("a", "2021-03-01", "2021-10-31"),
      season("b", "2022-03-01", "2022-10-31"),
      season("c", "2023-03-01", "2023-10-31"),
      season("d", "2024-03-01", "2024-10-31"),
      season("e", "2025-03-01", "2025-10-31"),
      season("f", "2026-03-01", "2026-10-31"),
    ];
    expect(seasonRow(list, "d", false).map((s) => s.id)).toEqual(["d", "f", "e", "c", "b"]);
    expect(seasonRow(list, "d", true).map((s) => s.id)).toEqual(["d", "f", "e", "c", "b", "a"]);
    expect(seasonRow(list, null, false).map((s) => s.id)).toEqual(["f", "e", "d", "c"]);
  });

  it("Zeitraum: Saison, Jahr, Von/Bis; unbekannte Saison oder keine Saisons → laufendes Jahr", () => {
    const list = [season("s1", "2026-03-01", "2026-10-31")];
    expect(resolve({ kind: "season", id: null }, list, "s1", 2026)).toMatchObject({ from: "2026-03-01", to: "2026-10-31", title: "s1 · 01.03.2026 – 31.10.2026" });
    expect(resolve({ kind: "season", id: "weg" }, list, null, 2026)).toMatchObject({ from: "2026-01-01", to: "2026-12-31", season: null, title: "Jahr 2026" });
    expect(resolve({ kind: "year", year: 2024 }, list, "s1", 2026)).toMatchObject({ from: "2024-01-01", year: 2024 });
    expect(resolve({ kind: "range", from: "", to: "2026-05-01" }, list, "s1", 2026)).toMatchObject({ from: null, to: "2026-05-01", title: "… – 01.05.2026" });
    const sel = readSelection(new URLSearchParams("sport=cycling&sub=MTB"));
    expect(statsQuery({ from: "2026-01-01", to: null }, sel)).toBe("from=2026-01-01&sport_class=cycling&sub_sport=MTB");
    expect(statsQuery(null, readSelection(new URLSearchParams()))).toBe("");
  });
});

describe("Balken und Rekorde", () => {
  const m = (month: string, sport_class: MonthBucket["sport_class"], km: number, h: number): MonthBucket => ({
    month,
    sport_class,
    count: 1,
    distance_m: km * 1000,
    total_time_ms: h * 3_600_000,
    elevation_gain_m: 0,
  });

  it("Monate lückenlos im Zeitraum, je Sportklasse; Ring mit Anteilen", () => {
    const bars = monthBars([m("2026-03", "cycling", 100, 4), m("2026-03", "running", 20, 2), m("2026-05", "cycling", 60, 2)], "distance", "2026-03-01", "2026-06-30");
    expect(bars.map((b) => [b.label, b.cycling, b.running])).toEqual([
      ["Mär", 100, 20],
      ["Apr", 0, 0],
      ["Mai", 60, 0],
      ["Jun", 0, 0],
    ]);
    expect(bars[0]!.title).toBe("März 2026");
    expect(shares(bars).map((s) => [s.sport, s.share])).toEqual([
      ["cycling", 160 / 180],
      ["running", 20 / 180],
    ]);
    // Über Jahresgrenzen mit Jahr in der Beschriftung, ohne Zeitraum erster bis letzter Monat
    const x = monthBars([m("2025-12", "cycling", 1, 1), m("2026-01", "cycling", 1, 1)], "time");
    expect(x.map((b) => b.label)).toEqual(["Dez 25", "Jan 26"]);
    expect(monthBars([], "time")).toEqual([]);
  });

  it("Wochen lückenlos mit ISO-Nummer, Zeit in Stunden", () => {
    const w: WeekBucket[] = [
      { week: "2026-W01", week_start: "2025-12-29", sport_class: "cycling", count: 1, distance_m: 0, total_time_ms: 5_400_000 },
      { week: "2026-W03", week_start: "2026-01-12", sport_class: "running", count: 1, distance_m: 0, total_time_ms: 1_800_000 },
    ];
    const bars = weekBars(w);
    expect(bars.map((b) => [b.label, b.cycling, b.running])).toEqual([
      ["1", 1.5, 0],
      ["2", 0, 0],
      ["3", 0, 0.5],
    ]);
    expect(bars[1]!.title).toBe("KW 2 · 05.01.2026 – 11.01.2026");
    expect(isoWeekName("2026-12-28")).toBe("2026-W53");
  });

  it("Rekordwerte wie angezeigt, Laufen als Pace", () => {
    const r = (kind: RecordItem["kind"], value: number, sport_class: RecordItem["sport_class"] = "cycling"): RecordItem => ({
      sport_class,
      kind,
      value,
      equaled_count: 0,
      activity: { id: "a", title: "A", start_time: "2026-07-01T08:00:00.000Z", sub_sport: null },
    });
    expect(recordValue(r("distance", 123_456))).toEqual({ value: "123,46", unit: "km" });
    expect(recordValue(r("elevation", 1234.6))).toEqual({ value: "1.235", unit: "m" });
    expect(recordValue(r("time", 18_061_400))).toEqual({ value: "5:01:01", unit: "h" });
    expect(recordValue(r("speed", 27.46))).toEqual({ value: "27,5", unit: "km/h" });
    expect(recordValue(r("speed", 12, "running"))).toEqual({ value: "5:00", unit: "/km" });
  });
});
