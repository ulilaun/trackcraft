// Importdialog (RFC-0006 „Importdialog“, RFC-0008 Abschnitt 8.1): Ablagefläche, Upload nacheinander, Speichern ohne
// Rückfrage für Dateien ohne Konflikt, Einzelentscheidungen für Duplikate und fehlende Zeitangaben, Ergebnis.
// Jede Datei ist ein eigener Importvorgang (RFC-0003 Abschnitt 8); die API ist unverändert.
import { useEffect, useMemo, useRef, useState } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { useQueryClient } from "@tanstack/react-query";
import {
  apiDelete,
  apiGet,
  apiPost,
  apiUpload,
  ApiError,
  OfflineError,
  type FieldError,
  type ImportDecision,
  type ImportJob,
  type ImportJobResponse,
} from "../../api/client";
import { Card, KpiCard } from "../../components/Card";
import { useConfig } from "../../hooks/useConfig";
import { errorText, t } from "../../i18n";
import {
  autoConfirmable,
  autoDecision,
  countOutcome,
  decisionFor,
  fieldSession,
  initialChoice,
  lockedOf,
  lockedRows,
  precheck,
  rowKey,
  rowsOf,
  savedIndices,
  savedRows,
  summarize,
  type Choice,
  type FileEntry,
  type Outcome,
} from "./model";
import {
  ActivityTable,
  ConfirmBar,
  DropZone,
  DuplicateCard,
  NotImportable,
  primaryButton,
  secondaryButton,
  SummaryCards,
  TimeCard,
  UploadList,
} from "./parts";

type Phase = "empty" | "upload" | "review" | "saving" | "done";

const NO_OUTCOME: Outcome = { imported: 0, replaced: 0, skipped: 0, failed: 0 };

interface Problem {
  code: string;
  message: string;
  fields: FieldError[];
}

function describe(e: unknown): Problem {
  if (e instanceof ApiError) return { code: e.code, message: e.message, fields: e.fields };
  if (e instanceof OfflineError) return { code: "offline", message: t("state.offline"), fields: [] };
  return { code: "internal_error", message: String(e), fields: [] };
}

/** Ergebnis einer Bestätigung für eine Datei. */
type Confirmed =
  | { kind: "saved"; patch: Partial<FileEntry>; outcome: (acc: Outcome) => Outcome; notices: string[] }
  | { kind: "changed"; patch: Partial<FileEntry> }
  | { kind: "error"; problem: Problem };

async function confirmFile(entry: FileEntry, job: ImportJob, decision: ImportDecision): Promise<Confirmed> {
  try {
    const res = await apiPost<ImportJobResponse>(`/import/jobs/${job.id}/confirm`, decision);
    const done = res.data.status === "done";
    const lockedSkips = entry.lockedSessions?.length ?? 0;
    return {
      kind: "saved",
      patch: done
        ? { status: "saved", job: res.data, saved: savedIndices(decision) }
        : { status: "rejected", job: res.data, error: res.data.error ?? { code: "internal_error", message: "" } },
      outcome: (acc) => countOutcome(decision, res.data, acc, lockedSkips),
      notices: (res.meta?.notices ?? []).map((n) =>
        n.code === "file_checksum_ignored"
          ? t("import.notice.checksum", { name: entry.name })
          : t("import.notice.shifted", { message: `${entry.name}: ${n.message}` }),
      ),
    };
  } catch (e) {
    if (e instanceof ApiError && (e.code === "import_state_changed" || e.code === "import_not_confirmable")) {
      // Neu laden; der Vorgang kommt in die Einzelentscheidung (RFC-0008 Abschnitt 8.1, Punkt 4).
      try {
        const fresh = (await apiGet<ImportJobResponse>(`/import/jobs/${job.id}`)).data;
        return fresh.status === "awaiting_decision"
          ? { kind: "changed", patch: { job: fresh, status: "checked", lockedSessions: lockedOf(fresh) } }
          : { kind: "changed", patch: { job: fresh, status: fresh.status === "done" ? "saved" : "rejected" } };
      } catch (e2) {
        return { kind: "error", problem: describe(e2) };
      }
    }
    return { kind: "error", problem: describe(e) };
  }
}

let counter = 0;

/** Navigationszustand, mit dem das Dashboard abgelegte Dateien an den Importdialog übergibt. */
export interface ImportHandoff {
  files?: File[];
}

export function ImportPage() {
  const config = useConfig();
  const tz = config.data?.data.timezone;
  const maxMb = config.data?.data.import_limits.max_file_size_mb ?? 50;
  const navigate = useNavigate();
  const location = useLocation();
  const queryClient = useQueryClient();

  const [phase, setPhase] = useState<Phase>("empty");
  const [entries, setEntries] = useState<FileEntry[]>([]);
  const [choices, setChoices] = useState<Record<string, Choice>>({});
  const [outcome, setOutcome] = useState<Outcome>(NO_OUTCOME);
  const [notices, setNotices] = useState<string[]>([]);

  const rows = useMemo(() => rowsOf(entries), [entries]);
  // Jede Zeile hat eine Wahl; fehlende mit Vorgabe ergänzen, ohne den Zustand zu ändern.
  const all = useMemo(() => {
    const out: Record<string, Choice> = {};
    for (const r of rows) out[r.key] = choices[r.key] ?? initialChoice(r);
    return out;
  }, [rows, choices]);
  const summary = summarize(entries, rows, all);

  const setChoice = (key: string, patch: Partial<Choice>) => {
    const row = rows.find((r) => r.key === key);
    if (!row) return;
    setChoices((prev) => ({ ...prev, [key]: { ...(prev[key] ?? initialChoice(row)), ...patch } }));
  };

  function reset() {
    setPhase("empty");
    setEntries([]);
    setChoices({});
    setOutcome(NO_OUTCOME);
    setNotices([]);
  }

  async function uploadAll(files: File[]) {
    const items = files.map((file) => ({
      file,
      entry: { key: `f${++counter}`, name: file.name, size: file.size, status: "waiting", progress: 0 } as FileEntry,
    }));
    let current = items.map((i) => i.entry);
    const update = (key: string, patch: Partial<FileEntry>) => {
      current = current.map((e) => (e.key === key ? { ...e, ...patch } : e));
      setEntries(current);
    };
    const messages: string[] = [];
    let changed = false;
    setEntries(current);
    setChoices({});
    setOutcome(NO_OUTCOME);
    setNotices([]);
    setPhase("upload");

    // Immer nur ein Upload zur Zeit (RFC-0008 Abschnitt 8.1).
    for (const { file, entry } of items) {
      const key = entry.key;
      const rejected = precheck(file, maxMb);
      if (rejected) {
        update(key, { status: "rejected", progress: 1, error: rejected });
        continue;
      }
      update(key, { status: "uploading" });
      let job: ImportJob;
      try {
        job = (await apiUpload<ImportJobResponse>("/activities/import", file, (p) => update(key, { progress: p }))).data;
      } catch (e) {
        update(key, { status: "rejected", progress: 1, error: describe(e) });
        continue;
      }
      const checked: FileEntry = { ...entry, status: "checked", progress: 1, job, lockedSessions: lockedOf(job) };
      update(key, checked);
      if (!autoConfirmable(job)) continue;

      // Ohne Duplikat und ohne Zeiteingabe: sofort speichern.
      const result = await confirmFile(checked, job, autoDecision(job));
      if (result.kind === "saved") {
        update(key, result.patch);
        setOutcome(result.outcome);
        messages.push(...result.notices);
      } else if (result.kind === "changed") {
        changed = true;
        update(key, result.patch);
      } else {
        update(key, { status: "rejected", error: result.problem });
      }
    }

    if (changed) messages.unshift(t("import.notice.stateChanged"));
    setNotices(messages);
    void queryClient.invalidateQueries();
    setPhase(rowsOf(current).length > 0 ? "review" : "done");
  }

  // Dateien, die auf dem Dashboard abgelegt wurden (RFC-0018 „Quick-Upload-Zone“), einmal übernehmen.
  const handed = useRef(false);
  const handedFiles = (location.state as ImportHandoff | null)?.files;
  useEffect(() => {
    if (handed.current || !handedFiles?.length) return;
    handed.current = true;
    void navigate(location.pathname, { replace: true, state: null });
    void uploadAll(handedFiles);
    // eslint-disable-next-line react-hooks/exhaustive-deps -- nur beim Öffnen mit übergebenen Dateien
  }, [handedFiles]);

  async function confirmAll() {
    setPhase("saving");
    let current = entries;
    const nextChoices = { ...all };
    const messages: string[] = [];
    let changed = false;
    let problems = false;

    for (const entry of entries) {
      const job = entry.job;
      if (!job || job.status !== "awaiting_decision") continue;
      const decision = decisionFor(job, rows, all);
      const result = await confirmFile(entry, job, decision);
      if (result.kind !== "error") {
        current = current.map((e) => (e.key === entry.key ? { ...e, ...result.patch } : e));
        if (result.kind === "saved") {
          setOutcome(result.outcome);
          messages.push(...result.notices);
        } else {
          changed = true;
          for (const s of job.sessions) delete nextChoices[rowKey(job.id, s.session_index)];
        }
        continue;
      }
      // Feldfehler an der Session anzeigen, sonst an allen Sessions der Datei.
      problems = true;
      const p = result.problem;
      const targets = p.fields.map((f) => fieldSession(f.field, decision)).filter((i): i is number => i !== undefined);
      const indices = targets.length ? targets : job.sessions.map((s) => s.session_index);
      for (const i of indices) {
        const key = rowKey(job.id, i);
        const c = nextChoices[key];
        if (!c) continue;
        nextChoices[key] = { ...c, problem: errorText(p.code, p.message), ambiguous: c.ambiguous || p.code === "local_time_ambiguous" };
      }
    }

    setEntries(current);
    setChoices(nextChoices);
    if (changed) messages.unshift(t("import.notice.stateChanged"));
    else if (problems) messages.unshift(t("import.notice.problems"));
    setNotices(messages);
    void queryClient.invalidateQueries();
    setPhase(rowsOf(current).length > 0 ? "review" : "done");
  }

  async function discardOpen() {
    // Offene Vorgänge abbrechen und Uploads bereinigen; schon gespeicherte bleiben. Fehler ignorieren,
    // das Backend räumt nach 24 h auf.
    const open = entries.filter((e) => e.job?.status === "awaiting_decision");
    await Promise.allSettled(open.map((e) => apiDelete(`/import/jobs/${e.job!.id}`)));
    const keys = new Set(open.map((e) => e.key));
    const skipped = open.reduce((n, e) => n + e.job!.sessions.filter((s) => s.state === "selectable").length, 0);
    setEntries((prev) => prev.map((e) => (keys.has(e.key) ? { ...e, job: undefined, status: "discarded" } : e)));
    setOutcome((o) => ({ ...o, skipped: o.skipped + skipped }));
    setPhase("done");
  }

  const title = <h1 className="text-2xl font-semibold">{t("import.title")}</h1>;
  const noticeList = notices.map((n) => (
    <p key={n} className="rounded-card border border-gold bg-card p-3 text-sm">
      {n}
    </p>
  ));

  if (phase === "empty") {
    return (
      <div className="grid grid-cols-1 gap-4">
        {title}
        <DropZone maxMb={maxMb} onFiles={(f) => void uploadAll(f)} />
      </div>
    );
  }

  if (phase === "upload") {
    return (
      <div className="grid grid-cols-1 gap-4">
        {title}
        <UploadList entries={entries} />
      </div>
    );
  }

  const saved = savedRows(entries);
  const importedTable = saved.length > 0 && (
    <ActivityTable title={`✓ ${t("import.done.title")}`} hint={t("import.done.hint")} rows={saved} tz={tz} />
  );
  const rejected = entries.filter((e) => e.status === "rejected" && e.error);
  const locked = lockedRows(entries);
  const notImportable = (locked.length > 0 || rejected.length > 0) && <NotImportable rows={locked} rejected={rejected} tz={tz} />;

  if (phase === "done") {
    return (
      <div className="grid grid-cols-1 gap-4">
        {title}
        {noticeList}
        <div className="grid grid-cols-2 gap-4 lg:grid-cols-4">
          <KpiCard label={t("import.result.imported")} value={outcome.imported} unit={t("import.result.importedSub")} />
          <KpiCard label={t("import.result.replaced")} value={outcome.replaced} />
          <KpiCard label={t("import.result.skipped")} value={outcome.skipped} />
          <KpiCard label={t("import.result.rejected")} value={summary.notImportable} />
        </div>
        <Card title={t("import.result.evaluation")}>
          <p className="text-sm text-muted">{t("import.result.evaluationText")}</p>
          {outcome.failed > 0 && <p className="mt-2 text-sm text-danger">{t("import.result.failed", { n: outcome.failed })}</p>}
          <div className="mt-4 flex flex-wrap gap-2">
            <button type="button" className={primaryButton} onClick={() => navigate("/aktivitaeten")}>
              {t("import.result.toActivities")}
            </button>
            <button type="button" className={secondaryButton} onClick={reset}>
              {t("import.result.again")}
            </button>
          </div>
        </Card>
        {importedTable}
        {notImportable}
      </div>
    );
  }

  // Offene Entscheidungen (Duplikate, Zeiteingabe)
  const deciding = rows.filter((r) => r.kind === "duplicate" || r.kind === "time");
  const pendingReady = rows.filter((r) => r.kind === "ready");
  return (
    <div className="grid grid-cols-1 gap-4 pb-2">
      {title}
      {noticeList}
      <SummaryCards s={summary} />
      {deciding.map((r) =>
        r.kind === "duplicate" ? (
          <DuplicateCard key={r.key} row={r} c={all[r.key] ?? initialChoice(r)} set={setChoice} tz={tz} />
        ) : (
          <TimeCard key={r.key} row={r} c={all[r.key] ?? initialChoice(r)} set={setChoice} />
        ),
      )}
      {pendingReady.length > 0 && <ActivityTable title={t("import.pending.title")} hint={t("import.pending.hint")} rows={pendingReady} tz={tz} />}
      {importedTable}
      {notImportable}
      <ConfirmBar s={summary} saving={phase === "saving"} onCancel={() => void discardOpen()} onConfirm={() => void confirmAll()} />
    </div>
  );
}
