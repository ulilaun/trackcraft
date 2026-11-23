import type { PlanPhase, SuggestionInput } from "../../api/client";

/** Vorgabe, wenn noch nichts übernommen wurde: Di/Do je 1 h, Sa 2 h, So 1,5 h. */
export const DEFAULT_INPUT: SuggestionInput = {
  sport_class: "cycling",
  minutes: [0, 60, 0, 60, 0, 120, 90],
  max_gain: 5,
  recovery_weeks: true,
  intensity: "polarized",
};

export const MIN_UNIT = 30;
export const MAX_DAY = 600;

export const PHASE_COLOR: Record<PlanPhase, string> = {
  build: "var(--color-accent)",
  recovery: "var(--color-muted)",
  taper: "var(--color-good)",
};

/** Text eines Minutenfelds als ganze Zahl 0 bis 600, sonst `null`. */
export function parseMinutes(s: string): number | null {
  const v = s.trim() === "" ? 0 : Number(s.trim());
  return Number.isInteger(v) && v >= 0 && v <= MAX_DAY ? v : null;
}

/** Prüft die Eingaben wie das Backend; liefert den Schlüssel der Meldung oder `null`. */
export function inputProblem(texts: string[]): "plan.suggest.problem.minutes" | "plan.suggest.problem.noDay" | null {
  const values = texts.map(parseMinutes);
  if (values.some((v) => v == null)) return "plan.suggest.problem.minutes";
  if (values.every((v) => (v ?? 0) < MIN_UNIT)) return "plan.suggest.problem.noDay";
  return null;
}

/** Eingaben für die Anfrage aus dem Formularzustand. */
export function requestInput(base: Omit<SuggestionInput, "minutes">, texts: string[]): SuggestionInput {
  return { ...base, minutes: texts.map((s) => parseMinutes(s) ?? 0) };
}

/** Wochenstunden aus den Minuten je Wochentag (nur Tage ab 30 min). */
export function weeklyHours(texts: string[]): number {
  return texts.map((s) => parseMinutes(s) ?? 0).filter((m) => m >= MIN_UNIT).reduce((a, b) => a + b, 0) / 60;
}
