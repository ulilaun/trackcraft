// Filter und Sortierung der Aktivitätenliste (RFC-0006 „Aktivitätenliste“); reine Logik ohne React.
import type { Season, SportClass } from "../../api/client";
import { addDays } from "../../utils/format";
import { SPORT_CLASSES } from "../../utils/sports";

export type SortField = "start_time" | "distance_m" | "total_time_ms" | "elevation_gain_m" | "load";

/**
 * Zeitraum der Liste: Knopfreihe Gesamt · Letzte 12 Wochen · Jahr · Saison · Von–Bis.
 * Von–Bis darf eine Seite offen lassen.
 */
export type ListPeriod =
  | { kind: "all" }
  | { kind: "last12" }
  | { kind: "year"; year: number }
  | { kind: "season"; id: string }
  | { kind: "range"; from: string | null; to: string | null };

export interface ListFilter {
  /** Vorgabe „Letzte 12 Wochen“; in der Adresse `zeitraum`, `jahr`, `saison`, `von`/`bis`. */
  period: ListPeriod;
  q: string;
  sport: SportClass | null;
  sub: string | null;
  /** Strecke (RFC-0016): nur Aktivitäten mit gültigem Versuch darauf. */
  route: string | null;
  sort: SortField;
  desc: boolean;
}

export const PAGE_SIZE = 50;
export { SPORT_CLASSES };

const isDay = (v: string | null): v is string => !!v && /^\d{4}-\d{2}-\d{2}$/.test(v);

/** Zeitraum aus der Adresse (`zeitraum`, `jahr`, `saison`, `von`/`bis`); Vorgabe „Letzte 12 Wochen“. */
export function readPeriod(p: URLSearchParams): ListPeriod {
  const season = p.get("saison");
  if (season) return { kind: "season", id: season };
  const year = Number(p.get("jahr"));
  if (Number.isInteger(year) && year >= 1900 && year <= 9999) return { kind: "year", year };
  const z = p.get("zeitraum");
  if (z === "frei") {
    const from = p.get("von");
    const to = p.get("bis");
    return { kind: "range", from: isDay(from) ? from : null, to: isDay(to) ? to : null };
  }
  return z === "alle" ? { kind: "all" } : { kind: "last12" };
}

/** Filter aus der Adresse; so bleibt er beim Zurückkommen aus der Detailseite erhalten. */
export function readFilter(p: URLSearchParams): ListFilter {
  const raw = p.get("sport");
  const sport = SPORT_CLASSES.includes(raw as SportClass) ? (raw as SportClass) : null;
  const sortRaw = p.get("sort") ?? "-start_time";
  const desc = sortRaw.startsWith("-");
  const field = sortRaw.replace(/^-/, "");
  const fields: SortField[] = ["start_time", "distance_m", "total_time_ms", "elevation_gain_m", "load"];
  return {
    period: readPeriod(p),
    q: p.get("q") ?? "",
    sport,
    sub: sport ? p.get("sub") : null,
    route: p.get("strecke") || null,
    sort: fields.includes(field as SortField) ? (field as SortField) : "start_time",
    desc: fields.includes(field as SortField) ? desc : true,
  };
}

export function writeFilter(f: ListFilter): URLSearchParams {
  const p = new URLSearchParams();
  writePeriod(p, f.period);
  if (f.q.trim()) p.set("q", f.q);
  if (f.sport) p.set("sport", f.sport);
  if (f.sport && f.sub) p.set("sub", f.sub);
  if (f.route) p.set("strecke", f.route);
  if (f.sort !== "start_time" || !f.desc) p.set("sort", `${f.desc ? "-" : ""}${f.sort}`);
  return p;
}

/** Zeitraum in die Adresse; „Letzte 12 Wochen“ als Vorgabe ohne Parameter. */
export function writePeriod(p: URLSearchParams, z: ListPeriod): void {
  if (z.kind === "all") p.set("zeitraum", "alle");
  else if (z.kind === "year") p.set("jahr", String(z.year));
  else if (z.kind === "season") p.set("saison", z.id);
  else if (z.kind === "range") {
    p.set("zeitraum", "frei");
    if (z.from) p.set("von", z.from);
    if (z.to) p.set("bis", z.to);
  }
}

/** Klick auf einen Spaltenkopf: erst absteigend, dann aufsteigend. */
export function toggleSort(f: ListFilter, field: SortField): ListFilter {
  return f.sort === field ? { ...f, desc: !f.desc } : { ...f, sort: field, desc: true };
}

/** Name einer Sportklasse: ganze Klasse; dieselbe Klasse noch einmal behält ihren Untertyp. */
export function selectSport(f: ListFilter, sport: SportClass | null): ListFilter {
  return { ...f, sport, sub: sport === f.sport ? f.sub : null };
}

/** Eintrag im Untertyp-Menü einer Sportklasse (`null`: alle Untertypen). */
export function selectSportSub(f: ListFilter, sport: SportClass, sub: string | null): ListFilter {
  return { ...f, sport, sub };
}

const LAST12_DAYS = 83;

/** Erster und letzter Tag (je einschließlich) eines Zeitraums; `null`: offen. */
export function periodDays(
  z: ListPeriod,
  today: string,
  season?: Pick<Season, "start_date" | "end_date"> | null,
): { from: string | null; to: string | null } {
  switch (z.kind) {
    case "all":
      return { from: null, to: null };
    case "last12":
      return { from: addDays(today, -LAST12_DAYS), to: null };
    case "year":
      return { from: `${z.year}-01-01`, to: `${z.year}-12-31` };
    case "season":
      return { from: season?.start_date ?? null, to: season?.end_date ?? null };
    case "range":
      // Vertauschte Grenzen nimmt die Liste in der richtigen Reihenfolge
      return z.from && z.to && z.from > z.to ? { from: z.to, to: z.from } : { from: z.from, to: z.to };
  }
}

/**
 * Von–Bis übernimmt den gerade gewählten Zeitraum zum Anpassen (bei „Letzte 12 Wochen“ bis heute).
 */
export function rangeFrom(z: ListPeriod, today: string, season?: Pick<Season, "start_date" | "end_date"> | null): ListPeriod {
  if (z.kind === "range") return z;
  const d = periodDays(z, today, season);
  return { kind: "range", from: d.from, to: z.kind === "last12" ? today : d.to };
}

/**
 * Query für `GET /activities`; `today` ist der lokale Tag in der Zeitzone der TOML, `season` der Zeitraum
 * der gewählten Saison (ganze lokale Tage, Ende einschließlich).
 */
export function apiQuery(
  f: ListFilter,
  today: string,
  offset: number,
  limit = PAGE_SIZE,
  season?: Pick<Season, "start_date" | "end_date"> | null,
): string {
  const p = new URLSearchParams();
  const d = periodDays(f.period, today, season);
  if (d.from) p.set("from", d.from);
  if (d.to) p.set("to", d.to);
  const q = f.q.trim();
  if (q) p.set("q", q);
  if (f.sport) p.set("sport_class", f.sport);
  if (f.sport && f.sub) p.set("sub_sport", f.sub);
  if (f.route) p.set("route_id", f.route);
  p.set("sort", `${f.desc ? "-" : ""}${f.sort}`);
  p.set("limit", String(limit));
  p.set("offset", String(offset));
  return p.toString();
}

/** Jahre zur Auswahl: mit Aktivitäten, dazu das laufende und das gewählte; neueste zuerst. */
export function yearChoices(withData: number[], current: number, picked: number | null): number[] {
  const all = new Set([...withData, current, ...(picked != null ? [picked] : [])]);
  return [...all].sort((a, b) => b - a);
}
