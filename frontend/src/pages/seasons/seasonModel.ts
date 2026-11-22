// Saisons (RFC-0009 „Saisons“): reine Logik für Auswahl, Zuordnung und Formular.
import { ApiError, type Season, type SeasonInput } from "../../api/client";
import { t } from "../../i18n";

/** Lokaler Kalendertag `YYYY-MM-DD` eines Zeitpunkts in der Zeitzone der Installation. */
export function localDate(iso: string | null | undefined, timeZone?: string): string | null {
  if (!iso) return null;
  // en-CA liefert das ISO-Format
  return new Intl.DateTimeFormat("en-CA", { year: "numeric", month: "2-digit", day: "2-digit", timeZone }).format(new Date(iso));
}

/** Liegt der lokale Tag `day` in der Saison (letzter Tag eingeschlossen)? */
export function inSeason(day: string | null, s: Pick<Season, "start_date" | "end_date">): boolean {
  return day != null && day >= s.start_date && day <= s.end_date;
}

/** `YYYY-MM-DD` als `TT.MM.JJJJ`. */
export function germanDate(day: string): string {
  const [y, m, d] = day.split("-");
  return `${d}.${m}.${y}`;
}

/** Zeitraum zur Anzeige, z. B. „01.03.2026 – 31.10.2026“. */
export function seasonRange(s: Pick<Season, "start_date" | "end_date">): string {
  return `${germanDate(s.start_date)} – ${germanDate(s.end_date)}`;
}

export interface SeasonForm {
  name: string;
  start_date: string;
  end_date: string;
}

export const emptyForm: SeasonForm = { name: "", start_date: "", end_date: "" };

export function formOf(s: Season): SeasonForm {
  return { name: s.name, start_date: s.start_date, end_date: s.end_date };
}

/** Prüfung vor dem Senden; Überschneidung und Namensgleichheit meldet der Server. */
export function formProblem(f: SeasonForm): string | null {
  if (!f.name.trim()) return t("season.problem.name");
  if (!f.start_date || !f.end_date) return t("season.problem.dates");
  if (f.end_date < f.start_date) return t("season.problem.order");
  return null;
}

/** Nur geänderte Felder; `null`, wenn sich nichts geändert hat. Ohne `before`: alle Felder (Anlegen). */
export function inputOf(f: SeasonForm, before?: Season): SeasonInput | null {
  const name = f.name.trim();
  if (!before) return { name, start_date: f.start_date, end_date: f.end_date };
  const p: SeasonInput = {};
  if (name !== before.name) p.name = name;
  if (f.start_date !== before.start_date) p.start_date = f.start_date;
  if (f.end_date !== before.end_date) p.end_date = f.end_date;
  return Object.keys(p).length > 0 ? p : null;
}

/** Verständlicher Text zu den Konflikten des Servers, mit der anderen Saison. */
export function conflictText(error: unknown): string | null {
  if (!(error instanceof ApiError)) return null;
  if (error.code === "season_name_taken") return t("season.problem.taken");
  if (error.code === "season_overlap") {
    const other = error.details.season as Season | undefined;
    return other ? t("season.problem.overlap", { name: other.name, range: seasonRange(other) }) : t("season.problem.overlapAny");
  }
  return null;
}
