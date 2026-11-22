// Segmentliste und Segmentseite (RFC-0006 „Segmentliste“, „Segmentseite“); reine Logik ohne React.
import { inSeason, localDate } from "../seasons/seasonModel";
import {
  apiGet,
  type Effort,
  type RankedSportClass,
  type Segment,
  type SegmentCategory,
  type SegmentKind,
  type SegmentTrackResponse,
  type SegmentUpdate,
} from "../../api/client";
import { categoriesFor } from "../../components/CategoryIcon";
import type { TrackPoint } from "../activities/trackModel";
import { RANKED_CLASSES } from "../../utils/sports";

export type SegmentSort = "name" | "length_m" | "created_at" | "location";

export interface SegmentFilter {
  /** Seite Segmente oder Seite Strecken (eigene Einträge in der Kopfzeile). */
  kind: SegmentKind;
  q: string;
  sport: RankedSportClass | null;
  /** Kategorie passend zur Art, sonst alle. */
  category: SegmentCategory | null;
  sort: SegmentSort;
  desc: boolean;
}

export const PAGE_SIZE = 50;
export { RANKED_CLASSES };
const SORTS: SegmentSort[] = ["name", "length_m", "created_at", "location"];

/** Adresse der Liste: Segmente und Strecken haben je eine eigene Seite. */
export function listPath(kind: SegmentKind): string {
  return kind === "route" ? "/strecken" : "/segmente";
}

/** Adresse der Seite eines Segments bzw. einer Strecke. */
export function segmentPath(kind: SegmentKind | undefined, id: string): string {
  return `${listPath(kind ?? "segment")}/${id}`;
}

/** Filter aus der Adresse; so bleibt er beim Zurückkommen aus der Segmentseite erhalten. */
export function readFilter(p: URLSearchParams, kind: SegmentKind): SegmentFilter {
  const raw = p.get("sport");
  const sortRaw = p.get("sort") ?? "name";
  const field = sortRaw.replace(/^-/, "") as SegmentSort;
  const known = SORTS.includes(field);
  const cat = p.get("art") as SegmentCategory | null;
  return {
    kind,
    q: p.get("q") ?? "",
    sport: RANKED_CLASSES.includes(raw as RankedSportClass) ? (raw as RankedSportClass) : null,
    category: cat && categoriesFor(kind).includes(cat) ? cat : null,
    sort: known ? field : "name",
    desc: known ? sortRaw.startsWith("-") : false,
  };
}

export function writeFilter(f: SegmentFilter): URLSearchParams {
  const p = new URLSearchParams();
  if (f.q.trim()) p.set("q", f.q);
  if (f.sport) p.set("sport", f.sport);
  if (f.category) p.set("art", f.category);
  if (f.sort !== "name" || f.desc) p.set("sort", `${f.desc ? "-" : ""}${f.sort}`);
  return p;
}

/** Klick auf einen Spaltenkopf: Name zuerst aufsteigend, Zahlen zuerst absteigend, dann umgekehrt. */
export function toggleSort(f: SegmentFilter, field: SegmentSort): SegmentFilter {
  return f.sort === field ? { ...f, desc: !f.desc } : { ...f, sort: field, desc: field !== "name" && field !== "location" };
}

/** Query für `GET /segments`. */
export function apiQuery(f: SegmentFilter, offset: number, limit = PAGE_SIZE): string {
  const p = new URLSearchParams();
  p.set("kind", f.kind);
  const q = f.q.trim();
  if (q) p.set("q", q);
  if (f.sport) p.set("sport_class", f.sport);
  if (f.category) p.set("category", f.category);
  p.set("sort", `${f.desc ? "-" : ""}${f.sort}`);
  p.set("limit", String(limit));
  p.set("offset", String(offset));
  return p.toString();
}

/** Schnitt in km/h aus Segmentlänge und Dauer; `null` ohne Dauer. */
export function effortKmh(lengthM: number, durationMs: number | null | undefined): number | null {
  return durationMs != null && durationMs > 0 ? (lengthM / (durationMs / 1000)) * 3.6 : null;
}

interface Page<T> {
  data: T[];
  meta: { total: number };
}

/** Alle Seiten einer Liste nacheinander laden (die API liefert höchstens 200 je Seite). */
export async function fetchAll<T>(path: string, get: <R>(p: string) => Promise<R> = apiGet): Promise<T[]> {
  const out: T[] = [];
  const sep = path.includes("?") ? "&" : "?";
  for (;;) {
    const page = await get<Page<T>>(`${path}${sep}limit=200&offset=${out.length}`);
    out.push(...page.data);
    if (page.data.length === 0 || out.length >= page.meta.total) return out;
  }
}

export interface ProgressPoint {
  /** Start des Versuchs in ms seit 1970. */
  x: number;
  /** Dauer in Sekunden. */
  d: number;
  /** Bis dahin beste Dauer in Sekunden (Stufenlinie). */
  best: number;
  /** Dauer noch einmal, wenn der Versuch zur Herkunftsaktivität gehört. */
  own: number | null;
  effort: Effort;
}

/** Bestzeiten-Entwicklung: gültige Versuche nach Datum, dazu die jeweils aktuelle Bestzeit. */
export function progression(efforts: Effort[], ownActivityId?: string): ProgressPoint[] {
  const valid = efforts
    .filter((e) => e.status === "valid" && e.start_time != null && e.duration_ms != null)
    .map((e) => ({ e, x: Date.parse(e.start_time!), d: e.duration_ms! / 1000 }))
    .sort((a, b) => a.x - b.x || a.e.attempt_position - b.e.attempt_position);
  let best = Infinity;
  return valid.map(({ e, x, d }) => {
    best = Math.min(best, d);
    return { x, d, best, own: e.activity_id === ownActivityId ? d : null, effort: e };
  });
}

/** Bester gültiger Versuch der Herkunftsaktivität, sonst ihr erster nicht gewerteter. */
export function ownEffort(efforts: Effort[], activityId: string | undefined): Effort | null {
  if (!activityId) return null;
  const mine = efforts.filter((e) => e.activity_id === activityId);
  const ranked = mine.filter((e) => e.rank != null).sort((a, b) => a.rank! - b.rank!);
  return ranked[0] ?? mine[0] ?? null;
}

/** Zuletzt begonnener Versuch. */
export function lastEffort(efforts: Effort[]): Effort | null {
  let last: Effort | null = null;
  for (const e of efforts) {
    if (e.start_time && (!last?.start_time || e.start_time > last.start_time)) last = e;
  }
  return last;
}

/** Referenz als Punkte für Karte und Höhenprofil. */
export function referencePoints(track: SegmentTrackResponse["data"]): TrackPoint[] {
  return track.lat.map((lat, i) => ({
    timeS: null,
    distM: track.dist_m[i] ?? null,
    lat,
    lon: track.lon[i] ?? null,
    ele: track.ele_m[i] ?? null,
    hr: null,
    kmh: null,
  }));
}

export interface SegmentForm {
  name: string;
  description: string;
  kind: SegmentKind;
  location: string;
  /** `"auto"`: automatische Kategorie. */
  category: SegmentCategory | "auto";
}

export function initialForm(s: Segment): SegmentForm {
  const manual = s.category != null && s.category !== s.category_auto;
  return { name: s.name, description: s.description ?? "", kind: s.kind, location: s.location ?? "", category: manual ? s.category! : "auto" };
}

/** Nur geänderte Felder als PATCH-Inhalt; `null`, wenn nichts geändert oder der Name leer ist. */
export function patchOf(s: Segment, f: SegmentForm): SegmentUpdate | null {
  const name = f.name.trim();
  if (!name) return null;
  const p: SegmentUpdate = {};
  if (name !== s.name) p.name = name;
  const description = f.description.trim() ? f.description : null;
  if (description !== s.description) p.description = description;
  if (f.kind !== s.kind) p.kind = f.kind;
  const location = f.location.trim() || null;
  if (location !== (s.location ?? null)) p.location = location;
  const before = initialForm(s).category;
  // Nach einem Artwechsel gilt eine Wahl nur, wenn sie zur neuen Art passt; sonst bestimmt der Server neu.
  if (f.kind !== s.kind) {
    if (f.category !== "auto" && categoriesFor(f.kind).includes(f.category)) p.category = f.category;
  } else if (f.category !== before) p.category = f.category === "auto" ? null : f.category;
  return Object.keys(p).length > 0 ? p : null;
}

/** Kalenderjahr eines Zeitpunkts in der Zeitzone der Installation. */
export function localYear(iso: string | null | undefined, timeZone?: string): number | null {
  if (!iso) return null;
  const y = new Intl.DateTimeFormat("en", { year: "numeric", timeZone }).format(new Date(iso));
  return Number(y);
}

/** Zeitraum der Bestenliste (RFC-0006): gesamt, ein Kalenderjahr oder eine Saison (lokale Tage, Ende einschließlich). */
export type BoardPeriod =
  | { kind: "all" }
  | { kind: "year"; year: number }
  | { kind: "season"; start_date: string; end_date: string };

/** Jahre mit Versuchen, neueste zuerst; das laufende Jahr ist immer dabei. */
export function effortYears(efforts: Effort[], now: Date, timeZone?: string): number[] {
  const years = new Set<number>([localYear(now.toISOString(), timeZone)!]);
  for (const e of efforts) {
    const y = localYear(e.start_time, timeZone);
    if (y != null) years.add(y);
  }
  return [...years].sort((a, b) => b - a);
}

/** Vermerk in der Spalte Bestzeit: Bestzeit einer Saison vor Bestzeit eines Jahres. */
export type BestBadge = { kind: "season"; name: string } | { kind: "year"; year: number };

export interface BoardRow {
  effort: Effort;
  /** `null` bei der absoluten Bestzeit und bei Versuchen, die weder Saison- noch Jahresbestzeit sind. */
  badge: BestBadge | null;
  /** Platz im gewählten Zeitraum; `null` ohne Wertung. */
  rank: number | null;
  /** Rückstand zur Bestzeit des Zeitraums in ms; 0 bei der Bestzeit selbst. */
  behindMs: number | null;
  /** Ø-HF gegenüber der Bestzeit des Zeitraums. */
  hr: "up" | "down" | null;
}

export interface Board {
  rows: BoardRow[];
  best: Effort | null;
  /** Bester gültiger Versuch im laufenden Jahr (Marker „Bestzeit 2026“). */
  yearBestId: string | null;
  year: number;
}

/** Bester gültiger Versuch je Gruppe (Jahr, Saison); Schlüssel `null` = gehört zu keiner Gruppe. */
function bestBy<K>(efforts: Effort[], group: (e: Effort) => K | null): Map<string, K> {
  const best = new Map<K, Effort>();
  for (const e of efforts) {
    const k = group(e);
    if (k == null) continue;
    const b = best.get(k);
    if (!b || byDuration(e, b) < 0) best.set(k, e);
  }
  return new Map([...best].map(([k, e]) => [e.id, k]));
}

const byDuration = (a: Effort, b: Effort) =>
  a.duration_ms! - b.duration_ms! || (a.start_time ?? "").localeCompare(b.start_time ?? "") || a.attempt_position - b.attempt_position;

/** Bestenliste nach Zeitraum: gültige nach Dauer, dahinter nicht gewertete; Rückstand und HF-Vergleich zur Bestzeit. */
export function board(
  efforts: Effort[],
  period: BoardPeriod,
  now: Date,
  timeZone?: string,
  seasons: { name: string; start_date: string; end_date: string }[] = [],
): Board {
  const year = localYear(now.toISOString(), timeZone)!;
  const inYear = (e: Effort) => localYear(e.start_time, timeZone) === year;
  const valid = (e: Effort) => e.status === "valid" && e.duration_ms != null;
  const allValid = efforts.filter(valid);
  const yearBest = allValid.filter(inYear).sort(byDuration)[0] ?? null;
  // Vermerke über alle Versuche, unabhängig vom gezeigten Zeitraum
  const absolute = [...allValid].sort(byDuration)[0] ?? null;
  const seasonBests = bestBy(allValid, (e) => {
    const day = localDate(e.start_time, timeZone);
    return seasons.find((s) => inSeason(day, s)) ?? null;
  });
  const yearBests = bestBy(allValid, (e) => localYear(e.start_time, timeZone));
  const badgeOf = (e: Effort): BestBadge | null => {
    if (!valid(e) || e === absolute || e.rank === 1) return null;
    const season = seasonBests.get(e.id);
    if (season) return { kind: "season", name: season.name };
    const y = yearBests.get(e.id);
    return y != null ? { kind: "year", year: y } : null;
  };
  const shown =
    period.kind === "year"
      ? efforts.filter((e) => localYear(e.start_time, timeZone) === period.year)
      : period.kind === "season"
        ? efforts.filter((e) => inSeason(localDate(e.start_time, timeZone), period))
        : efforts;
  const ranked = shown.filter(valid).sort(byDuration);
  const best = ranked[0] ?? null;
  const rest = shown.filter((e) => !valid(e));
  const row = (e: Effort, rank: number | null): BoardRow => {
    const hrBest = best?.avg_heart_rate_bpm;
    const hr = e.avg_heart_rate_bpm;
    return {
      effort: e,
      badge: badgeOf(e),
      rank,
      behindMs: rank != null && best ? e.duration_ms! - best.duration_ms! : null,
      hr: e === best || hr == null || hrBest == null || hr === hrBest ? null : hr > hrBest ? "up" : "down",
    };
  };
  return { rows: [...ranked.map((e, i) => row(e, i + 1)), ...rest.map((e) => row(e, null))], best, yearBestId: yearBest?.id ?? null, year };
}
