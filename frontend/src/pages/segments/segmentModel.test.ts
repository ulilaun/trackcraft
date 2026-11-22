import {
  board,
  effortYears,
  localYear,
  apiQuery,
  effortKmh,
  fetchAll,
  initialForm,
  lastEffort,
  listPath,
  ownEffort,
  patchOf,
  progression,
  readFilter,
  referencePoints,
  segmentPath,
  toggleSort,
  writeFilter,
} from "./segmentModel";
import { attempts, effort, segment, track } from "./testData";

describe("Filter der Segmentliste", () => {
  it("liest und schreibt Suche, Sportklasse und Sortierung; die Art kommt von der Seite", () => {
    const f = readFilter(new URLSearchParams("q=Haus&sport=running&art=loop&sort=-length_m"), "route");
    expect(f).toEqual({ kind: "route", q: "Haus", sport: "running", category: "loop", sort: "length_m", desc: true });
    expect(writeFilter(f).toString()).toBe("q=Haus&sport=running&art=loop&sort=-length_m");
    expect([listPath("route"), listPath("segment"), segmentPath("route", "r1"), segmentPath(undefined, "s1")]).toEqual([
      "/strecken",
      "/segmente",
      "/strecken/r1",
      "/segmente/s1",
    ]);
    expect(apiQuery(f, 50)).toBe("kind=route&q=Haus&sport_class=running&category=loop&sort=-length_m&limit=50&offset=50");
  });

  it("Vorgabe: Segmente nach Name, unbekannte Werte fallen weg", () => {
    // Kategorie einer anderen Art gilt nicht
    const f = readFilter(new URLSearchParams("sport=unknown&art=loop&sort=-quatsch"), "segment");
    expect(f).toEqual({ kind: "segment", q: "", sport: null, category: null, sort: "name", desc: false });
    expect(writeFilter(f).toString()).toBe("");
    expect(apiQuery(f, 0)).toBe("kind=segment&sort=name&limit=50&offset=0");
  });

  it("Spaltenkopf: Name zuerst aufsteigend, Länge zuerst absteigend, zweiter Klick dreht", () => {
    const f = readFilter(new URLSearchParams(), "segment");
    expect(toggleSort(f, "name")).toMatchObject({ sort: "name", desc: true });
    const byLength = toggleSort(f, "length_m");
    expect(byLength).toMatchObject({ sort: "length_m", desc: true });
    expect(toggleSort(byLength, "length_m")).toMatchObject({ sort: "length_m", desc: false });
  });
});

describe("Segmentseite", () => {
  it("Bestzeiten-Entwicklung: nur gültige Versuche nach Datum, Bestzeit als Stufe, Herkunft markiert", () => {
    const p = progression(attempts(), "a2");
    expect(p.map((x) => [x.effort.id, x.d, x.best, x.own])).toEqual([
      ["e1", 300, 300, null],
      ["e2", 271.25, 271.25, 271.25],
    ]);
    expect(progression([], "a2")).toEqual([]);
  });

  it("Versuch der Herkunftsaktivität und letzter Versuch", () => {
    const all = attempts();
    expect(ownEffort(all, "a1")?.id).toBe("e1");
    expect(ownEffort(all, "a3")?.id).toBe("e3");
    expect(ownEffort(all, undefined)).toBeNull();
    expect(ownEffort([effort({ id: "x", rank: 5 }), effort({ id: "y", rank: 3 })], "a1")?.id).toBe("y");
    expect(lastEffort(all)?.id).toBe("e2");
    expect(lastEffort([])).toBeNull();
  });

  it("Schnitt aus Länge und Dauer", () => {
    expect(effortKmh(1000, 120_000)).toBeCloseTo(30);
    expect(effortKmh(1000, null)).toBeNull();
  });

  it("Referenz als Punkte für Karte und Profil", () => {
    const pts = referencePoints(track.data);
    expect(pts).toHaveLength(3);
    expect(pts[2]).toMatchObject({ lat: 50.002, lon: 8, ele: 188, distM: 1520, timeS: null });
  });

  it("lädt alle Seiten", async () => {
    const calls: string[] = [];
    const get = async <R>(path: string): Promise<R> => {
      calls.push(path);
      const offset = Number(new URLSearchParams(path.split("?")[1]).get("offset"));
      return { data: offset === 0 ? [1, 2] : [3], meta: { total: 3 } } as R;
    };
    expect(await fetchAll<number>("/x?sort=a", get)).toEqual([1, 2, 3]);
    expect(calls).toEqual(["/x?sort=a&limit=200&offset=0", "/x?sort=a&limit=200&offset=2"]);
  });
});

describe("Segment bearbeiten", () => {
  it("schickt nur geänderte Felder, nichts bei leerem Namen oder ohne Änderung", () => {
    const s = segment({ description: "alt" });
    const f = initialForm(s);
    expect(patchOf(s, f)).toBeNull();
    expect(patchOf(s, { ...f, name: "  " })).toBeNull();
    expect(patchOf(s, { ...f, name: " Neu ", description: "", kind: "route" })).toEqual({ name: "Neu", description: null, kind: "route" });
  });

  it("Ort und Kategorie: von Hand, zurück auf automatisch, beim Artwechsel nur passende", () => {
    const s = segment({ category: "climb", category_auto: "climb", location: null });
    const f = initialForm(s);
    expect(f.category).toBe("auto");
    expect(patchOf(s, { ...f, category: "rolling", location: " Aischfeld " })).toEqual({ category: "rolling", location: "Aischfeld" });
    const manual = segment({ category: "rolling", category_auto: "climb", location: "Aischfeld" });
    expect(initialForm(manual).category).toBe("rolling");
    expect(patchOf(manual, { ...initialForm(manual), category: "auto", location: "" })).toEqual({ category: null, location: null });
    // Artwechsel: alte Wahl passt nicht, der Server bestimmt neu; eine passende wird mitgeschickt
    expect(patchOf(manual, { ...initialForm(manual), kind: "route" })).toEqual({ kind: "route" });
    expect(patchOf(manual, { ...initialForm(manual), kind: "route", category: "loop" })).toEqual({ kind: "route", category: "loop" });
  });
});

describe("Bestenliste nach Zeitraum", () => {
  const now = new Date("2026-11-12T10:00:00Z");
  const e = (id: string, start: string, ms: number | null, hr: number | null, status: "valid" | "not_ranked" = "valid") =>
    effort({ id, start_time: start, duration_ms: ms, avg_heart_rate_bpm: hr, status, rank: null });
  const all = [
    e("a", "2025-06-01T10:00:00Z", 250_000, 150),
    e("b", "2026-05-01T10:00:00Z", 270_000, 160),
    e("c", "2026-07-01T10:00:00Z", 260_000, 140),
    e("d", "2026-08-01T10:00:00Z", null, null, "not_ranked"),
  ];

  it("gesamt: Rang, Rückstand und HF im Vergleich zur Bestzeit; Jahresbestzeit getrennt", () => {
    const b = board(all, { kind: "all" }, now, "Europe/Berlin");
    expect(b.rows.map((r) => [r.effort.id, r.rank, r.behindMs, r.hr])).toEqual([
      ["a", 1, 0, null],
      ["c", 2, 10_000, "down"],
      ["b", 3, 20_000, "up"],
      ["d", null, null, null],
    ]);
    expect(b.yearBestId).toBe("c");
    expect(b.year).toBe(2026);
  });

  it("dieses Jahr: nur Versuche des Jahres, Bestzeit des Jahres als Bezug", () => {
    const b = board(all, { kind: "year", year: 2026 }, now, "Europe/Berlin");
    expect(b.rows.map((r) => [r.effort.id, r.rank, r.behindMs, r.hr])).toEqual([
      ["c", 1, 0, null],
      ["b", 2, 10_000, "up"],
      ["d", null, null, null],
    ]);
    expect(b.best?.id).toBe("c");
  });

  it("früheres Jahr und Saison mit lokalem letzten Tag", () => {
    expect(board(all, { kind: "year", year: 2025 }, now, "Europe/Berlin").rows.map((r) => r.effort.id)).toEqual(["a"]);
    // 01.05. bis 01.07. einschließlich: b und c, nicht d (August)
    const b = board(all, { kind: "season", start_date: "2026-05-01", end_date: "2026-07-01" }, now, "Europe/Berlin");
    expect(b.rows.map((r) => [r.effort.id, r.rank])).toEqual([
      ["c", 1],
      ["b", 2],
    ]);
    expect(effortYears(all, now, "Europe/Berlin")).toEqual([2026, 2025]);
    expect(effortYears([], now, "Europe/Berlin")).toEqual([2026]);
  });

  it("Spalte Bestzeit: Saison vor Jahr, nichts bei der absoluten Bestzeit", () => {
    const badges = (b: ReturnType<typeof board>) => Object.fromEntries(b.rows.map((r) => [r.effort.id, r.badge]));
    // ohne Saisons: c ist Bestzeit 2026; a (absolut), b und d (nicht gewertet) ohne Vermerk
    expect(badges(board(all, { kind: "all" }, now, "Europe/Berlin"))).toEqual({ a: null, b: null, c: { kind: "year", year: 2026 }, d: null });
    // Frühling: b ist dort am schnellsten; Sommer: c, als Saison vor dem Jahr
    const seasons = [
      { name: "Frühling", start_date: "2026-03-01", end_date: "2026-05-31" },
      { name: "Sommer", start_date: "2026-06-01", end_date: "2026-08-31" },
    ];
    const expected = { a: null, b: { kind: "season", name: "Frühling" }, c: { kind: "season", name: "Sommer" }, d: null };
    expect(badges(board(all, { kind: "all" }, now, "Europe/Berlin", seasons))).toEqual(expected);
    // unabhängig vom gezeigten Zeitraum: im Jahr 2026 ist c dort die Bestzeit, aber nicht die absolute
    expect(badges(board(all, { kind: "year", year: 2026 }, now, "Europe/Berlin", seasons))).toEqual({ b: expected.b, c: expected.c, d: null });
  });

  it("Jahresgrenze in der Zeitzone der Installation", () => {
    expect(localYear("2025-12-31T23:30:00Z", "Europe/Berlin")).toBe(2026);
    expect(localYear("2025-12-31T23:30:00Z", "UTC")).toBe(2025);
  });
});
