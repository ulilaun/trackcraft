// Trackdaten für Karte und Diagramm der Detailseite (RFC-0006 „Aktivitätsdetail“); reine Logik ohne React.
import type { TrackSection } from "../../api/client";

export interface TrackPoint {
  /** Sekunden seit dem ersten Messpunkt mit Zeit. */
  timeS: number | null;
  /** GPS-Distanz in Metern, über Abschnitte fortlaufend. */
  distM: number | null;
  lat: number | null;
  lon: number | null;
  ele: number | null;
  hr: number | null;
  /** Geschwindigkeit in km/h über ein kurzes Zeitfenster. */
  kmh: number | null;
}

export type Channel = "ele" | "speed" | "hr";
export type XAxisKind = "distance" | "time";

/** Halbe Fensterbreite für die Geschwindigkeit; ausgedünnte Punkte liegen meist schon weiter auseinander. */
const SPEED_HALF_WINDOW_S = 5;

/** Abschnitte aneinanderhängen; die Distanz läuft über Abschnittsgrenzen weiter. */
export function mergeSections(sections: TrackSection[]): TrackPoint[] {
  const out: TrackPoint[] = [];
  let t0: number | null = null;
  let offset = 0;
  for (const s of sections) {
    let last = 0;
    for (let i = 0; i < s.t_ms.length; i++) {
      const t = s.t_ms[i] ?? null;
      if (t != null && t0 == null) t0 = t;
      const d = s.dist_m[i] ?? null;
      if (d != null) last = d;
      out.push({
        timeS: t != null && t0 != null ? (t - t0) / 1000 : null,
        distM: d != null ? d + offset : null,
        lat: s.lat[i] ?? null,
        lon: s.lon[i] ?? null,
        ele: s.ele_m[i] ?? null,
        hr: s.hr[i] ?? null,
        kmh: null,
      });
    }
    offset += last;
  }
  addSpeed(out);
  return out;
}

/** Geschwindigkeit je Punkt aus Distanz- und Zeitdifferenz der Nachbarn im Fenster ±5 s. */
function addSpeed(pts: TrackPoint[]) {
  const idx = pts.flatMap((p, i) => (p.timeS != null && p.distM != null ? [i] : []));
  let lo = 0;
  let hi = 0;
  for (let k = 0; k < idx.length; k++) {
    const p = pts[idx[k]!]!;
    const t = p.timeS!;
    while (lo < k && pts[idx[lo]!]!.timeS! < t - SPEED_HALF_WINDOW_S) lo++;
    if (hi < k) hi = k;
    while (hi < idx.length - 1 && pts[idx[hi]!]!.timeS! < t + SPEED_HALF_WINDOW_S) hi++;
    const a = pts[idx[lo === k && k > 0 ? k - 1 : lo]!]!;
    const b = pts[idx[hi]!]!;
    const dt = b.timeS! - a.timeS!;
    p.kmh = dt > 0 ? ((b.distM! - a.distM!) / dt) * 3.6 : null;
  }
}

export function hasPosition(pts: TrackPoint[]): boolean {
  return pts.some((p) => p.lat != null && p.lon != null);
}

/** Pace in Minuten je km; unter 1 km/h (Stehen) ohne Wert, damit die Achse nicht explodiert. */
export function paceMin(kmh: number | null): number | null {
  return kmh != null && kmh >= 1 ? 60 / kmh : null;
}

export function channelValue(p: TrackPoint, c: Channel, running: boolean): number | null {
  if (c === "ele") return p.ele;
  if (c === "hr") return p.hr;
  return running ? paceMin(p.kmh) : p.kmh;
}

export function available(pts: TrackPoint[], c: Channel): boolean {
  return pts.some((p) => (c === "ele" ? p.ele : c === "hr" ? p.hr : p.kmh) != null);
}

export interface ChartDatum {
  /** Index in der Punktliste, für Karte und Hover. */
  i: number;
  /** km bzw. Sekunden */
  x: number;
  a: number | null;
  b: number | null;
}

/** Datenreihe für das Diagramm; Punkte ohne x-Wert entfallen. */
export function chartData(pts: TrackPoint[], xAxis: XAxisKind, c1: Channel, c2: Channel | null, running: boolean): ChartDatum[] {
  const out: ChartDatum[] = [];
  pts.forEach((p, i) => {
    const x = xAxis === "distance" ? (p.distM != null ? p.distM / 1000 : null) : p.timeS;
    if (x == null) return;
    out.push({ i, x, a: channelValue(p, c1, running), b: c2 ? channelValue(p, c2, running) : null });
  });
  return out;
}

/** Nächster Datenpunkt zu einem x-Wert (Daten nach x sortiert). */
export function nearest(data: ChartDatum[], x: number): ChartDatum | undefined {
  let lo = 0;
  let hi = data.length - 1;
  if (hi < 0) return undefined;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (data[mid]!.x < x) lo = mid + 1;
    else hi = mid;
  }
  const a = data[lo];
  const b = data[lo - 1];
  if (a && b && Math.abs(b.x - x) < Math.abs(a.x - x)) return b;
  return a;
}

/** Ausschnitt nach Zoom; ohne Zoom alle Daten. */
export function zoomed(data: ChartDatum[], range: [number, number] | null): ChartDatum[] {
  if (!range) return data;
  const [from, to] = range;
  return data.filter((d) => d.x >= from && d.x <= to);
}

/** Zeit des ersten Messpunkts mit Zeit (ms), Bezug für `TrackPoint.timeS`. */
export function trackStartMs(sections: TrackSection[]): number | null {
  for (const s of sections) for (const t of s.t_ms) if (t != null) return t;
  return null;
}

/** Punkte zwischen zwei Zeitpunkten (ISO), etwa ein Segmentversuch; `null`, wenn nichts darin liegt. */
export function pointRange(pts: TrackPoint[], t0Ms: number | null, startIso: string | null, endIso: string | null): [number, number] | null {
  if (t0Ms == null || !startIso || !endIso) return null;
  const a = (Date.parse(startIso) - t0Ms) / 1000;
  const b = (Date.parse(endIso) - t0Ms) / 1000;
  let first = -1;
  let last = -1;
  pts.forEach((p, i) => {
    if (p.timeS == null || p.timeS < a || p.timeS > b) return;
    if (first < 0) first = i;
    last = i;
  });
  return first >= 0 && last > first ? [first, last] : null;
}
