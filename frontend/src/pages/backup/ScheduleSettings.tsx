import { useMutation, useQueryClient } from "@tanstack/react-query";
import { apiPatch, type BackupSettingsUpdate, type BackupStatus, type BackupStatusResponse } from "../../api/client";
import { problemText } from "../../components/Problem";
import { t } from "../../i18n";

const input = "rounded-lg border border-line bg-card-2 px-2 py-1 text-sm text-text disabled:opacity-60";

/** Uhrzeit des täglichen Versuchs und Zahl der aufbewahrten Stände (Uli 25.11.2026); speichert bei jeder Änderung. */
export function ScheduleSettings({ s }: { s: BackupStatus }) {
  const queryClient = useQueryClient();
  const save = useMutation({
    mutationFn: (u: BackupSettingsUpdate) => apiPatch<BackupStatusResponse>("/backups/settings", u),
    onSuccess: (data) => queryClient.setQueryData(["backup", "status"], data),
  });
  const busy = save.isPending;
  const custom = s.settings.daily_time !== null || s.settings.keep !== null;
  const counts = Array.from({ length: s.settings.max_keep }, (_, i) => i + 1);

  return (
    <div className="grid gap-2">
      <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
        <label className="flex items-center gap-2">
          <span>{t("backup.dailyAt")}</span>
          <input
            className={input}
            type="time"
            aria-label={t("backup.dailyTime")}
            value={s.daily_time}
            disabled={busy}
            onChange={(e) => e.target.value && save.mutate({ daily_time: e.target.value })}
          />
        </label>
        <label className="flex items-center gap-2">
          <span>{t("backup.keepLabel")}</span>
          <select
            className={input}
            aria-label={t("backup.keepCount")}
            value={s.keep}
            disabled={busy}
            onChange={(e) => save.mutate({ keep: Number(e.target.value) })}
          >
            {counts.map((n) => (
              <option key={n} value={n}>
                {n}
              </option>
            ))}
          </select>
          <span>{t("backup.keepUnit")}</span>
        </label>
        {custom && (
          <button
            type="button"
            className="text-xs text-accent hover:underline disabled:opacity-50"
            disabled={busy}
            onClick={() => save.mutate({ daily_time: null, keep: null })}
          >
            {t("backup.resetDefaults", { time: s.settings.default_daily_time, keep: s.settings.default_keep })}
          </button>
        )}
      </div>
      <p className="text-xs text-muted">{t("backup.scheduleHint")}</p>
      {save.error != null && <p className="text-danger">{problemText(save.error)}</p>}
    </div>
  );
}
