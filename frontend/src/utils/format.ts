// Anzeigeformate nach RFC-0006 „Sprache und Formate“: Gebietsschema aus der Formatwahl (Vorgabe de-DE),
// metrisch, Zeitzone der TOML. Alle Datums-, Zahl- und Zeitausgaben laufen über diese Datei.
import { formatLocale } from "../i18n";

/** Dezimaltrennzeichen des Gebietsschemas (Komma bzw. Punkt). */
export function decimalSeparator(): string {
  return new Intl.NumberFormat(formatLocale()).formatToParts(1.5).find((p) => p.type === "decimal")?.value ?? ",";
}

/** Zahl mit festen Nachkommastellen und Tausendertrennung des Gebietsschemas. */
export function num(value: number, digits = 0): string {
  return new Intl.NumberFormat(formatLocale(), { minimumFractionDigits: digits, maximumFractionDigits: digits }).format(value);
}

/** Meter als Kilometer mit zwei Nachkommastellen, `–` bei unbekannt. */
export function km(meters: number | null | undefined): string {
  return meters == null ? "–" : num(meters / 1000, 2);
}

/** Dauer `h:mm:ss` bzw. `m:ss`, `–` bei unbekannt. */
export function duration(ms: number | null | undefined): string {
  if (ms == null) return "–";
  const total = Math.round(ms / 1000);
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const ss = String(s).padStart(2, "0");
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${ss}` : `${m}:${ss}`;
}

/** Segmentzeit mit Zehnteln `m:ss,t` bzw. `h:mm:ss,t` (RFC-0006, Trennzeichen des Gebietsschemas), Zehntel abgeschnitten wie die Rangliste in ms. */
export function segmentTime(ms: number | null | undefined): string {
  if (ms == null) return "–";
  const tenths = Math.floor(ms / 100);
  const total = Math.floor(tenths / 10);
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const ss = String(total % 60).padStart(2, "0");
  const base = h > 0 ? `${h}:${String(m).padStart(2, "0")}:${ss}` : `${m}:${ss}`;
  return `${base}${decimalSeparator()}${tenths % 10}`;
}

/** Eingabe `h:mm:ss` oder `m:ss` in Millisekunden; `null` bei ungültig. */
export function parseDuration(text: string): number | null {
  const parts = text.trim().split(":");
  if (parts.length < 2 || parts.length > 3 || parts.some((p) => !/^\d+$/.test(p))) return null;
  const n = parts.map(Number);
  const [h = 0, m = 0, s = 0] = n.length === 3 ? n : [0, ...n];
  if (m >= 60 && n.length === 3) return null;
  if (s >= 60) return null;
  return ((h * 60 + m) * 60 + s) * 1000;
}

/** Zeitpunkt `TT.MM.JJ HH:MM` (Tabellen) bzw. `TT.MM.JJJJ HH:MM` in der gegebenen Zeitzone (Muster de-DE). */
export function dateTime(iso: string | null | undefined, timeZone?: string, long = false): string {
  if (!iso) return "–";
  const d = new Date(iso);
  const time = new Intl.DateTimeFormat(formatLocale(), { timeZone, hour: "2-digit", minute: "2-digit" }).format(d);
  return `${dateOnly(iso, timeZone, long)} ${time}`;
}

/** Datum `TT.MM.JJ` (Tabellen) bzw. `TT.MM.JJJJ` in der gegebenen Zeitzone (Muster de-DE). */
export function dateOnly(iso: string | null | undefined, timeZone?: string, long = false): string {
  if (!iso) return "–";
  return new Intl.DateTimeFormat(formatLocale(), { timeZone, day: "2-digit", month: "2-digit", year: long ? "numeric" : "2-digit" }).format(
    new Date(iso),
  );
}

/** Kalendertag `JJJJ-MM-TT` als Mittag UTC, damit keine Zeitzone den Tag verschiebt. */
function dayInstant(day: string): Date {
  return new Date(`${day}T12:00:00Z`);
}

/** Kalendertag `JJJJ-MM-TT` als Datum, z. B. `24.11.2026` (Muster de-DE). */
export function dayDate(day: string): string {
  return new Intl.DateTimeFormat(formatLocale(), { timeZone: "UTC", day: "2-digit", month: "2-digit", year: "numeric" }).format(dayInstant(day));
}

/** Kalendertag ohne Jahr, z. B. `24.11.` (Muster de-DE). */
export function dayMonth(day: string): string {
  return new Intl.DateTimeFormat(formatLocale(), { timeZone: "UTC", day: "2-digit", month: "2-digit" }).format(dayInstant(day));
}

/** Zeitraum zweier Kalendertage ohne Jahr, z. B. `23.–29.11.` oder `30.11.–06.12.` (Muster de-DE). */
export function dayRange(from: string, to: string): string {
  const f = new Intl.DateTimeFormat(formatLocale(), { timeZone: "UTC", day: "2-digit", month: "2-digit" });
  return f.formatRange(dayInstant(from), dayInstant(to)).replace(/\s–\s/, "–");
}

/** Wochentage 0 = Montag bis 6 = Sonntag, z. B. für Kalenderköpfe. */
export const WEEK = [0, 1, 2, 3, 4, 5, 6] as const;

/** Kurzer Wochentag, 0 = Montag (Wochen beginnen nach ISO immer am Montag), z. B. `Mo`. */
export function weekdayShort(index: number): string {
  // 2024-01-01 war ein Montag
  const s = new Intl.DateTimeFormat(formatLocale(), { timeZone: "UTC", weekday: "short" }).format(dayInstant(addDays("2024-01-01", index)));
  return s.replace(/\.$/, "");
}

/** Kurzer Monatsname, 0 = Januar, z. B. `Mär`. */
export function monthShort(index: number): string {
  const s = new Intl.DateTimeFormat(formatLocale(), { timeZone: "UTC", month: "short" }).format(new Date(Date.UTC(2024, index, 15)));
  return s.replace(/\.$/, "");
}

/** Langer Monatsname, 0 = Januar, z. B. `März`. */
export function monthLong(index: number): string {
  return new Intl.DateTimeFormat(formatLocale(), { timeZone: "UTC", month: "long" }).format(new Date(Date.UTC(2024, index, 15)));
}

/** Monat und Jahr eines Kalendertags, z. B. `November 2026`. */
export function monthYear(day: string): string {
  return new Intl.DateTimeFormat(formatLocale(), { timeZone: "UTC", month: "long", year: "numeric" }).format(dayInstant(day));
}

/** Großschreibung des ersten Zeichens nach den Regeln des Gebietsschemas. */
export function upperFirst(text: string): string {
  return text.charAt(0).toLocaleUpperCase(formatLocale()) + text.slice(1);
}

/** Bytes als MB mit einer Nachkommastelle. */
export function megabytes(bytes: number): string {
  return num(bytes / 1024 / 1024, 1);
}

/** Dateigröße mit Einheit: unter 1 GB in MB, sonst in GB, je eine Nachkommastelle. */
export function fileSize(bytes: number): string {
  const gb = bytes / 1024 ** 3;
  return gb >= 1 ? `${num(gb, 1)} GB` : `${megabytes(bytes)} MB`;
}

/** Pace `m:ss` je km aus km/h; 3600 / Schnitt, auf ganze Sekunden gerundet (RFC-0006). */
export function pace(kmh: number | null | undefined): string {
  if (kmh == null || !(kmh > 0)) return "–";
  const s = Math.round(3600 / kmh);
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}

/** Schnitt als Wert und Einheit: Laufen als Pace, sonst km/h mit einer Nachkommastelle. */
export function speed(kmh: number | null | undefined, running: boolean): { value: string; unit: string } {
  if (running) return { value: pace(kmh), unit: "/km" };
  return { value: kmh == null ? "–" : num(kmh, 1), unit: "km/h" };
}

/** Summe von Stunden als `h:mm` (Kennzahl-Karten). */
export function hoursMinutes(ms: number): string {
  const totalMin = Math.round(ms / 60_000);
  return `${num(Math.floor(totalMin / 60))}:${String(totalMin % 60).padStart(2, "0")}`;
}

/** Lokales Datum `JJJJ-MM-TT` in der Zeitzone der TOML. */
export function localDate(date: Date, timeZone?: string): string {
  const parts = new Intl.DateTimeFormat("en-CA", { timeZone, year: "numeric", month: "2-digit", day: "2-digit" }).formatToParts(date);
  const get = (t: Intl.DateTimeFormatPartTypes) => parts.find((p) => p.type === t)?.value ?? "";
  return `${get("year")}-${get("month")}-${get("day")}`;
}

/** Kalendertag verschieben (`JJJJ-MM-TT`), ohne Zeitzonen. */
export function addDays(day: string, days: number): string {
  const d = new Date(`${day}T12:00:00Z`);
  d.setUTCDate(d.getUTCDate() + days);
  return d.toISOString().slice(0, 10);
}
