// Reine Logik der Seite „Sicherung“ (#57): Zustand der Statuszeile und Abfragetakt.
import type { BackupStatus, Operation } from "../../api/client";

export type BackupState = "running" | "failed" | "missing" | "never" | "pending" | "ok";

/** Zustand für Punkt und Text der Statuszeile. Fehler und fehlendes Ziel gehen vor ungesicherten Änderungen. */
export function backupState(s: BackupStatus): BackupState {
  if (s.running_operation_id) return "running";
  if (s.last_attempt?.outcome === "failed") return "failed";
  if (s.target.state !== "available") return "missing";
  if (!s.last_success) return "never";
  return s.pending_changes ? "pending" : "ok";
}

/** Ein Vorgang ist beendet, sobald er einen Endzeitpunkt hat. */
export function finished(op: Operation | undefined): boolean {
  return Boolean(op?.finished_at);
}

/** Abfragetakt des Status: schnell, solange ein Vorgang läuft, sonst jede Minute. */
export function statusPollInterval(s: BackupStatus | undefined): number {
  return s?.running_operation_id ? 2_000 : 60_000;
}

/** Abfragetakt eines Vorgangs: jede Sekunde bis zum Ende, danach nicht mehr. */
export function operationPollInterval(op: Operation | undefined): number | false {
  return finished(op) ? false : 1_000;
}
