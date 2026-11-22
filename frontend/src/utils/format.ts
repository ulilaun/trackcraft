// Anzeigeformate nach RFC-0006 „Sprache und Formate“ (de-DE, metrisch, Zeitzone der TOML).

const LOCALE = "de-DE";

/** Zahl mit festen Nachkommastellen, Komma und Tausenderpunkt. */
export function num(value: number, digits = 0): string {
  return new Intl.NumberFormat(LOCALE, { minimumFractionDigits: digits, maximumFractionDigits: digits }).format(value);
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

/** Segmentzeit mit Zehnteln `m:ss,t` bzw. `h:mm:ss,t` (RFC-0006), Zehntel abgeschnitten wie die Rangliste in ms. */
export function segmentTime(ms: number | null | undefined): string {
  if (ms == null) return "–";
  const tenths = Math.floor(ms / 100);
  const total = Math.floor(tenths / 10);
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const ss = String(total % 60).padStart(2, "0");
  const base = h > 0 ? `${h}:${String(m).padStart(2, "0")}:${ss}` : `${m}:${ss}`;
  return `${base},${tenths % 10}`;
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

/** Zeitpunkt `TT.MM.JJ HH:MM` (Tabellen) bzw. `TT.MM.JJJJ HH:MM` in der gegebenen Zeitzone. */
export function dateTime(iso: string | null | undefined, timeZone?: string, long = false): string {
  if (!iso) return "–";
  const d = new Date(iso);
  const parts = new Intl.DateTimeFormat(LOCALE, {
    timeZone,
    day: "2-digit",
    month: "2-digit",
    year: long ? "numeric" : "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    hourCycle: "h23",
  }).formatToParts(d);
  const get = (t: Intl.DateTimeFormatPartTypes) => parts.find((p) => p.type === t)?.value ?? "";
  return `${get("day")}.${get("month")}.${get("year")} ${get("hour")}:${get("minute")}`;
}

/** Datum `TT.MM.JJ` (Tabellen) bzw. `TT.MM.JJJJ` in der gegebenen Zeitzone. */
export function dateOnly(iso: string | null | undefined, timeZone?: string, long = false): string {
  return iso ? dateTime(iso, timeZone, long).slice(0, long ? 10 : 8) : "–";
}

/** Bytes als MB mit einer Nachkommastelle. */
export function megabytes(bytes: number): string {
  return num(bytes / 1024 / 1024, 1);
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
