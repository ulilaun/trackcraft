// Bearbeiten einer Aktivität (RFC-0017 „Anzeigen und bearbeiten“): Formularzustand und PATCH-Inhalt, ohne React.
import type { Activity, ActivityUpdate, SportClass, SubSportCatalog, SubSportEntry } from "../../api/client";
import { duration, parseDuration } from "../../utils/format";
import { SPORT_CLASSES } from "../../utils/sports";

export { SPORT_CLASSES };

/** Auswahlwert „Neuer Untertyp …“; der Dialog legt ihn vor dem Speichern im Katalog an. */
export const NEW_SUB_SPORT = "__new__";

export interface EditForm {
  title: string;
  description: string;
  location: string;
  sportClass: SportClass;
  /** Schlüssel im Katalog der Sportklasse, `""` ohne Untertyp, `NEW_SUB_SPORT` für einen neuen */
  subSport: string;
  /** Name des neuen Untertyps, nur mit `NEW_SUB_SPORT` */
  newSubSport: string;
  /** `YYYY-MM-DDTHH:MM:SS` in der Zeitzone der TOML (Eingabefeld `datetime-local` mit Sekunden) */
  startLocal: string;
  /** `h:mm:ss` oder `m:ss` */
  durationText: string;
  /** nötig, nachdem das Backend die Startzeit als doppelte Herbststunde gemeldet hat */
  ambiguous: boolean;
  occurrence?: "first" | "second";
}

/** Zeitpunkt als lokale Zeit `YYYY-MM-DDTHH:MM:SS` in der gegebenen Zeitzone. */
export function toLocalInput(iso: string, timeZone?: string): string {
  const parts = new Intl.DateTimeFormat("en-CA", {
    timeZone,
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hourCycle: "h23",
  }).formatToParts(new Date(iso));
  const get = (t: Intl.DateTimeFormatPartTypes) => parts.find((p) => p.type === t)?.value ?? "";
  return `${get("year")}-${get("month")}-${get("day")}T${get("hour")}:${get("minute")}:${get("second")}`;
}

/** Eingabefelder liefern ohne Sekunden `YYYY-MM-DDTHH:MM`; das Backend erwartet Sekunden. */
function withSeconds(local: string): string {
  return local.length === 16 ? `${local}:00` : local;
}

export function initialForm(a: Activity, timeZone?: string): EditForm {
  return {
    title: a.title,
    description: a.description ?? "",
    location: a.location ?? "",
    sportClass: a.sport_class,
    subSport: a.sub_sport ?? "",
    newSubSport: "",
    startLocal: toLocalInput(a.start_time, timeZone),
    durationText: duration(a.total_time_ms),
    ambiguous: false,
  };
}

export type FormProblem = "title_empty" | "sub_sport_name_empty" | "start_invalid" | "duration_invalid" | "occurrence_missing";

/** Erster Eingabefehler, der das Speichern sperrt, sonst `null`. */
export function formProblem(f: EditForm): FormProblem | null {
  if (!f.title.trim()) return "title_empty";
  if (f.subSport === NEW_SUB_SPORT && !f.newSubSport.trim()) return "sub_sport_name_empty";
  if (!/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}(:\d{2})?$/.test(f.startLocal)) return "start_invalid";
  if (parseDuration(f.durationText) === null) return "duration_invalid";
  if (f.ambiguous && !f.occurrence) return "occurrence_missing";
  return null;
}

/** Leere Freitexte werden `null` (Feld gelöscht). */
function optionalText(s: string): string | null {
  const v = s.trim();
  return v ? v : null;
}

/**
 * Nur geänderte Felder als PATCH-Inhalt; `null`, wenn nichts geändert ist.
 * Start und Gesamtzeit werden nur geschickt, wenn sie sich wirklich ändern, damit die Herkunft
 * nicht ohne Grund „manuell korrigiert“ wird.
 */
export function patchOf(a: Activity, f: EditForm, timeZone?: string): ActivityUpdate | null {
  const p: ActivityUpdate = {};
  const title = f.title.trim();
  if (title !== a.title) p.title = title;
  const description = f.description.trim() ? f.description : null;
  if (description !== a.description) p.description = description;
  const location = optionalText(f.location);
  if (location !== a.location) p.location = location;
  if (f.sportClass !== a.sport_class) p.sport_class = f.sportClass;
  const subSport = f.subSport || null;
  if (subSport !== a.sub_sport) p.sub_sport = subSport;

  const start = withSeconds(f.startLocal);
  if (start !== toLocalInput(a.start_time, timeZone) || f.occurrence) {
    p.start_time = f.occurrence ? { local: start, occurrence: f.occurrence } : { local: start };
  }
  const ms = parseDuration(f.durationText);
  if (ms !== null && ms !== Math.round(a.total_time_ms / 1000) * 1000) p.total_time_ms = ms;

  return Object.keys(p).length > 0 ? p : null;
}

/** Auswahl für den Untertyp: Katalog der gewählten Sportklasse (nur Katalogeinträge, neue legt der Dialog an). */
export function subSportOptions(catalog: SubSportCatalog | undefined, sportClass: SportClass): SubSportEntry[] {
  return catalog?.[sportClass] ?? [];
}

/** Sportklasse gewechselt: ein Untertyp, den die neue Klasse nicht kennt, entfällt (wie im Backend). */
export function withSportClass(f: EditForm, sportClass: SportClass, catalog: SubSportCatalog | undefined): EditForm {
  const keep = f.subSport === NEW_SUB_SPORT || subSportOptions(catalog, sportClass).some((e) => e.key === f.subSport);
  return { ...f, sportClass, subSport: keep ? f.subSport : "" };
}
