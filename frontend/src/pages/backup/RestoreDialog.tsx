import { useState } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";
import { CircleAlert, CircleCheck, TriangleAlert } from "lucide-react";
import { ApiError, apiGet, apiPost, type BackupEntry, type OperationResponse, type RestorePreviewResponse } from "../../api/client";
import { dangerButton, Dialog, secondaryButton } from "../../components/Dialog";
import { Problem, problemText } from "../../components/Problem";
import { hasText, t, type TextKey } from "../../i18n";
import { dateTime, num } from "../../utils/format";
import { finished } from "./backupModel";

/** Grund aus `details.reason` (beschädigter oder zu neuer Stand). */
function reason(error: unknown): string | null {
  return error instanceof ApiError && typeof error.details.reason === "string" ? error.details.reason : null;
}

/** Vorschau, Zustimmung und Ablauf des Wiederherstellens (#57, RFC-0012). */
export function RestoreDialog({ entry, tz, onClose }: { entry: BackupEntry; tz: string | undefined; onClose: () => void }) {
  const [ack, setAck] = useState(false);
  const [opId, setOpId] = useState<string | null>(null);
  const preview = useQuery({
    queryKey: ["backup", "restore-preview", entry.id],
    queryFn: () => apiPost<RestorePreviewResponse>("/restores/preview", { backup_id: entry.id }),
    gcTime: 0,
    retry: false,
    refetchOnWindowFocus: false,
  });
  const start = useMutation({
    mutationFn: (token: string) => apiPost<OperationResponse>("/restores", { preview_token: token, acknowledge_warnings: ack }),
    onSuccess: (data) => setOpId(data.data.id),
  });
  // Während des internen Neustarts schlagen Abfragen fehl; weiter nachsehen, bis der Vorgang beendet ist.
  const op = useQuery({
    queryKey: ["backup", "operation", opId],
    queryFn: () => apiGet<OperationResponse>(`/operations/${opId}`),
    enabled: opId !== null,
    retry: false,
    refetchInterval: (q) => (finished(q.state.data?.data) ? false : 1_000),
  });

  const p = preview.data?.data;
  const o = op.data?.data;
  const running = opId !== null && !finished(o);
  const done = finished(o);
  const ok = done && o?.outcome === "restored";
  const busy = start.isPending || running;

  const footer = done ? (
    ok ? (
      <button type="button" className={dangerButton} onClick={() => window.location.reload()} data-autofocus>
        {t("restore.reload")}
      </button>
    ) : (
      <button type="button" className={secondaryButton} onClick={onClose} data-autofocus>
        {t("restore.close")}
      </button>
    )
  ) : (
    <>
      <button type="button" className={secondaryButton} onClick={onClose} disabled={busy} data-autofocus>
        {t("edit.cancel")}
      </button>
      <button
        type="button"
        className={dangerButton}
        disabled={!p || busy || (p.requires_acknowledgement && !ack)}
        onClick={() => p && start.mutate(p.preview_token)}
      >
        {busy ? t("restore.running") : t("restore.confirm")}
      </button>
    </>
  );

  return (
    <Dialog title={t("restore.title")} onClose={onClose} busy={busy} footer={footer}>
      <div className="grid gap-3 text-sm">
        <p>
          <strong>{entry.created_at ? dateTime(entry.created_at, tz, true) : entry.id}</strong>
          <span className="ml-2 font-mono text-xs text-muted">{entry.id}</span>
        </p>

        {opId !== null ? (
          <Progress phase={o?.phase ?? null} offline={op.isError} done={done} ok={ok} error={o?.error ?? null} status={o?.status ?? null} />
        ) : preview.error ? (
          <div className="grid gap-1">
            <Problem error={preview.error} />
            {reason(preview.error) && <p className="break-words text-xs text-muted">{reason(preview.error)}</p>}
          </div>
        ) : !p ? (
          <p className="text-muted">{t("restore.checking")}</p>
        ) : (
          <>
            <p>{t(`restore.comparison.${p.comparison}` as TextKey)}</p>
            <table className="w-full text-sm">
              <thead className="text-xs text-muted">
                <tr className="border-b border-line">
                  <th className="py-1 text-left font-medium" />
                  <th className="px-2 py-1 text-right font-medium">{t("restore.col.current")}</th>
                  <th className="px-2 py-1 text-right font-medium">{t("restore.col.backup")}</th>
                </tr>
              </thead>
              <tbody>
                <tr className="border-b border-line">
                  <td className="py-1">{t("backup.col.activities")}</td>
                  <td className="num px-2 py-1 text-right">{num(p.current.activity_count)}</td>
                  <td className="num px-2 py-1 text-right">{num(p.backup.activity_count)}</td>
                </tr>
                <tr>
                  <td className="py-1">{t("restore.segments")}</td>
                  <td className="num px-2 py-1 text-right">{num(p.current.segment_count)}</td>
                  <td className="num px-2 py-1 text-right">{num(p.backup.segment_count)}</td>
                </tr>
              </tbody>
            </table>
            {p.warnings.length > 0 && (
              <ul className="grid gap-1">
                {p.warnings.map((w) => (
                  <li key={w.code} className="flex items-start gap-2 text-warn">
                    <TriangleAlert size={16} className="mt-0.5 shrink-0" aria-hidden />
                    {hasText(`restore.warning.${w.code}`) ? t(`restore.warning.${w.code}` as TextKey) : w.message}
                  </li>
                ))}
              </ul>
            )}
            <p className="text-xs text-muted">{t("restore.effect")}</p>
            {p.requires_acknowledgement && (
              <label className="flex items-start gap-2">
                <input type="checkbox" className="mt-0.5" checked={ack} onChange={(e) => setAck(e.target.checked)} />
                <span>{t("restore.acknowledge")}</span>
              </label>
            )}
          </>
        )}
        {start.error != null && <p className="text-danger">{problemText(start.error)}</p>}
      </div>
    </Dialog>
  );
}

function Progress({
  phase,
  offline,
  done,
  ok,
  error,
  status,
}: {
  phase: string | null;
  offline: boolean;
  done: boolean;
  ok: boolean;
  error: { code: string; message: string } | null;
  status: string | null;
}) {
  if (done && ok) {
    return (
      <p className="flex items-center gap-2 text-good" aria-live="polite">
        <CircleCheck size={16} aria-hidden /> {t("restore.done")}
      </p>
    );
  }
  if (done) {
    return (
      <div className="grid gap-1" aria-live="polite">
        <p className="flex items-center gap-2 text-danger">
          <CircleAlert size={16} aria-hidden /> {t(status === "rolled_back" ? "restore.rolledBack" : "restore.failed")}
        </p>
        {error && <p className="break-words text-xs text-muted">{error.message}</p>}
      </div>
    );
  }
  return (
    <div className="grid gap-2" aria-live="polite">
      <div className="h-1.5 overflow-hidden rounded-full bg-card-2" role="progressbar" aria-label={t("restore.running")}>
        <div className="h-full w-full animate-pulse rounded-full bg-accent/40" />
      </div>
      <p>{offline ? t("restore.restarting") : t(phase ? (`restore.phase.${phase}` as TextKey) : "restore.running")}</p>
    </div>
  );
}
