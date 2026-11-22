import { useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { apiPatch, type CurveModel, type LoadSettings, type LoadSettingsResponse, type SportClass } from "../../api/client";
import { Dialog, primaryButton, secondaryButton } from "../../components/Dialog";
import { useFlash } from "../../components/Flash";
import { Problem } from "../../components/Problem";
import { SportIcon } from "../../components/SportIcon";
import { t, type TextKey } from "../../i18n";
import { SPORT_CLASSES } from "../../utils/sports";

const input = "w-24 rounded-lg border border-line bg-card-2 px-2 py-1.5 text-right text-sm text-text disabled:opacity-50";
const legend = "mb-1 text-xs font-semibold uppercase tracking-wide text-muted";
const MODELS: CurveModel[] = ["standard", "two_curve"];

const whole = (s: string, lo: number, hi: number): number | null => {
  if (!/^\d{1,3}$/.test(s.trim())) return null;
  const v = Number(s);
  return v >= lo && v <= hi ? v : null;
};

function NumberField({ label, value, onChange, disabled }: { label: string; value: string; onChange: (v: string) => void; disabled?: boolean }) {
  return (
    <label className="grid gap-1">
      <span className="text-xs font-medium text-muted">{label}</span>
      <input className={input} inputMode="numeric" value={value} disabled={disabled} onChange={(e) => onChange(e.target.value)} />
    </label>
  );
}

/** Modell und Zeitkonstanten der Formkurve, Foster-Aufschlag und Schätzfaktoren für Aktivitäten ohne HF. */
export function FormSettingsDialog({ settings, onClose }: { settings: LoadSettings; onClose: () => void }) {
  const [model, setModel] = useState<CurveModel>(settings.curve_model);
  const [fitness, setFitness] = useState(String(settings.fitness_days));
  const [fatigue, setFatigue] = useState(String(settings.fatigue_days));
  const [aerobic, setAerobic] = useState(String(settings.aerobic_days));
  const [anaerobic, setAnaerobic] = useState(String(settings.anaerobic_days));
  const [bonus, setBonus] = useState(String(settings.rest_bonus_pct));
  const [surcharge, setSurcharge] = useState(settings.foster_surcharge);
  const [surchargePct, setSurchargePct] = useState(String(settings.foster_surcharge_pct));
  const [est, setEst] = useState<Record<SportClass, string>>(
    () => Object.fromEntries(SPORT_CLASSES.map((c) => [c, String(settings.estimate_per_hour[c] ?? 0)])) as Record<SportClass, string>,
  );
  const queryClient = useQueryClient();
  const flash = useFlash();

  const f = whole(fitness, 7, 120);
  const g = whole(fatigue, 2, 30);
  const a = whole(aerobic, 2, 30);
  const b = whole(anaerobic, 2, 30);
  const r = whole(bonus, 0, 50);
  const p = whole(surchargePct, 1, 50);
  const factors = SPORT_CLASSES.map((c) => [c, whole(est[c], 0, 300)] as const);
  const invalid =
    f == null ||
    g == null ||
    g >= f ||
    a == null ||
    b == null ||
    Math.max(a, b) >= f ||
    r == null ||
    p == null ||
    factors.some(([, v]) => v == null);

  const save = useMutation({
    mutationFn: () =>
      apiPatch<LoadSettingsResponse>("/form/settings", {
        curve_model: model,
        fitness_days: f,
        fatigue_days: g,
        aerobic_days: a,
        anaerobic_days: b,
        rest_bonus_pct: r,
        foster_surcharge: surcharge,
        foster_surcharge_pct: p,
        estimate_per_hour: Object.fromEntries(factors),
      }),
    onSuccess: (res) => {
      queryClient.setQueryData(["form-settings"], res);
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
      <div className="grid gap-5">
        <fieldset className="grid gap-2">
          <legend className={legend}>{t("form.settings.model")}</legend>
          <div className="flex flex-wrap gap-2" role="radiogroup" aria-label={t("form.settings.model")}>
            {MODELS.map((m) => (
              <button
                key={m}
                type="button"
                role="radio"
                aria-checked={model === m}
                onClick={() => setModel(m)}
                className={`rounded-full border px-3 py-1 text-sm ${model === m ? "border-transparent bg-accent-strong text-white" : "border-line bg-card-2 text-muted hover:text-text"}`}
              >
                {t(`form.model.${m}` as TextKey)}
              </button>
            ))}
          </div>
          <p className="text-xs text-muted">{t(`form.settings.modelHint.${model}` as TextKey)}</p>
          <div className="grid gap-3 sm:grid-cols-2">
            <NumberField label={t("form.settings.fitnessDays")} value={fitness} onChange={setFitness} />
            {model === "standard" ? (
              <NumberField label={t("form.settings.fatigueDays")} value={fatigue} onChange={setFatigue} />
            ) : (
              <>
                <NumberField label={t("form.settings.aerobicDays")} value={aerobic} onChange={setAerobic} />
                <NumberField label={t("form.settings.anaerobicDays")} value={anaerobic} onChange={setAnaerobic} />
                <NumberField label={t("form.settings.restBonus")} value={bonus} onChange={setBonus} />
              </>
            )}
          </div>
        </fieldset>

        <fieldset className="grid gap-2">
          <legend className={legend}>{t("form.settings.foster")}</legend>
          <label className="flex items-center gap-2 text-sm">
            <input type="checkbox" checked={surcharge} onChange={(e) => setSurcharge(e.target.checked)} />
            {t("form.settings.fosterSurcharge")}
          </label>
          <NumberField label={t("form.settings.fosterPct")} value={surchargePct} onChange={setSurchargePct} disabled={!surcharge} />
        </fieldset>

        <fieldset className="grid gap-2">
          <legend className={legend}>{t("form.settings.estimate")}</legend>
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
