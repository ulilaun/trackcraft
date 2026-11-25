import { useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { CircleAlert, CircleCheck, HardDriveDownload, RotateCcw } from "lucide-react";
import {
  apiGet,
  apiPost,
  type BackupEntry,
  type BackupListResponse,
  type BackupStatus,
  type BackupStatusResponse,
  type Operation,
  type OperationResponse,
} from "../../api/client";
import { Card } from "../../components/Card";
import { Problem, problemText } from "../../components/Problem";
import { useConfig } from "../../hooks/useConfig";
import { t, tn, type TextKey } from "../../i18n";
import { dateTime, fileSize, num } from "../../utils/format";
import { RestoreDialog } from "./RestoreDialog";
import { backupState, finished, operationPollInterval, statusPollInterval, type BackupState } from "./backupModel";

const primary =
  "inline-flex items-center gap-1.5 rounded-lg bg-accent-strong px-3 py-1.5 text-sm font-medium text-white hover:opacity-90 disabled:opacity-50";

const restoreButton =
  "inline-flex items-center gap-1 rounded-lg border border-line bg-card-2 px-2 py-1 text-xs text-text hover:border-accent disabled:opacity-50";

const DOT: Record<BackupState, string> = {
  running: "bg-accent animate-pulse",
  failed: "bg-danger",
  missing: "bg-warn",
  never: "bg-warn",
  pending: "bg-warn",
  ok: "bg-good",
};

function StatusCard({
  s,
  op,
  tz,
  onRun,
  runError,
}: {
  s: BackupStatus;
  op: Operation | undefined;
  tz: string | undefined;
  onRun: () => void;
  runError: unknown;
}) {
  const state = backupState(s);
  const busy = state === "running";
  const phase = op && !finished(op) ? op.phase : null;
  return (
    <Card className="min-w-0">
      <div className="grid gap-3">
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div className="grid min-w-0 gap-1 text-sm">
            <p className="flex items-center gap-2 font-medium">
              <span className={`inline-block h-2.5 w-2.5 shrink-0 rounded-full ${DOT[state]}`} aria-hidden />
              {t(`backup.state.${state}` as TextKey)}
            </p>
            <p className="text-muted">
              {s.last_success
                ? t("backup.lastSuccess", {
                    time: dateTime(s.last_success.time, tz, true),
                  })
                : t("backup.noSuccess")}
            </p>
            {s.last_attempt && (
              <p className="text-muted">
                {t("backup.lastCheck", {
                  time: dateTime(s.last_attempt.time, tz, true),
                  trigger: t(`backup.trigger.${s.last_attempt.trigger}` as TextKey),
                  outcome: t(`backup.outcome.${s.last_attempt.outcome}` as TextKey),
                })}
              </p>
            )}
            {s.last_attempt?.reason && !busy && (
              <p className={`break-words ${s.last_attempt.outcome === "failed" ? "text-danger" : "text-muted"}`}>{s.last_attempt.reason}</p>
            )}
          </div>
          <button type="button" className={primary} disabled={busy || s.target.state !== "available"} onClick={onRun}>
            <HardDriveDownload size={14} className={busy ? "animate-pulse" : ""} aria-hidden />
            {busy ? t(phase ? (`backup.phase.${phase}` as TextKey) : "backup.running") : t("backup.runNow")}
          </button>
        </div>
        {runError != null && <p className="text-sm text-danger">{problemText(runError)}</p>}
        <dl className="grid gap-x-4 gap-y-1 text-sm sm:grid-cols-[auto_1fr]">
          <dt className="text-muted">{t("backup.target")}</dt>
          <dd className="min-w-0 break-all">
            <span className="font-mono text-xs">{s.target.path}</span>
            {s.target.state !== "available" ? (
              <span className="ml-2 text-warn">{t(`backup.target.${s.target.state}` as TextKey)}</span>
            ) : (
              s.target.free_bytes !== null && (
                <span className="ml-2 whitespace-nowrap text-muted">{t("backup.free", { size: fileSize(s.target.free_bytes) })}</span>
              )
            )}
          </dd>
          <dt className="text-muted">{t("backup.schedule")}</dt>
          <dd>{t("backup.scheduleText", { time: s.daily_time, keep: s.keep })}</dd>
        </dl>
        <p className="text-xs text-muted">{t("backup.hint")}</p>
      </div>
    </Card>
  );
}

function EntryRow({
  e,
  tz,
  latest,
  busy,
  onRestore,
}: {
  e: BackupEntry;
  tz: string | undefined;
  latest: boolean;
  busy: boolean;
  onRestore: () => void;
}) {
  return (
    <tr className="border-b border-line align-top last:border-0">
      <td className="num whitespace-nowrap px-2 py-1.5">
        {e.created_at ? dateTime(e.created_at, tz, true) : e.id}
        {latest && <span className="ml-2 rounded-full bg-card-2 px-2 py-0.5 text-xs text-muted">{t("backup.latest")}</span>}
      </td>
      <td className="whitespace-nowrap px-2 py-1.5">{e.trigger ? t(`backup.trigger.${e.trigger}` as TextKey) : "–"}</td>
      <td className="num whitespace-nowrap px-2 py-1.5 text-right">{num(e.activity_count)}</td>
      <td className="num whitespace-nowrap px-2 py-1.5 text-right">{fileSize(e.size_bytes)}</td>
      <td className="num whitespace-nowrap px-2 py-1.5 text-muted">
        {e.app_version ?? "–"} / {e.schema_version ?? "–"}
      </td>
      <td className="px-2 py-1.5">
        {e.missing_original_count > 0 && (
          <span className="block text-xs text-warn">{tn("backup.missingOriginals", e.missing_original_count)}</span>
        )}
        {e.damaged_original_count > 0 && (
          <span className="block text-xs text-warn">{tn("backup.damagedOriginals", e.damaged_original_count)}</span>
        )}
        {e.check_status === "ok" ? (
          <span className="flex items-center gap-1 text-good">
            <CircleCheck size={16} aria-hidden />
            {t("backup.complete")}
          </span>
        ) : (
          <span className="flex items-start gap-1 text-danger" title={e.problem ?? undefined}>
            <CircleAlert size={16} className="mt-0.5 shrink-0" aria-hidden />
            {t("backup.damaged")}
          </span>
        )}
      </td>
      <td className="whitespace-nowrap px-2 py-1.5 text-right">
        <button type="button" className={restoreButton} disabled={busy || e.check_status !== "ok"} onClick={onRestore}>
          <RotateCcw size={14} aria-hidden /> {t("restore.open")}
        </button>
      </td>
    </tr>
  );
}

function EntriesCard({ s, tz }: { s: BackupStatus; tz: string | undefined }) {
  const [restoring, setRestoring] = useState<BackupEntry | null>(null);
  const list = useQuery({
    queryKey: ["backup", "list"],
    queryFn: () => apiGet<BackupListResponse>("/backups"),
  });
  const entries = list.data?.data ?? [];
  return (
    <Card title={t("backup.stands")} className="min-w-0">
      {list.error ? (
        <Problem error={list.error} />
      ) : !list.data ? (
        <p className="text-sm text-muted">{t("state.loading")}</p>
      ) : entries.length === 0 ? (
        <p className="text-sm text-muted">
          {list.data.meta.target.state === "unavailable" ? t("backup.standsUnavailable") : t("backup.noStands")}
        </p>
      ) : (
        <div className="overflow-x-auto">
          <table className="w-full text-sm">
            <thead className="text-xs text-muted">
              <tr className="border-b border-line">
                <th className="px-2 py-1 text-left font-medium">{t("backup.col.created")}</th>
                <th className="px-2 py-1 text-left font-medium">{t("backup.col.trigger")}</th>
                <th className="px-2 py-1 text-right font-medium">{t("backup.col.activities")}</th>
                <th className="px-2 py-1 text-right font-medium">{t("backup.col.size")}</th>
                <th className="px-2 py-1 text-left font-medium">{t("backup.col.version")}</th>
                <th className="px-2 py-1 text-left font-medium">{t("backup.col.check")}</th>
                <th className="px-2 py-1" />
              </tr>
            </thead>
            <tbody>
              {entries.map((e) => (
                <EntryRow
                  key={e.id}
                  e={e}
                  tz={tz}
                  latest={e.id === s.last_success?.backup_id}
                  busy={s.running_operation_id !== null}
                  onRestore={() => setRestoring(e)}
                />
              ))}
            </tbody>
          </table>
        </div>
      )}
      {restoring && <RestoreDialog entry={restoring} tz={tz} onClose={() => setRestoring(null)} />}
    </Card>
  );
}

/** Seite „Sicherung“ (#57, RFC-0012): Status, „Jetzt sichern“, Stände im Sicherungsziel. */
export function BackupPage() {
  const queryClient = useQueryClient();
  const tz = useConfig().data?.data.timezone;
  const status = useQuery({
    queryKey: ["backup", "status"],
    queryFn: () => apiGet<BackupStatusResponse>("/backups/status"),
    refetchInterval: (q) => statusPollInterval(q.state.data?.data),
  });
  const s = status.data?.data;
  // Laufender Vorgang laut Status (auch der tägliche), sonst der zuletzt selbst angestoßene.
  const [started, setStarted] = useState<string | null>(null);
  const activeId = s?.running_operation_id ?? started;
  const op = useQuery({
    queryKey: ["backup", "operation", activeId],
    queryFn: () => apiGet<OperationResponse>(`/operations/${activeId}`),
    enabled: activeId !== null,
    refetchInterval: (q) => operationPollInterval(q.state.data?.data),
  });
  const run = useMutation({
    mutationFn: () => apiPost<OperationResponse>("/backups", undefined),
    onSuccess: (data) => {
      queryClient.setQueryData(["backup", "operation", data.data.id], data);
      setStarted(data.data.id);
      void queryClient.invalidateQueries({ queryKey: ["backup", "status"] });
    },
  });

  // Ende des Vorgangs: Status und Liste neu laden.
  const done = finished(op.data?.data);
  useEffect(() => {
    if (done) {
      void queryClient.invalidateQueries({ queryKey: ["backup", "status"] });
      void queryClient.invalidateQueries({ queryKey: ["backup", "list"] });
    }
  }, [done, queryClient]);

  if (status.error) return <Problem error={status.error} />;
  if (!s) return <p className="text-sm text-muted">{t("state.loading")}</p>;
  const shown: BackupStatus = activeId && op.data && !done ? { ...s, running_operation_id: activeId } : s;

  return (
    <div className="grid grid-cols-1 gap-4">
      <StatusCard s={shown} op={op.data?.data} tz={tz} onRun={() => run.mutate()} runError={run.error} />
      <EntriesCard s={s} tz={tz} />
    </div>
  );
}
