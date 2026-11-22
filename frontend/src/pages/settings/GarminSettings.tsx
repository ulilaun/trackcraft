import { useEffect, useRef, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link } from "react-router-dom";
import { KeyRound, RefreshCw, RotateCcw, Trash2 } from "lucide-react";
import {
  apiGet,
  apiPatch,
  apiPost,
  type GarminActivityListResponse,
  type GarminSettingsUpdate,
  type GarminStatus,
  type GarminStatusResponse,
} from "../../api/client";
import { Card } from "../../components/Card";
import { Problem, problemText } from "../../components/Problem";
import { useConfig } from "../../hooks/useConfig";
import { t, type TextKey } from "../../i18n";
import { dateOnly, dateTime, num } from "../../utils/format";
import { finishedRun, garminState, intervalOptions, pollInterval, UNSETTLED, unsettledCount } from "./garminModel";

const input = "rounded-lg border border-line bg-card-2 px-2 py-1 text-sm text-text disabled:opacity-60";
const button =
  "inline-flex items-center gap-1.5 rounded-lg border border-line bg-card-2 px-3 py-1.5 text-sm text-text hover:bg-card disabled:opacity-50";
const primary = "inline-flex items-center gap-1.5 rounded-lg bg-accent-strong px-3 py-1.5 text-sm font-medium text-white hover:opacity-90 disabled:opacity-50";

const DOT: Record<ReturnType<typeof garminState>, string> = {
  unconfigured: "bg-muted",
  never: "bg-muted",
  running: "bg-accent animate-pulse",
  error: "bg-danger",
  ok: "bg-good",
};

function StatusLine({ s, tz }: { s: GarminStatus; tz: string | undefined }) {
  const state = garminState(s);
  return (
    <div className="grid gap-1 text-sm">
      <p className="flex items-center gap-2 font-medium">
        <span className={`inline-block h-2.5 w-2.5 shrink-0 rounded-full ${DOT[state]}`} aria-hidden />
        {state === "unconfigured" ? t("garmin.notConnected") : t("garmin.connected", { date: dateOnly(s.key_set_at, tz, true) })}
      </p>
      {s.configured && (
        <p className="text-muted">
          {s.last_run_started_at
            ? t("garmin.lastRun", { time: dateTime(s.last_run_finished_at ?? s.last_run_started_at, tz, true) })
            : t("garmin.neverRun")}
          {s.last_success_at && state === "error" && ` · ${t("garmin.lastSuccess", { time: dateTime(s.last_success_at, tz, true) })}`}
        </p>
      )}
      {s.last_error && !s.running && <p className="text-danger">{s.last_error.message}</p>}
    </div>
  );
}

function Progress({ s }: { s: GarminStatus }) {
  const p = s.progress;
  const total = s.counts;
  // Gesamtzahl steht erst nach dem Lesen der Liste fest; bis dahin ein pulsierender Balken ohne Anteil.
  const known = p.total > 0;
  const pct = known ? Math.min(100, Math.round((p.processed / p.total) * 100)) : 0;
  return (
    <div className="grid gap-1 text-sm" aria-live="polite">
      {s.running && (
        <>
          <div
            className="h-1.5 overflow-hidden rounded-full bg-card-2"
            role="progressbar"
            aria-label={t("garmin.running")}
            aria-valuemin={known ? 0 : undefined}
            aria-valuemax={known ? 100 : undefined}
            aria-valuenow={known ? pct : undefined}
          >
            {known ? (
              <div className="h-full rounded-full bg-accent transition-[width]" style={{ width: `${pct}%` }} />
            ) : (
              <div className="h-full w-full animate-pulse rounded-full bg-accent/40" />
            )}
          </div>
          <p>
            {t(s.backfill_done ? "garmin.runningNew" : "garmin.runningBackfill")} ·{" "}
            {t("garmin.progress", { processed: num(p.processed), total: num(p.total), imported: num(p.imported), linked: num(p.linked) })}
            {p.phase && ` · ${t(`garmin.phase.${p.phase}` as TextKey)}`}
          </p>
        </>
      )}
      <p className="text-muted">
        {t("garmin.totals", { imported: num(total.imported), linked: num(total.linked), unsettled: num(unsettledCount(s)) })}
        {!s.backfill_done && s.configured && !s.running && ` · ${t("garmin.backfillPending")}`}
      </p>
    </div>
  );
}

/** Der Aufrufer setzt `key` aus Schlüsselzustand und -datum: nach Speichern oder Entfernen beginnt das Feld neu. */
function KeyField({ s, busy, onSave }: { s: GarminStatus; busy: boolean; onSave: (u: GarminSettingsUpdate) => void }) {
  const [editing, setEditing] = useState(!s.configured);
  const [key, setKey] = useState("");
  const [confirmRemove, setConfirmRemove] = useState(false);

  if (!editing) {
    return (
      <div className="flex flex-wrap items-center gap-2 text-sm">
        <KeyRound size={16} className="text-muted" aria-hidden />
        <span className="font-mono text-muted">••••••••••</span>
        <button type="button" className={button} disabled={busy} onClick={() => setEditing(true)}>
          {t("garmin.replaceKey")}
        </button>
        {confirmRemove ? (
          <>
            <span className="text-warn">{t("garmin.removeConfirm")}</span>
            <button type="button" className={button} disabled={busy} onClick={() => onSave({ api_key: null, enabled: false })}>
              <Trash2 size={14} aria-hidden /> {t("garmin.removeKey")}
            </button>
            <button type="button" className={button} onClick={() => setConfirmRemove(false)}>
              {t("sports.cancel")}
            </button>
          </>
        ) : (
          <button type="button" className={button} disabled={busy || s.running} onClick={() => setConfirmRemove(true)}>
            <Trash2 size={14} aria-hidden /> {t("garmin.removeKey")}
          </button>
        )}
      </div>
    );
  }
  return (
    <form
      className="grid gap-2"
      onSubmit={(e) => {
        e.preventDefault();
        if (key.trim()) onSave({ api_key: key.trim() });
      }}
    >
      <label className="grid gap-1 text-sm">
        <span>{t("garmin.key")}</span>
        <input
          className={`${input} font-mono`}
          type="password"
          autoComplete="off"
          spellCheck={false}
          value={key}
          disabled={busy}
          onChange={(e) => setKey(e.target.value)}
        />
      </label>
      <p className="text-xs text-muted">{t("garmin.keyHint")}</p>
      <div className="flex flex-wrap gap-2">
        <button type="submit" className={primary} disabled={busy || key.trim().length < 8}>
          {busy ? t("garmin.checking") : t("garmin.saveKey")}
        </button>
        {s.configured && (
          <button type="button" className={button} onClick={() => setEditing(false)}>
            {t("sports.cancel")}
          </button>
        )}
      </div>
    </form>
  );
}

function Unsettled({ s, tz }: { s: GarminStatus; tz: string | undefined }) {
  const queryClient = useQueryClient();
  const [open, setOpen] = useState(false);
  const count = unsettledCount(s);
  const list = useQuery({
    queryKey: ["garmin", "unsettled", count],
    queryFn: () => apiGet<GarminActivityListResponse>(`/garmin/activities?status=${UNSETTLED.join(",")}&limit=200`),
    enabled: open && count > 0,
  });
  const retry = useMutation({
    mutationFn: () => apiPost("/garmin/activities/retry", undefined),
    onSuccess: () => void queryClient.invalidateQueries({ queryKey: ["garmin"] }),
  });
  if (count === 0) return null;
  return (
    <details className="text-sm" open={open} onToggle={(e) => setOpen(e.currentTarget.open)}>
      <summary className="cursor-pointer select-none">
        {t("garmin.unsettled", { count: num(count) })}
      </summary>
      <div className="mt-2 grid gap-2">
        <p className="text-xs text-muted">{t("garmin.unsettledHint")}</p>
        {list.error && <Problem error={list.error} />}
        {list.data && (
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead className="text-xs text-muted">
                <tr className="border-b border-line">
                  <th className="px-2 py-1 text-left font-medium">{t("garmin.col.date")}</th>
                  <th className="px-2 py-1 text-left font-medium">{t("garmin.col.type")}</th>
                  <th className="px-2 py-1 text-left font-medium">{t("garmin.col.status")}</th>
                  <th className="px-2 py-1 text-left font-medium">{t("garmin.col.reason")}</th>
                </tr>
              </thead>
              <tbody>
                {list.data.data.map((a) => (
                  <tr key={a.garmin_id} className="border-b border-line last:border-0 align-top">
                    <td className="num whitespace-nowrap px-2 py-1">{dateTime(a.start_time, tz, true)}</td>
                    <td className="px-2 py-1 font-mono text-xs">{a.type_key ?? "–"}</td>
                    <td className="whitespace-nowrap px-2 py-1">{t(`garmin.status.${a.status}` as TextKey)}</td>
                    <td className="px-2 py-1 text-muted">{a.reason ?? a.reason_code ?? "–"}</td>
                  </tr>
                ))}
              </tbody>
            </table>
            {list.data.meta.total > list.data.data.length && (
              <p className="mt-1 text-xs text-muted">{t("garmin.more", { count: num(list.data.meta.total - list.data.data.length) })}</p>
            )}
          </div>
        )}
        <div>
          <button type="button" className={button} disabled={retry.isPending || s.running} onClick={() => retry.mutate()}>
            <RotateCcw size={14} aria-hidden /> {t("garmin.retry")}
          </button>
          {retry.error && <Problem error={retry.error} />}
        </div>
      </div>
    </details>
  );
}

/** Abschnitt „Garmin Connect“ der Einstellungen (Uli 14.11.2026). */
export function GarminSettings() {
  const queryClient = useQueryClient();
  const tz = useConfig().data?.data.timezone;
  // Angestoßener Lauf: Zeitpunkt und der Laufbeginn davor; ein neuer Laufbeginn zeigt, dass er gestartet ist.
  const [requested, setRequested] = useState<{ at: number; before: string | null } | null>(null);
  const status = useQuery({
    queryKey: ["garmin", "status"],
    queryFn: () => apiGet<GarminStatusResponse>("/garmin"),
    refetchInterval: (q) => pollInterval(q.state.data?.data, requested?.at ?? null, Date.now()),
  });
  const s = status.data?.data;

  // Nach dem Ende eines Laufs alles andere neu laden (Liste, Dashboard, Sportarten).
  const previous = useRef<GarminStatus | undefined>(undefined);
  useEffect(() => {
    if (finishedRun(previous.current, s)) {
      void queryClient.invalidateQueries({ predicate: (q) => q.queryKey[0] !== "garmin" });
      void queryClient.invalidateQueries({ queryKey: ["garmin", "unsettled"] });
    }
    previous.current = s;
  }, [s, queryClient]);

  const save = useMutation({
    mutationFn: (u: GarminSettingsUpdate) => apiPatch<GarminStatusResponse>("/garmin", u),
    onSuccess: (data) => queryClient.setQueryData(["garmin", "status"], data),
  });
  const sync = useMutation({
    mutationFn: () => apiPost<GarminStatusResponse>("/garmin/sync", undefined),
    onSuccess: (data) => {
      setRequested({ at: Date.now(), before: data.data.last_run_started_at });
      queryClient.setQueryData(["garmin", "status"], data);
    },
  });

  if (status.error) return <Problem error={status.error} />;
  if (!s) return <p className="text-sm text-muted">{t("state.loading")}</p>;
  const busy = save.isPending;
  const starting = requested !== null && !s.running && !s.last_error && s.last_run_started_at === requested.before;

  return (
    <Card className="min-w-0">
      <div className="grid gap-4">
        <div className="flex flex-wrap items-start justify-between gap-3">
          <StatusLine s={s} tz={tz} />
          {s.configured && (
            <button type="button" className={primary} disabled={s.running || starting || sync.isPending} onClick={() => sync.mutate()}>
              <RefreshCw size={14} className={s.running || starting ? "animate-spin" : ""} aria-hidden />
              {s.running || starting ? t("garmin.running") : t("garmin.syncNow")}
            </button>
          )}
        </div>
        {sync.error && <p className="text-sm text-danger">{problemText(sync.error)}</p>}

        {s.configured && <Progress s={s} />}

        {s.configured && (
          <div className="grid gap-3 text-sm sm:grid-cols-2">
            <label className="flex items-center gap-2">
              <input
                type="checkbox"
                checked={s.enabled}
                disabled={busy}
                onChange={(e) => save.mutate({ enabled: e.target.checked })}
              />
              <span>{t("garmin.auto")}</span>
              <select
                className={input}
                aria-label={t("garmin.interval")}
                value={s.interval_minutes}
                disabled={busy}
                onChange={(e) => save.mutate({ interval_minutes: Number(e.target.value) })}
              >
                {intervalOptions(s.interval_minutes).map((m) => (
                  <option key={m} value={m}>
                    {m < 60 ? t("garmin.minutes", { n: m }) : t("garmin.hours", { n: m / 60 })}
                  </option>
                ))}
              </select>
            </label>
            <label className="flex flex-wrap items-center gap-2">
              <span>{t("garmin.startDate")}</span>
              <input
                className={input}
                type="date"
                value={s.start_date ?? ""}
                disabled={busy || s.running}
                onChange={(e) => save.mutate({ start_date: e.target.value || null })}
              />
              <span className="text-xs text-muted">{t("garmin.startDateHint")}</span>
            </label>
          </div>
        )}

        <KeyField key={`${s.configured}-${s.key_set_at}`} s={s} busy={busy} onSave={(u) => save.mutate(u)} />
        {save.error && <p className="text-sm text-danger">{problemText(save.error)}</p>}

        <Unsettled s={s} tz={tz} />
        {s.counts.imported + s.counts.linked > 0 && (
          <p className="text-xs text-muted">
            <Link className="underline" to="/aktivitaeten">
              {t("garmin.toActivities")}
            </Link>
          </p>
        )}
      </div>
    </Card>
  );
}
