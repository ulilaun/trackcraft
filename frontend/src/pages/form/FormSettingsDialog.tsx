import { useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { apiPatch, type LoadSettings, type LoadSettingsResponse, type SportClass } from "../../api/client";
import { Dialog, primaryButton, secondaryButton } from "../../components/Dialog";
import { useFlash } from "../../components/Flash";
import { Problem } from "../../components/Problem";
import { SportIcon } from "../../components/SportIcon";
import { t, type TextKey } from "../../i18n";
import { SPORT_CLASSES } from "../../utils/sports";

const input = "w-24 rounded-lg border border-line bg-card-2 px-2 py-1.5 text-right text-sm text-text";

const whole = (s: string, lo: number, hi: number): number | null => {
  if (!/^\d{1,3}$/.test(s.trim())) return null;
  const v = Number(s);
  return v >= lo && v <= hi ? v : null;
};

/** Zeitkonstanten der Formkurve und Schätzfaktoren für Aktivitäten ohne HF. */
export function FormSettingsDialog({ settings, onClose }: { settings: LoadSettings; onClose: () => void }) {
  const [fitness, setFitness] = useState(String(settings.fitness_days));
  const [fatigue, setFatigue] = useState(String(settings.fatigue_days));
  const [est, setEst] = useState<Record<SportClass, string>>(
    () => Object.fromEntries(SPORT_CLASSES.map((c) => [c, String(settings.estimate_per_hour[c] ?? 0)])) as Record<SportClass, string>,
  );
  const queryClient = useQueryClient();
  const flash = useFlash();

  const f = whole(fitness, 7, 120);
  const g = whole(fatigue, 2, 30);
  const factors = SPORT_CLASSES.map((c) => [c, whole(est[c], 0, 300)] as const);
  const invalid = f == null || g == null || g >= f || factors.some(([, v]) => v == null);

  const save = useMutation({
    mutationFn: () =>
      apiPatch<LoadSettingsResponse>("/form/settings", {
        fitness_days: f,
        fatigue_days: g,
        estimate_per_hour: Object.fromEntries(factors),
      }),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["form"] });
      void queryClient.invalidateQueries({ queryKey: ["activities"] });
      flash(t("form.settings.saved"));
      onClose();
    },
  });
  const busy = save.isPending;

  return (
    <Dialog
      title={t("form.settings")}
      onClose={onClose}
      busy={busy}
      footer={
        <>
          <button type="button" className={secondaryButton} onClick={onClose} disabled={busy}>
            {t("edit.cancel")}
          </button>
          <button type="button" className={primaryButton} disabled={invalid || busy} onClick={() => save.mutate()}>
            {busy ? t("edit.saving") : t("edit.save")}
          </button>
        </>
      }
    >
      <div className="grid gap-4">
        <div className="grid gap-3 sm:grid-cols-2">
          <label className="grid gap-1">
            <span className="text-xs font-medium text-muted">{t("form.settings.fitnessDays")}</span>
            <input className={input} inputMode="numeric" value={fitness} onChange={(e) => setFitness(e.target.value)} />
          </label>
          <label className="grid gap-1">
            <span className="text-xs font-medium text-muted">{t("form.settings.fatigueDays")}</span>
            <input className={input} inputMode="numeric" value={fatigue} onChange={(e) => setFatigue(e.target.value)} />
          </label>
        </div>
        <fieldset className="grid gap-2">
          <legend className="mb-1 text-xs font-semibold uppercase tracking-wide text-muted">{t("form.settings.estimate")}</legend>
          {SPORT_CLASSES.map((c) => (
            <label key={c} className="flex items-center justify-between gap-3 text-sm">
              <span className="flex items-center gap-2">
                <SportIcon sport={c} size={16} decorative />
                {t(`sport.${c}` as TextKey)}
              </span>
              <input
                className={input}
                inputMode="numeric"
                aria-label={t(`sport.${c}` as TextKey)}
                value={est[c]}
                onChange={(e) => setEst((x) => ({ ...x, [c]: e.target.value }))}
              />
            </label>
          ))}
          <p className="text-xs text-muted">{t("form.settings.estimateHint")}</p>
        </fieldset>
        <p className="text-xs text-muted">{t("form.settings.recalc")}</p>
        {invalid && (
          <p className="text-sm text-danger" role="alert">
            {t("form.settings.problem")}
          </p>
        )}
        {save.error && <Problem error={save.error} />}
      </div>
    </Dialog>
  );
}
