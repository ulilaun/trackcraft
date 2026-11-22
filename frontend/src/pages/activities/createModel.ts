// Auswahl von Start und Ziel beim Erstellen eines Segments oder einer Strecke (RFC-0016); reine Logik ohne React.
import type { RankedSportClass, SegmentKind, SportClass, TrackSection } from "../../api/client";
import type { TextKey } from "../../i18n";
import { RANKED_CLASSES } from "../../utils/sports";

/** Punkt mit Position im gewählten Abschnitt. */
export interface SelPoint {
  /** Position im Abschnitt (start_index/end_index der API). */
  index: number;
  lat: number;
  lon: number;
  ele: number | null;
  /** Sekunden seit dem ersten Punkt mit Zeit. */
  t: number | null;
  /** Distanz entlang der Punkte mit Position, wie das Backend die Länge misst. */
  d: number;
}

/** Strecke: Regler starten so weit vom Rand (Uli, 09.11.2026). */
export const ROUTE_DEFAULT_M = 500;
/** Segment: anfangs ein Ausschnitt in der Mitte, damit beide Regler greifbar sind. */
const SEGMENT_DEFAULT: [number, number] = [0.4, 0.48];
/** Hysterese für Auf- und Abstieg wie bei Aktivitäten (RFC-0009). */
const HYSTERESIS_M = 3;
/** Nähe, ab der eine andere Durchfahrt als „hier vorbei“ zählt (Matching-Radius). */
export const PASS_RADIUS_M = 15;
/** Gebrochene Achse der Strecke: linker Teil bis GAP_A, Lücke GAP_W, rechter Teil danach. */
export const GAP_A = 0.46;
export const GAP_W = 0.08;

const R = 6371008.8;
const RAD = Math.PI / 180;

export function distanceM(a: { lat: number; lon: number }, b: { lat: number; lon: number }): number {
  const p1 = a.lat * RAD;
  const p2 = b.lat * RAD;
  const dl = (b.lon - a.lon) * RAD;
  const x = Math.sin((p2 - p1) / 2) ** 2 + Math.cos(p1) * Math.cos(p2) * Math.sin(dl / 2) ** 2;
  return 2 * R * Math.asin(Math.sqrt(x));
}

/** Punkte mit Position aus einem vollständig geladenen Abschnitt. */
export function selectionPoints(s: TrackSection): SelPoint[] {
  const out: SelPoint[] = [];
  let t0: number | null = null;
  for (let i = 0; i < s.lat.length; i++) {
    const t = s.t_ms[i] ?? null;
    if (t != null && t0 == null) t0 = t;
    const lat = s.lat[i];
    const lon = s.lon[i];
    const d = s.dist_m[i];
    if (lat == null || lon == null || d == null) continue;
    out.push({ index: s.index[i] ?? i, lat, lon, ele: s.ele_m[i] ?? null, t: t != null && t0 != null ? (t - t0) / 1000 : null, d });
  }
  return out;
}

export function total(pts: SelPoint[]): number {
  return pts.length ? pts[pts.length - 1]!.d - pts[0]!.d : 0;
}

/** Abstand vom ersten Punkt. */
function at(pts: SelPoint[], i: number): number {
  return pts[i]!.d - pts[0]!.d;
}

/** Nächster Punkt zur Distanz m (ab erstem Punkt). */
export function idxAt(pts: SelPoint[], m: number): number {
  if (pts.length === 0) return 0;
  let lo = 0;
  let hi = pts.length - 1;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (at(pts, mid) < m) lo = mid + 1;
    else hi = mid;
  }
  return lo > 0 && m - at(pts, lo - 1) < at(pts, lo) - m ? lo - 1 : lo;
}

/** Rand der Strecke: höchstens route_edge_m, bei kurzen Aufzeichnungen weniger, damit die Mindestlänge bleibt. */
export function routeEdge(totalM: number, edgeM: number, minLen: number): number {
  return Math.max(0, Math.min(edgeM, totalM / 2 - minLen));
}

export interface Axis {
  /** Mitte gekürzt (nur Strecke). */
  broken: boolean;
  edge: number;
  totalM: number;
  /** Distanz → Anteil der Breite (0…1). */
  fx(m: number): number;
  /** Anteil der Breite → Distanz; in der Lücke zum näheren Rand. */
  fm(f: number): number;
}

/** Achse für Profil und Schiene: Segment linear, Strecke mit gekürzter Mitte (Uli, 09.11.2026). */
export function makeAxis(kind: SegmentKind, totalM: number, edgeM: number, minLen: number): Axis {
  const edge = routeEdge(totalM, edgeM, minLen);
  // Kürzen lohnt nur, wenn die Mitte länger ist als ein Rand; sonst bleibt die Achse linear.
  const broken = kind === "route" && edge > 0 && totalM - 2 * edge > edge;
  const T = Math.max(totalM, 1e-9);
  if (!broken) return { broken, edge, totalM, fx: (m) => m / T, fm: (f) => f * T };
  const right = 1 - GAP_A - GAP_W;
  return {
    broken,
    edge,
    totalM,
    fx: (m) => {
      if (m <= edge) return (m / edge) * GAP_A;
      if (m >= T - edge) return GAP_A + GAP_W + ((m - (T - edge)) / edge) * right;
      return GAP_A + ((m - edge) / (T - 2 * edge)) * GAP_W;
    },
    fm: (f) => {
      if (f <= GAP_A) return (f / GAP_A) * edge;
      if (f >= GAP_A + GAP_W) return T - edge + ((f - GAP_A - GAP_W) / right) * edge;
      return f < GAP_A + GAP_W / 2 ? edge : T - edge;
    },
  };
}

export type Range = [number, number];

/** Startauswahl: Segment Mitte, Strecke je 500 m vom Rand. */
export function initialRange(kind: SegmentKind, pts: SelPoint[], edgeM: number, minLen: number): Range {
  const T = total(pts);
  if (kind === "route") {
    const v = Math.min(ROUTE_DEFAULT_M, routeEdge(T, edgeM, minLen));
    return clamp(kind, pts, [idxAt(pts, v), idxAt(pts, T - v)], edgeM, minLen);
  }
  const s = idxAt(pts, T * SEGMENT_DEFAULT[0]);
  return clamp(kind, pts, [s, Math.max(idxAt(pts, T * SEGMENT_DEFAULT[1]), s + 1)], edgeM, minLen);
}

/** Start vor Ziel, beide im Abschnitt; Strecke zusätzlich höchstens edge vom Rand. */
export function clamp(kind: SegmentKind, pts: SelPoint[], [s, e]: Range, edgeM: number, minLen: number): Range {
  const last = pts.length - 1;
  s = Math.max(0, Math.min(s, last - 1));
  e = Math.min(last, Math.max(e, s + 1));
  if (kind === "route") {
    const T = total(pts);
    const edge = routeEdge(T, edgeM, minLen);
    while (s > 0 && at(pts, s) > edge) s--;
    while (e < last && T - at(pts, e) > edge) e++;
  }
  return [s, e];
}

/** Klick in Profil oder Schiene: der nähere Regler springt hin. */
export function moveNearer([s, e]: Range, i: number): Range {
  return Math.abs(i - s) <= Math.abs(i - e) ? [Math.min(i, e - 1), e] : [s, Math.max(i, s + 1)];
}

/** Band ziehen: Start und Ziel gemeinsam bei gleicher Länge (nur Segment). */
export function shiftBand(pts: SelPoint[], startM: number, endM: number, deltaM: number): Range {
  const T = total(pts);
  const len = endM - startM;
  const ns = Math.min(Math.max(0, startM + deltaM), T - len);
  const s = idxAt(pts, ns);
  return [s, Math.max(idxAt(pts, ns + len), s + 1)];
}

/** Durchfahrten in der Nähe von Punkt i und welche davon gewählt ist (1-basiert). */
export function passages(pts: SelPoint[], i: number): { count: number; own: number } {
  const here = pts[i];
  if (!here) return { count: 0, own: 0 };
  let count = 0;
  let own = 0;
  let inRun = false;
  for (let j = 0; j < pts.length; j++) {
    const near = distanceM(here, pts[j]!) <= PASS_RADIUS_M;
    if (near && !inRun) {
      count++;
      if (j <= i) own = count;
    }
    inRun = near;
  }
  return { count, own };
}

export interface SelectionStats {
  lengthM: number;
  gainM: number | null;
  lossM: number | null;
  gradePct: number | null;
  durationS: number | null;
}

export function selectionStats(pts: SelPoint[], [s, e]: Range): SelectionStats {
  let gain = 0;
  let loss = 0;
  let ref: number | null = null;
  for (let i = s; i <= e; i++) {
    const a = pts[i]!.ele;
    if (a == null) continue;
    if (ref == null) {
      ref = a;
      continue;
    }
    const d = a - ref;
    if (d >= HYSTERESIS_M) {
      gain += d;
      ref = a;
    } else if (d <= -HYSTERESIS_M) {
      loss -= d;
      ref = a;
    }
  }
  const a = pts[s]!;
  const b = pts[e]!;
  const lengthM = b.d - a.d;
  return {
    lengthM,
    gainM: ref == null ? null : gain,
    lossM: ref == null ? null : loss,
    gradePct: a.ele != null && b.ele != null && lengthM > 0 ? ((b.ele - a.ele) / lengthM) * 100 : null,
    durationS: a.t != null && b.t != null ? b.t - a.t : null,
  };
}

export interface CreateForm {
  name: string;
  description: string;
  sport: RankedSportClass | "";
  /** Nur Strecke: Ort als freier Text. */
  location: string;
}

/** Sportklasse der Aktivität, wenn sie Bestenlisten hat; sonst muss gewählt werden (unbekannt, Schwimmen, Studio). */
export function initialSport(activitySport: SportClass): RankedSportClass | "" {
  return RANKED_CLASSES.find((c) => c === activitySport) ?? "";
}

/** Erster Grund, warum noch nicht erstellt werden kann. */
export function createProblem(form: CreateForm, lengthM: number, minLen: number): TextKey | null {
  if (lengthM < minLen) return "create.problem.tooShort";
  if (!form.name.trim()) return "create.problem.name";
  if (!form.sport) return "create.problem.sport";
  return null;
}

/** Ziehen auf der Karte: so weit entlang der Aufzeichnung sucht ein Schritt nach einem näheren Punkt. */
const FOLLOW_WINDOW_M = 150;

/**
 * Ziehen auf der Karte, ohne zu springen (Uli 12.11.2026): Der Punkt folgt der Aufzeichnung von seiner bisherigen
 * Lage aus vor oder zurück, schrittweise zum nächsten Punkt innerhalb von 150 m entlang der Linie, bis keiner
 * näher an der Maus liegt. Eine parallele Rückfahrt oder spätere Runde erreicht er so nur entlang der Linie.
 */
export function follow(pts: SelPoint[], q: { lat: number; lon: number }, lo: number, hi: number, current: number): number {
  if (pts.length === 0 || hi < lo) return lo;
  let i = Math.min(Math.max(current, lo), hi);
  for (let step = 0; step < 1000; step++) {
    const here = pts[i]!.d;
    let best = i;
    let bd = distanceM(q, pts[i]!);
    for (let j = i - 1; j >= lo && here - pts[j]!.d <= FOLLOW_WINDOW_M; j--) {
      const d = distanceM(q, pts[j]!);
      if (d < bd) [best, bd] = [j, d];
    }
    for (let j = i + 1; j <= hi && pts[j]!.d - here <= FOLLOW_WINDOW_M; j++) {
      const d = distanceM(q, pts[j]!);
      if (d < bd) [best, bd] = [j, d];
    }
    if (best === i) return i;
    i = best;
  }
  return i;
}

/** Ausschnitt um die Auswahl für das vergrößerte Profil: Auswahl plus je ein Viertel, mindestens 50 m Rand. */
export function detailWindow(pts: SelPoint[], [s, e]: Range): { from: number; to: number } {
  const a = pts[s]?.d ?? 0;
  const b = pts[e]?.d ?? a;
  const margin = Math.max((b - a) / 4, 50);
  const first = pts[0]?.d ?? 0;
  const last = pts[pts.length - 1]?.d ?? first;
  return { from: Math.max(first, a - margin), to: Math.min(last, b + margin) };
}

/**
 * Bestehende Segmente, die fast gleich sind: Start und Ziel je höchstens 30 m entfernt, Länge höchstens 10 % anders
 * (RFC-0016). Strecken vergleicht das Backend über den Verlauf (`POST /segments/similar`).
 */
export function similarSegments<S extends { id: string; length_m: number; start?: { lat: number; lon: number }; end?: { lat: number; lon: number } }>(
  segments: S[],
  start: { lat: number; lon: number },
  end: { lat: number; lon: number },
  lengthM: number,
): S[] {
  const radius = 30;
  return segments.filter(
    (x) =>
      x.start != null &&
      x.end != null &&
      distanceM(x.start, start) <= radius &&
      distanceM(x.end, end) <= radius &&
      Math.abs(x.length_m - lengthM) <= 0.1 * Math.max(lengthM, x.length_m),
  );
}
