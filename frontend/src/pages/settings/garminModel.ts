// Reine Logik des Abschnitts „Garmin Connect“: Statuszeile, Intervall-Auswahl, Abfragetakt.
import type { GarminActivityStatus, GarminStatus } from "../../api/client";

/** Wählbare Abrufintervalle in Minuten. */
export const INTERVALS = [15, 30, 60, 120, 360, 1440] as const;

/** Intervalle für die Auswahl; ein anderer gespeicherter Wert bleibt wählbar. */
export function intervalOptions(current: number): number[] {
  const list: number[] = [...INTERVALS];
  return list.includes(current) ? list : [...list, current].sort((a, b) => a - b);
}

/** Status, die in „Nicht übernommen“ erscheinen. */
export const UNSETTLED: GarminActivityStatus[] = ["locked", "rejected", "failed"];

export function unsettledCount(s: GarminStatus): number {
  return UNSETTLED.reduce((n, k) => n + s.counts[k], 0);
}

export type GarminState = "unconfigured" | "running" | "error" | "ok" | "never";

/** Zustand für Punkt und Text der Statuszeile. */
export function garminState(s: GarminStatus): GarminState {
  if (!s.configured) return "unconfigured";
  if (s.running) return "running";
  if (s.last_error) return "error";
  if (s.last_success_at) return "ok";
  return "never";
}

/** Nach „Jetzt abrufen“ so lange schnell nachsehen, bis der Lauf als laufend erscheint. */
export const REQUEST_GRACE_MS = 15_000;

/** Abfragetakt des Status: schnell während eines Laufs (und kurz nach dem Anstoßen), sonst gemächlich. */
export function pollInterval(s: GarminStatus | undefined, requestedAt: number | null, now: number): number {
  if (s?.running) return 2_000;
  if (requestedAt !== null && now - requestedAt < REQUEST_GRACE_MS) return 1_000;
  return 30_000;
}

/** Ein Lauf ist gerade zu Ende gegangen: neue Aktivitäten können da sein. */
export function finishedRun(before: GarminStatus | undefined, after: GarminStatus | undefined): boolean {
  return Boolean(before?.running && after && !after.running);
}
