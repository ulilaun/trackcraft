import type { TrackSection } from "../../api/client";
import { chartData, mergeSections, nearest, paceMin, pointRange, trackStartMs, zoomed } from "./trackModel";

/** Gerade Fahrt nach Norden: je Sekunde 10 m (36 km/h). */
function section(n: number, t0: number, index = 0, d0 = 0): TrackSection {
  const r = Array.from({ length: n }, (_, i) => i);
  return {
    section_index: index,
    point_count: n,
    returned_point_count: n,
    simplified: false,
    index: r,
    t_ms: r.map((i) => t0 + i * 1000),
    lat: r.map((i) => 50 + (i * 10) / 111_195),
    lon: r.map(() => 8),
    ele_m: r.map((i) => 100 + i),
    hr: r.map(() => 130),
    dist_m: r.map((i) => d0 + i * 10),
  };
}

describe("Trackdaten für Karte und Diagramm", () => {
  it("Zeit ab Start und Geschwindigkeit im Fenster", () => {
    const pts = mergeSections([section(30, 1_700_000_000_000)]);
    expect(pts[0]!.timeS).toBe(0);
    expect(pts[29]!.timeS).toBe(29);
    for (const p of pts) expect(p.kmh).toBeCloseTo(36, 5);
  });

  it("Distanz läuft über Abschnitte weiter", () => {
    const pts = mergeSections([section(11, 0), section(11, 20_000, 1)]);
    expect(pts[10]!.distM).toBe(100);
    expect(pts[11]!.distM).toBe(100);
    expect(pts[21]!.distM).toBe(200);
    expect(pts[11]!.timeS).toBe(20);
  });

  it("Punkte ohne Position oder Zeit", () => {
    const s = section(3, 0);
    s.lat[1] = null;
    s.lon[1] = null;
    s.dist_m[1] = null;
    s.t_ms[2] = null;
    const pts = mergeSections([s]);
    expect(pts[1]!.distM).toBeNull();
    expect(pts[2]!.timeS).toBeNull();
    expect(chartData(pts, "distance", "ele", null, false).map((d) => d.i)).toEqual([0, 2]);
    expect(chartData(pts, "time", "ele", null, false).map((d) => d.i)).toEqual([0, 1]);
  });

  it("Pace in min/km, Stehen ohne Wert", () => {
    expect(paceMin(12)).toBe(5);
    expect(paceMin(0.5)).toBeNull();
    const pts = mergeSections([section(10, 0)]);
    const d = chartData(pts, "distance", "speed", "hr", true);
    expect(d[3]!.a).toBeCloseTo(60 / 36, 5);
    expect(d[3]!.b).toBe(130);
  });

  it("nächster Punkt und Zoom", () => {
    const d = chartData(mergeSections([section(11, 0)]), "distance", "ele", null, false);
    expect(nearest(d, 0.034)?.i).toBe(3);
    expect(nearest(d, 0.036)?.i).toBe(4);
    expect(nearest(d, 5)?.i).toBe(10);
    expect(nearest([], 1)).toBeUndefined();
    expect(zoomed(d, [0.02, 0.05]).map((x) => x.i)).toEqual([2, 3, 4, 5]);
    expect(zoomed(d, null)).toBe(d);
  });
});

describe("Segmentversuch auf Karte und Diagramm", () => {
  it("findet die Punkte zwischen Start und Ende des Versuchs", () => {
    const sections = [
      {
        section_index: 0,
        point_count: 5,
        returned_point_count: 5,
        simplified: false,
        index: [0, 1, 2, 3, 4],
        t_ms: [Date.parse("2026-11-01T09:00:00Z"), null, Date.parse("2026-11-01T09:00:20Z"), Date.parse("2026-11-01T09:00:30Z"), Date.parse("2026-11-01T09:00:40Z")],
        lat: [50, 50, 50, 50, 50],
        lon: [8, 8, 8, 8, 8],
        ele_m: [1, 1, 1, 1, 1],
        hr: [null, null, null, null, null],
        dist_m: [0, 10, 20, 30, 40],
      },
    ];
    const pts = mergeSections(sections);
    const t0 = trackStartMs(sections);
    expect(pointRange(pts, t0, "2026-11-01T09:00:15Z", "2026-11-01T09:00:35Z")).toEqual([2, 3]);
    expect(pointRange(pts, t0, "2026-11-01T09:00:15Z", "2026-11-01T09:00:22Z")).toBeNull();
    expect(pointRange(pts, t0, null, "2026-11-01T09:00:22Z")).toBeNull();
  });
});
