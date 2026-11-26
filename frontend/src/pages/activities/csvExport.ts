// CSV der Aktivitätenliste (RFC-0019 „CSV der Aktivitätenliste“): genau die gefilterte und sortierte Liste über
// alle Seiten, Kopfzeile und Zahlen nach Sprache und Format der Oberfläche. Deutsches Format (Dezimalkomma) mit `;`,
// sonst `,`. UTF-8 mit BOM, Zeilenende CRLF.
import { apiGet, type Activity, type ActivityListResponse, type Season, type SubSportCatalog } from "../../api/client";
import { t, type TextKey } from "../../i18n";
import { decimalSeparator, duration, localDate, localDateTime, plainNumber } from "../../utils/format";
import { subSportLabel } from "../../utils/sports";
import { apiQuery, type ListFilter } from "./listModel";

/** Seitengröße beim Abholen (Höchstwert der API). */
const FETCH_LIMIT = 200;

/** Trenner passend zum Dezimalzeichen. */
export function csvDelimiter(): string {
  return decimalSeparator() === "," ? ";" : ",";
}

function cell(value: string, delimiter: string): string {
  return /["\r\n]/.test(value) || value.includes(delimiter) ? `"${value.replace(/"/g, '""')}"` : value;
}

/** CSV-Text ohne BOM. */
export function toCsv(rows: Activity[], timeZone?: string, catalog?: SubSportCatalog): string {
  const d = csvDelimiter();
  const head: TextKey[] = [
    "csv.col.start",
    "csv.col.title",
    "csv.col.sport",
    "csv.col.subSport",
    "csv.col.location",
    "csv.col.distance",
    "csv.col.time",
    "csv.col.avg",
    "csv.col.gain",
    "csv.col.hr",
    "csv.col.load",
  ];
  const lines = [head.map((k) => cell(t(k), d)).join(d)];
  for (const a of rows) {
    const values = [
      localDateTime(a.start_time, timeZone),
      a.title,
      t(`sport.${a.sport_class}` as TextKey),
      a.sub_sport ? subSportLabel(catalog, a.sport_class, a.sub_sport) : "",
      a.location ?? "",
      plainNumber(a.distance_m == null ? null : a.distance_m / 1000, 2),
      duration(a.total_time_ms),
      plainNumber(a.avg_speed_kmh, 1),
      plainNumber(a.elevation_gain_m),
      plainNumber(a.avg_heart_rate_bpm),
      plainNumber(a.load, 1),
    ];
    lines.push(values.map((v) => cell(v, d)).join(d));
  }
  return lines.join("\r\n") + "\r\n";
}

/** Alle Seiten der gefilterten Liste abholen. */
export async function fetchAll(
  filter: ListFilter,
  today: string,
  season: Pick<Season, "start_date" | "end_date"> | null,
  fetcher: (path: string) => Promise<ActivityListResponse> = (path) => apiGet<ActivityListResponse>(path),
): Promise<Activity[]> {
  const out: Activity[] = [];
  for (;;) {
    const page = await fetcher(`/activities?${apiQuery(filter, today, out.length, FETCH_LIMIT, season)}`);
    out.push(...page.data);
    if (page.data.length === 0 || out.length >= page.meta.total) return out;
  }
}

/** Datei im Browser speichern. */
export function saveFile(name: string, text: string, type: string) {
  const url = URL.createObjectURL(new Blob(["﻿", text], { type }));
  const a = document.createElement("a");
  a.href = url;
  a.download = name;
  document.body.appendChild(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

export function csvFileName(timeZone?: string): string {
  return `trackcraft-aktivitaeten-${localDate(new Date(), timeZone)}.csv`;
}
