// Importdialog als reine Logik (RFC-0006 „Importdialog“, RFC-0008 Abschnitt 8.1): Dateien, Zeilen je Session,
// Entscheidungen und die daraus gebauten Bestätigungen. Ohne React, damit es ohne Browser testbar ist.
//
// Ein Vorgang ohne Duplikat und ohne Zeiteingabe wird direkt nach dem Hochladen gespeichert (gesperrte Sessions
// übersprungen). Nur Vorgänge mit Duplikat oder Zeiteingabe warten auf eine Entscheidung. Titel, Beschreibung
// und Ort werden auf der Detailseite der Aktivität bearbeitet, nicht im Importdialog.
import type { CarryOver, ImportDecision, ImportJob, ImportSession, SessionDecision } from "../../api/client";
import { parseDuration } from "../../utils/format";

/** checked: geprüft, wartet auf Entscheidung; saved: gespeichert; discarded: offene Entscheidung verworfen. */
export type FileStatus = "waiting" | "uploading" | "checked" | "saved" | "discarded" | "rejected";

export interface FileEntry {
  key: string;
  name: string;
  size: number;
  status: FileStatus;
  /** Upload-Fortschritt 0 bis 1 */
  progress: number;
  job?: ImportJob;
  /** Gesperrte Sessions beim letzten Prüfen; ein abgeschlossener Vorgang meldet „bereits vorhanden“ nicht mehr */
  lockedSessions?: ImportSession[];
  /** Sessions, die mit `import` oder `replace` gespeichert wurden */
  saved?: number[];
  error?: { code: string; message: string };
}

export function lockedOf(job: ImportJob): ImportSession[] {
  return job.sessions.filter((s) => s.state !== "selectable");
}

/** Gesperrte Sessions aller Dateien für „Nicht importierbar“. */
export function lockedRows(entries: FileEntry[]): Row[] {
  return entries.flatMap((e) =>
    (e.lockedSessions ?? []).map((s) => ({
      key: `${e.key}:${s.session_index}`,
      jobId: e.job?.id ?? e.key,
      fileName: e.name,
      session: s,
      kind: "locked" as const,
    })),
  );
}

/** ready: ohne Konflikt; duplicate: vermutetes Duplikat; time: Start und Gesamtzeit fehlen; locked: nicht importierbar. */
export type RowKind = "ready" | "duplicate" | "time" | "locked";

export interface Row {
  key: string;
  jobId: string;
  fileName: string;
  session: ImportSession;
  kind: RowKind;
}

export type DuplicateAction = "skip" | "import" | "replace";

export interface Choice {
  /** time: speichern ja/nein */
  include: boolean;
  /** duplicate: ohne Wahl offen */
  action?: DuplicateAction;
  /** duplicate: gewählter Kandidat (Vergleich und Ersetzungsziel) */
  target?: string;
  carry: CarryOver;
  /** time: `YYYY-MM-DDTHH:MM` aus dem Eingabefeld */
  startLocal: string;
  /** time: `h:mm:ss` */
  durationText: string;
  /** time: nötig, nachdem das Backend die Zeit als doppelte Herbststunde gemeldet hat */
  ambiguous: boolean;
  occurrence?: "first" | "second";
  /** Fehlermeldung des Backends zu dieser Session */
  problem?: string;
}

export const ALL_CARRY: CarryOver = { title: true, description: true, location: true, sport_class: true, sub_sport: true };

export function rowKey(jobId: string, sessionIndex: number): string {
  return `${jobId}:${sessionIndex}`;
}

export function classify(s: ImportSession): RowKind {
  if (s.state !== "selectable") return "locked";
  if (s.time_input_required) return "time";
  if (s.duplicate_candidates.length > 0) return "duplicate";
  return "ready";
}

/** Ohne Duplikat und ohne Zeiteingabe: wird ohne Rückfrage gespeichert. */
export function autoConfirmable(job: ImportJob): boolean {
  return job.status === "awaiting_decision" && job.sessions.every((s) => classify(s) === "ready" || classify(s) === "locked");
}

/** Bestätigung ohne Rückfrage: alle wählbaren Sessions importieren, gesperrte überspringen. */
export function autoDecision(job: ImportJob): ImportDecision {
  return {
    state_token: job.state_token ?? "",
    sessions: job.sessions.map((s) => ({ session_index: s.session_index, action: classify(s) === "ready" ? "import" : "skip" })),
  };
}

/** Zeilen aller offenen Vorgänge, in Dateireihenfolge. */
export function rowsOf(entries: FileEntry[]): Row[] {
  return entries.flatMap((e) =>
    e.job && e.job.status === "awaiting_decision"
      ? e.job.sessions.map((s) => ({ key: rowKey(e.job!.id, s.session_index), jobId: e.job!.id, fileName: e.name, session: s, kind: classify(s) }))
      : [],
  );
}

/** Gespeicherte Sessions aller Dateien, für die Liste „Importiert“. */
export function savedRows(entries: FileEntry[]): Row[] {
  return entries.flatMap((e) =>
    e.job && e.saved?.length
      ? e.job.sessions
          .filter((s) => e.saved!.includes(s.session_index))
          .map((s) => ({ key: rowKey(e.job!.id, s.session_index), jobId: e.job!.id, fileName: e.name, session: s, kind: "ready" as const }))
      : [],
  );
}

export function initialChoice(row: Row): Choice {
  return {
    include: row.kind === "ready" || row.kind === "time",
    target: row.session.duplicate_candidates[0]?.activity_id,
    carry: { ...ALL_CARRY },
    startLocal: "",
    durationText: "",
    ambiguous: false,
  };
}

function startValid(c: Choice): boolean {
  return /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}(:\d{2})?$/.test(c.startLocal);
}

/** Wartet die Zeile noch auf eine Entscheidung oder Eingabe? Dann ist „Bestätigen“ gesperrt. */
export function isOpen(row: Row, c: Choice): boolean {
  switch (row.kind) {
    case "duplicate":
      return c.action === undefined;
    case "time":
      return c.include && (!startValid(c) || parseDuration(c.durationText) === null || (c.ambiguous && !c.occurrence));
    default:
      return false;
  }
}

/** Wird aus der Zeile eine Aktivität gespeichert (neu oder ersetzt)? */
export function willSave(row: Row, c: Choice): boolean {
  switch (row.kind) {
    case "ready":
    case "time":
      return c.include;
    case "duplicate":
      return c.action === "import" || c.action === "replace";
    default:
      return false;
  }
}

/** Entscheidung für eine Session; gesperrte und abgewählte werden übersprungen. */
export function sessionDecision(row: Row, c: Choice): SessionDecision {
  const index = row.session.session_index;
  const skip: SessionDecision = { session_index: index, action: "skip" };
  switch (row.kind) {
    case "locked":
      return skip;
    case "ready":
      return c.include ? { session_index: index, action: "import" } : skip;
    case "time": {
      if (!c.include) return skip;
      const local = c.startLocal.length === 16 ? `${c.startLocal}:00` : c.startLocal;
      return {
        session_index: index,
        action: "import",
        start_time: c.occurrence ? { local, occurrence: c.occurrence } : { local },
        total_time_ms: parseDuration(c.durationText) ?? 0,
      };
    }
    case "duplicate":
      if (c.action === "import") return { session_index: index, action: "import" };
      if (c.action === "replace" && c.target) {
        return { session_index: index, action: "replace", replace_activity_id: c.target, carry_over: c.carry };
      }
      return skip;
  }
}

export function decisionFor(job: ImportJob, rows: Row[], choices: Record<string, Choice>): ImportDecision {
  const own = rows.filter((r) => r.jobId === job.id);
  return {
    state_token: job.state_token ?? "",
    sessions: own.map((r) => sessionDecision(r, choices[r.key] ?? initialChoice(r))),
  };
}

/** Sessions, die eine Entscheidung speichert. */
export function savedIndices(decision: ImportDecision): number[] {
  return decision.sessions.filter((s) => s.action !== "skip").map((s) => s.session_index);
}

export interface Summary {
  files: number;
  bytes: number;
  imported: number;
  needDecision: number;
  open: number;
  notImportable: number;
  toSave: number;
}

export function summarize(entries: FileEntry[], rows: Row[], choices: Record<string, Choice>): Summary {
  const c = (r: Row) => choices[r.key] ?? initialChoice(r);
  const deciding = rows.filter((r) => r.kind === "duplicate" || r.kind === "time");
  return {
    files: entries.length,
    bytes: entries.reduce((s, e) => s + e.size, 0),
    imported: entries.reduce((n, e) => n + (e.saved?.length ?? 0), 0),
    needDecision: deciding.length,
    open: rows.filter((r) => isOpen(r, c(r))).length,
    notImportable: entries.reduce((n, e) => n + (e.lockedSessions?.length ?? 0), 0) + entries.filter((e) => e.status === "rejected" && e.error).length,
    toSave: rows.filter((r) => willSave(r, c(r))).length,
  };
}

/** Ergebnis über alle bestätigten Vorgänge. */
export interface Outcome {
  imported: number;
  replaced: number;
  skipped: number;
  failed: number;
}

export function countOutcome(decision: ImportDecision, job: ImportJob, acc: Outcome, lockedSkips = 0): Outcome {
  if (job.status !== "done") return { ...acc, failed: acc.failed + decision.sessions.filter((s) => s.action !== "skip").length };
  return {
    imported: acc.imported + decision.sessions.filter((s) => s.action === "import").length,
    replaced: acc.replaced + decision.sessions.filter((s) => s.action === "replace").length,
    // Gesperrte Sessions zählen als „nicht importierbar“, nicht als übersprungen.
    skipped: acc.skipped + decision.sessions.filter((s) => s.action === "skip").length - lockedSkips,
    failed: acc.failed,
  };
}

/** Index der Session aus einem Feldfehler wie `sessions[2].start_time`. */
export function fieldSession(field: string, decision: ImportDecision): number | undefined {
  const m = /^sessions\[(\d+)\]/.exec(field);
  return m ? decision.sessions[Number(m[1])]?.session_index : undefined;
}

/** Dateien, die gar nicht erst hochgeladen werden (Endung, Größe); sonst `null`. */
export function precheck(file: { name: string; size: number }, maxMb: number): { code: string; message: string } | null {
  if (/\.zip$/i.test(file.name)) return { code: "unsupported_format", message: "ZIP" };
  if (file.size > maxMb * 1024 * 1024) return { code: "file_too_large", message: `${maxMb} MB` };
  return null;
}
