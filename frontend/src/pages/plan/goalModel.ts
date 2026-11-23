import type { PlanGoal, SportClass } from "../../api/client";
import { addDays, num, parseDuration } from "../../utils/format";

/** Eingaben des Ziel-Dialogs als Texte. */
export interface GoalForm {
  date: string;
  sportClass: Exclude<SportClass, "unknown">;
  name: string;
  description: string;
  url: string;
  /** Kilometer, Komma oder Punkt */
  distance: string;
  /** h:mm:ss */
  targetTime: string;
}

export type GoalProblem = "name" | "date" | "url" | "distance" | "targetTime";

export function goalFormOf(goal: PlanGoal | null, today: string, sportClass: GoalForm["sportClass"] = "cycling"): GoalForm {
  if (!goal) return { date: addDays(today, 28), sportClass, name: "", description: "", url: "", distance: "", targetTime: "" };
  return {
    date: goal.date,
    sportClass: goal.sport_class === "unknown" ? sportClass : goal.sport_class,
    name: goal.name,
    description: goal.description ?? "",
    url: goal.url ?? "",
    distance: goal.distance_m == null ? "" : String(Math.round(goal.distance_m) / 1000).replace(".", ","),
    targetTime: goal.target_time_s == null ? "" : hms(goal.target_time_s),
  };
}

/** Sekunden als `h:mm:ss` (auch unter einer Stunde mit Stunden). */
export function hms(seconds: number): string {
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  const s = Math.round(seconds % 60);
  return `${h}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
}

/** Kurzinfo eines Ziels: „120 km · 4:30:00“, leer ohne Angaben. */
export function goalFacts(g: PlanGoal): string {
  const parts: string[] = [];
  if (g.distance_m != null) parts.push(`${num(g.distance_m / 1000, g.distance_m % 1000 === 0 ? 0 : 1)} km`);
  if (g.target_time_s != null) parts.push(hms(g.target_time_s));
  return parts.join(" · ");
}

/** Kilometer als Meter; leer = `null`, ungültig = `undefined`. */
export function parseKm(text: string): number | null | undefined {
  const t = text.trim();
  if (!t) return null;
  if (!/^\d{1,5}([.,]\d{1,3})?$/.test(t)) return undefined;
  const v = Number(t.replace(",", "."));
  return v > 0 ? Math.round(v * 1000) : undefined;
}

/** Zielzeit `h:mm:ss` in Sekunden; leer = `null`, ungültig = `undefined`. */
export function parseTargetTime(text: string): number | null | undefined {
  const t = text.trim();
  if (!t) return null;
  if (t.split(":").length !== 3) return undefined;
  const ms = parseDuration(t);
  return ms != null && ms > 0 ? ms / 1000 : undefined;
}

function validUrl(text: string): boolean {
  const t = text.trim();
  return !t || (/^https?:\/\/\S+$/i.test(t) && t.length <= 500);
}

export function goalProblem(f: GoalForm): GoalProblem | null {
  if (!f.name.trim()) return "name";
  if (!/^\d{4}-\d{2}-\d{2}$/.test(f.date)) return "date";
  if (!validUrl(f.url)) return "url";
  if (parseKm(f.distance) === undefined) return "distance";
  if (parseTargetTime(f.targetTime) === undefined) return "targetTime";
  return null;
}

/** Body für POST/PATCH `/plan/goals`; leere Felder werden `null`. */
export function goalInput(f: GoalForm) {
  const text = (s: string) => (s.trim() ? s.trim() : null);
  return {
    date: f.date,
    sport_class: f.sportClass,
    name: f.name.trim(),
    description: text(f.description),
    url: text(f.url),
    distance_m: parseKm(f.distance) ?? null,
    target_time_s: parseTargetTime(f.targetTime) ?? null,
  };
}

/** Nächstes Ziel ab morgen (für Kacheln und Vorschau), sonst `null`. */
export function nextGoal(goals: PlanGoal[], today: string): PlanGoal | null {
  return goals.find((g) => g.date > today) ?? null;
}
