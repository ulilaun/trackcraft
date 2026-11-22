import { describe, expect, it } from "vitest";
import type { TrackSection } from "../../api/client";
import {
  clamp,
  createProblem,
  detailWindow,
  distanceM,
  follow,
  idxAt,
  initialRange,
  initialSport,
  makeAxis,
  moveNearer,
  passages,
  selectionPoints,
  selectionStats,
  shiftBand,
  similarSegments,
  total,
  type SelPoint,
} from "./createModel";

const STEP = 0.0003; // gut 33,4 m in Nord-Süd-Richtung

/** Gerade nach Norden mit n Punkten, 10 s Abstand, 1 m Anstieg je Punkt. */
function line(n: number, lat0 = 47): SelPoint[] {
  const out: SelPoint[] = [];
  let d = 0;
  for (let i = 0; i < n; i++) {
    const p = { lat: lat0 + i * STEP, lon: 11 };
    if (i > 0) d += distanceM(out[i - 1]!, p);
    out.push({ index: i, ...p, ele: 500 + i, t: i * 10, d });
  }
  return out;
}

describe("selectionPoints", () => {
  it("nimmt nur Punkte mit Position und behält den Abschnittsindex", () => {
    const s: TrackSection = {
      section_index: 0,
      point_count: 3,
      returned_point_count: 3,
      simplified: false,
      index: [0, 1, 2],
      t_ms: [1000, 2000, 3000],
      lat: [47, null, 47.001],
      lon: [11, null, 11],
      ele_m: [500, null, 505],
      hr: [null, null, null],
      dist_m: [0, null, 111.2],
    };
    const p = selectionPoints(s);
    expect(p.map((x) => x.index)).toEqual([0, 2]);
    expect(p[1]!.t).toBe(2);
    expect(total(p)).toBeCloseTo(111.2);
  });
});

describe("Achse", () => {
  it("ist beim Segment linear", () => {
    const a = makeAxis("segment", 10_000, 1000, 20);
    expect(a.broken).toBe(false);
    expect(a.fx(2500)).toBe(0.25);
    expect(a.fm(0.5)).toBe(5000);
  });

  it("kürzt bei der Strecke die Mitte und bildet die Ränder breit ab", () => {
    const a = makeAxis("route", 43_000, 1000, 20);
    expect(a.broken).toBe(true);
    expect(a.fx(500)).toBeCloseTo(0.23);
    expect(a.fx(1000)).toBeCloseTo(0.46);
    expect(a.fx(42_000)).toBeCloseTo(0.54);
    expect(a.fx(43_000)).toBeCloseTo(1);
    expect(a.fm(a.fx(300))).toBeCloseTo(300);
    expect(a.fm(a.fx(42_700))).toBeCloseTo(42_700);
    // Klick in die Lücke landet am näheren Rand
    expect(a.fm(0.47)).toBe(1000);
    expect(a.fm(0.53)).toBe(42_000);
  });

  it("bricht kurze Strecken nicht, der Rand bleibt unter der halben Länge", () => {
    const a = makeAxis("route", 1300, 1000, 20);
    expect(a.edge).toBe(630);
    expect(a.broken).toBe(false);
    expect(makeAxis("route", 3100, 1000, 20).broken).toBe(true);
    expect(makeAxis("route", 40, 1000, 20).broken).toBe(false);
  });
});

describe("Auswahl", () => {
  const pts = line(100); // ≈ 3,3 km

  it("startet beim Segment in der Mitte und bei der Strecke 500 m vom Rand", () => {
    const [s, e] = initialRange("segment", pts, 1000, 20);
    expect(idxAt(pts, total(pts) * 0.4)).toBe(s);
    expect(e).toBeGreaterThan(s);
    const [rs, re] = initialRange("route", pts, 1000, 20);
    expect(pts[rs]!.d).toBeCloseTo(500, -2);
    expect(total(pts) - pts[re]!.d).toBeCloseTo(500, -2);
  });

  it("hält Strecken höchstens 1 km vom Rand", () => {
    const [s, e] = clamp("route", pts, [60, 70], 1000, 20);
    expect(pts[s]!.d).toBeLessThanOrEqual(1000);
    expect(total(pts) - pts[e]!.d).toBeLessThanOrEqual(1000);
    expect(pts[s + 1]!.d).toBeGreaterThan(1000);
  });

  it("lässt Start nie hinter das Ziel", () => {
    expect(clamp("segment", pts, [50, 40], 1000, 20)).toEqual([50, 51]);
    expect(moveNearer([10, 20], 12)).toEqual([12, 20]);
    expect(moveNearer([10, 20], 19)).toEqual([10, 19]);
    expect(moveNearer([10, 20], 25)).toEqual([10, 25]);
  });

  it("verschiebt das Band bei gleicher Länge und stoppt am Ende", () => {
    const [s, e] = shiftBand(pts, pts[10]!.d, pts[20]!.d, 1e6);
    expect([s, e]).toEqual([89, 99]);
  });

  it("berechnet Länge, Höhen, Steigung und Zeit", () => {
    const st = selectionStats(pts, [10, 40]);
    expect(st.lengthM).toBeCloseTo(30 * 33.36, 0);
    expect(st.gainM).toBe(30);
    expect(st.lossM).toBe(0);
    expect(st.gradePct).toBeCloseTo(3, 1);
    expect(st.durationS).toBe(300);
  });
});

describe("Durchfahrten", () => {
  // hin und zurück: 0…20 nach Norden, dann wieder nach Süden
  const out = line(21);
  const back = out.slice(0, 20).reverse();
  const pts: SelPoint[] = [...out];
  for (const p of back) {
    const prev = pts[pts.length - 1]!;
    pts.push({ ...p, index: pts.length, d: prev.d + distanceM(prev, p), t: pts.length * 10 });
  }

  it("zählt mehrfach durchfahrene Stellen und nennt die gewählte", () => {
    expect(passages(pts, 5)).toEqual({ count: 2, own: 1 });
    expect(passages(pts, 35)).toEqual({ count: 2, own: 2 });
    // Wendepunkt: nur eine zusammenhängende Durchfahrt
    expect(passages(pts, 20).count).toBe(1);
  });

  it("folgt auf der Karte der Aufzeichnung von der bisherigen Lage aus und springt nicht auf die andere Richtung", () => {
    const q = { lat: pts[5]!.lat, lon: 11 };
    expect(follow(pts, q, 0, pts.length - 1, 4)).toBe(5);
    expect(follow(pts, q, 0, pts.length - 1, 36)).toBe(35);
    // weit nach Süden gezogen: auf der Rückfahrt bis zu deren Ende, nicht zum gleich liegenden Anfang
    expect(follow(pts, { lat: 46.9, lon: 11 }, 0, pts.length - 1, 30)).toBe(40);
    // Grenzen des Bereichs gelten
    expect(follow(pts, { lat: 46.9, lon: 11 }, 0, 33, 30)).toBe(33);
  });

  it("bleibt bei einer parallelen Rückfahrt 15 m daneben auf der eigenen Seite", () => {
    const side: SelPoint[] = [...out];
    for (const p of back) {
      const prev = side[side.length - 1]!;
      const q = { ...p, lon: 11.0002 };
      side.push({ ...q, index: side.length, d: prev.d + distanceM(prev, q), t: side.length * 10 });
    }
    // Maus genau auf der Rückfahrt neben Punkt 8 der Hinfahrt
    const q = { lat: side[32]!.lat, lon: 11.0002 };
    expect(follow(side, q, 0, side.length - 1, 6)).toBe(8);
    expect(follow(side, { lat: side[8]!.lat, lon: 11 }, 0, side.length - 1, 30)).toBe(32);
  });
});

describe("Ausschnitt und ähnliche Segmente", () => {
  const pts = line(101);
  it("Ausschnitt: Auswahl plus je ein Viertel, mindestens 50 m, innerhalb der Aufzeichnung", () => {
    const w = detailWindow(pts, [40, 60]);
    const len = pts[60]!.d - pts[40]!.d;
    expect(w.from).toBeCloseTo(pts[40]!.d - len / 4, 3);
    expect(w.to).toBeCloseTo(pts[60]!.d + len / 4, 3);
    expect(detailWindow(pts, [0, 1])).toEqual({ from: 0, to: pts[1]!.d + 50 });
  });

  it("findet Segmente mit nahem Start und Ziel und ähnlicher Länge", () => {
    const a = pts[10]!;
    const b = pts[40]!;
    const len = b.d - a.d;
    const near = { lat: a.lat + 0.0001, lon: 11 }; // gut 11 m
    const segs = [
      { id: "gleich", length_m: len * 1.05, start: near, end: b },
      { id: "zu lang", length_m: len * 1.2, start: a, end: b },
      { id: "anderes Ziel", length_m: len, start: a, end: pts[42]! },
      { id: "ohne Lage", length_m: len },
    ];
    expect(similarSegments(segs, a, b, len).map((s) => s.id)).toEqual(["gleich"]);
  });
});

describe("Formular", () => {
  it("übernimmt die Sportklasse der Aktivität, bei unbekannt Pflichtauswahl", () => {
    expect(initialSport("running")).toBe("running");
    expect(initialSport("unknown")).toBe("");
    expect(initialSport("skiing")).toBe("skiing");
    expect(initialSport("swimming")).toBe("");
    expect(initialSport("studio")).toBe("");
  });

  it("meldet den ersten fehlenden Punkt", () => {
    expect(createProblem({ name: "", description: "", location: "", sport: "" }, 10, 20)).toBe("create.problem.tooShort");
    expect(createProblem({ name: " ", description: "", location: "", sport: "" }, 30, 20)).toBe("create.problem.name");
    expect(createProblem({ name: "A", description: "", location: "", sport: "" }, 30, 20)).toBe("create.problem.sport");
    expect(createProblem({ name: "A", description: "", location: "", sport: "ebike" }, 30, 20)).toBeNull();
  });
});
