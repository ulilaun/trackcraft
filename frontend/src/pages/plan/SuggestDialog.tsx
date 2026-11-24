import { useEffect, useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Lock } from "lucide-react";
import { apiPost, type PlanGoal, type SuggestionApplyResponse, type SuggestionInput, type SuggestionResponse } from "../../api/client";
import { Dialog, primaryButton, secondaryButton } from "../../components/Dialog";
import { useFlash } from "../../components/Flash";
import { Problem } from "../../components/Problem";
import { t, type TextKey } from "../../i18n";
import { addDays, dayDate, dayMonth, num, WEEK, weekdayShort } from "../../utils/format";
import { SPORT_CLASSES } from "../../utils/sports";
import { FormChart } from "../form/FormPage";
import { STATE_CLASS, chartPoints, formState, signed } from "../form/formModel";
import { hm, isoWeek } from "./planModel";
import { DEFAULT_INPUT, PHASE_COLOR, inputProblem, requestInput, weeklyHours } from "./suggestModel";

const CLASSES = SPORT_CLASSES.filter((c) => c !== "unknown");
const input = "w-full rounded-lg border border-line bg-card-2 px-2 py-1.5 text-sm text-text disabled:opacity-60";
const tile = "rounded-lg border border-line bg-card-2 px-3 py-2";

/**
 * Planungshilfe (E8 Stufe 2) für ein Ziel: Eingaben links, Vorschlag rechts. Die Vorschau rechnet bei jeder Änderung
 * neu, „Übernehmen“ trägt die Einheiten als normale Einheiten in den Kalender ein; vorhandene Einheiten bleiben fest.
 */
export function SuggestDialog({ goal, saved, onClose }: { goal: PlanGoal; saved: SuggestionInput | null; onClose: () => void }) {
  const start = saved ?? DEFAULT_INPUT;
  const [base, setBase] = useState<Omit<SuggestionInput, "minutes">>(() => ({
    goal_id: goal.id,
    sport_class: goal.sport_class === "unknown" ? start.sport_class : goal.sport_class,
    max_gain: start.max_gain,
    recovery_weeks: start.recovery_weeks,
    intensity: start.intensity,
  }));
  const [texts, setTexts] = useState<string[]>(() => start.minutes.map(String));
  const problem = inputProblem(texts);
  const req = useMemo(() => requestInput(base, texts), [base, texts]);
  // Kurz warten, damit nicht jeder Tastendruck rechnet
  const [debounced, setDebounced] = useState(req);
  useEffect(() => {
    const timer = setTimeout(() => setDebounced(req), 300);
    return () => clearTimeout(timer);
  }, [req]);

  const preview = useQuery({
    queryKey: ["plan-suggestion", debounced],
    enabled: problem == null,
    queryFn: () => apiPost<SuggestionResponse>("/plan/suggestion", debounced),
    placeholderData: (p) => p,
    staleTime: 0,
    gcTime: 0,
  });
  const queryClient = useQueryClient();
  const flash = useFlash();
  const apply = useMutation({
    mutationFn: () => apiPost<SuggestionApplyResponse>("/plan/suggestion/apply", req),
    onSuccess: (r) => {
      void queryClient.invalidateQueries({ queryKey: ["plan"] });
      void queryClient.invalidateQueries({ queryKey: ["form"] });
      flash(t("plan.suggest.applied", { n: r.data.created }));
      onClose();
    },
  });

  const p = preview.data?.data;
  const stale = preview.isFetching || req !== debounced;
  const busy = apply.isPending;
  const points = p ? chartPoints([], p.forecast) : [];
  const maxWeek = Math.max(1, ...(p?.weeks ?? []).map((w) => w.load));
  const set = (patch: Partial<typeof base>) => setBase((b) => ({ ...b, ...patch }));
  const state = (v: number) => {
    const s = formState(v);
    return <span className={`rounded-full px-2 py-0.5 text-xs font-semibold ${STATE_CLASS[s]}`}>{t(`form.state.${s}` as TextKey)}</span>;
  };

  return (
    <Dialog
      title={t("plan.suggest.title")}
      onClose={onClose}
      busy={busy}
      wide
      footer={
        <>
          <span className="flex-1" />
          <button type="button" className={secondaryButton} onClick={onClose} disabled={busy}>
            {t("edit.cancel")}
          </button>
          <button
            type="button"
            className={primaryButton}
            disabled={busy || problem != null || !p || stale || p.units.length === 0}
            onClick={() => apply.mutate()}
          >
            {t("plan.suggest.apply", { n: p?.units.length ?? 0 })}
          </button>
        </>
      }
    >
      <div className="grid gap-5 md:grid-cols-[minmax(0,18rem)_minmax(0,1fr)]">
        <div className="grid content-start gap-3 text-sm">
          <div className={tile}>
            <div className="text-xs text-muted">{t("plan.suggest.goal")}</div>
            <div className="font-semibold">
              {goal.name} · {dayDate(goal.date)}
            </div>
          </div>
          <label className="grid gap-1">
            <span className="text-xs font-medium text-muted">{t("plan.field.sport")}</span>
            <select className={input} value={base.sport_class} disabled={busy} onChange={(e) => set({ sport_class: e.target.value as SuggestionInput["sport_class"] })}>
              {CLASSES.map((c) => (
                <option key={c} value={c}>
                  {t(`sport.${c}` as TextKey)}
                </option>
              ))}
            </select>
          </label>
          <fieldset className="grid gap-1">
            <legend className="mb-1 text-xs font-medium text-muted">{t("plan.suggest.minutes")}</legend>
            <div className="grid grid-cols-7 gap-1">
              {WEEK.map((i) => (
                <label key={i} className="grid gap-0.5 text-center text-[11px] text-muted">
                  {weekdayShort(i)}
                  <input
                    className={`${input} num px-0 text-center`}
                    inputMode="numeric"
                    aria-label={t("plan.suggest.minutesOf", { day: weekdayShort(i) })}
                    value={texts[i]}
                    disabled={busy}
                    onChange={(e) => setTexts((old) => old.map((v, k) => (k === i ? e.target.value : v)))}
                  />
                </label>
              ))}
            </div>
            <span className="text-xs text-muted">{t("plan.suggest.weekly", { h: num(weeklyHours(texts), 1) })}</span>
          </fieldset>
          <label className="grid gap-1">
            <span className="text-xs font-medium text-muted">{t("plan.suggest.maxGain")}</span>
            <span className="flex items-center gap-2">
              <input
                type="range"
                min={3}
                max={7}
                step={1}
                className="flex-1 accent-[var(--color-accent)]"
                value={base.max_gain}
                disabled={busy}
                onChange={(e) => set({ max_gain: Number(e.target.value) })}
              />
              <b className="num w-4 text-right">{base.max_gain}</b>
            </span>
          </label>
          <label className="flex items-center gap-2">
            <input type="checkbox" checked={base.recovery_weeks} disabled={busy} onChange={(e) => set({ recovery_weeks: e.target.checked })} />
            {t("plan.suggest.recovery")}
          </label>
          <label className="grid gap-1">
            <span className="text-xs font-medium text-muted">{t("plan.suggest.intensity")}</span>
            <select className={input} value={base.intensity} disabled={busy} onChange={(e) => set({ intensity: e.target.value as SuggestionInput["intensity"] })}>
              <option value="polarized">{t("plan.suggest.polarized")}</option>
              <option value="base">{t("plan.suggest.base")}</option>
            </select>
          </label>
          <p className="text-xs text-muted">{t("plan.suggest.rules")}</p>
          {problem && <p className="text-sm text-danger">{t(problem)}</p>}
        </div>

        <div className={`grid min-w-0 content-start gap-3 ${stale ? "opacity-60" : ""}`} aria-busy={stale} data-testid="suggest-preview">
          {preview.error && <Problem error={preview.error} />}
          {apply.error && <Problem error={apply.error} />}
          {p && (
            <>
              <div className="grid grid-cols-2 gap-2 lg:grid-cols-4">
                <div className={tile}>
                  <div className="text-xs text-muted">{t("plan.suggest.formGoal")}</div>
                  <div className="num text-xl font-bold">{signed(p.summary.form_at_goal)}</div>
                  {state(p.summary.form_at_goal)}
                </div>
                <div className={tile}>
                  <div className="text-xs text-muted">{t("plan.suggest.fitness")}</div>
                  <div className="num text-xl font-bold">
                    {num(p.summary.fitness_today)} → {num(p.summary.fitness_at_goal)}
                  </div>
                </div>
                <div className={tile}>
                  <div className="text-xs text-muted">{t("plan.suggest.taper")}</div>
                  <div className="num text-xl font-bold">{t("plan.suggest.days", { n: p.summary.taper_days })}</div>
                </div>
                <div className={tile}>
                  <div className="text-xs text-muted">{t("plan.suggest.units")}</div>
                  <div className="num text-xl font-bold">{p.units.length}</div>
                  {p.fixed_units > 0 && <div className="text-xs text-muted">{t("plan.suggest.fixed", { n: p.fixed_units })}</div>}
                </div>
              </div>

              {p.summary.limited_by_time && p.summary.avg_gain != null && p.summary.avg_gain < p.summary.max_gain - 0.5 && (
                <p className="rounded-lg bg-warn/15 px-3 py-2 text-xs text-text">
                  {t("plan.suggest.limited", { v: num(p.summary.avg_gain, 1), max: p.summary.max_gain })}
                </p>
              )}
              {p.units.length === 0 && <p className="rounded-lg bg-warn/15 px-3 py-2 text-xs text-text">{t("plan.suggest.none")}</p>}
              {(p.summary.form_at_goal <= 5 || p.summary.form_at_goal > 25) && (
                <p className="rounded-lg bg-warn/15 px-3 py-2 text-xs text-text">{t("plan.suggest.notFresh")}</p>
              )}

              <div className="grid gap-1" data-testid="suggest-weeks">
                <div className="flex h-24 items-end gap-1">
                  {p.weeks.map((w) => (
                    <div
                      key={w.start}
                      className="flex-1 rounded-t"
                      style={{ height: `${Math.max(2, (w.load / maxWeek) * 100)}%`, background: w.phase ? PHASE_COLOR[w.phase] : "var(--color-muted)" }}
                      title={`${t("plan.kw", { n: isoWeek(w.start) })}: ${num(w.load)}${w.monotony != null ? ` · ${t("form.monotonyShort")} ${num(w.monotony, 2)}` : ""}`}
                    />
                  ))}
                </div>
                <div className="flex gap-1 text-center text-[10px] text-muted">
                  {p.weeks.map((w) => (
                    <span key={w.start} className={`num flex-1 truncate ${w.monotony != null && w.monotony > 2 ? "font-bold text-danger" : ""}`}>
                      {isoWeek(w.start)}
                    </span>
                  ))}
                </div>
                <div className="flex flex-wrap gap-3 text-[11px] text-muted">
                  {(["build", "recovery", "taper"] as const).map((ph) => (
                    <span key={ph} className="inline-flex items-center gap-1">
                      <i className="inline-block h-2.5 w-2.5 rounded-sm" style={{ background: PHASE_COLOR[ph] }} />
                      {t(`plan.phase.${ph}` as TextKey)}
                    </span>
                  ))}
                </div>
              </div>

              {points.length > 0 && <FormChart points={points} goals={[goal]} />}

              <div className="grid gap-1 text-sm">
                {p.weeks.map((w) => {
                  const end = addDays(w.start, 6);
                  const us = p.units.filter((u) => u.date >= w.start && u.date <= end);
                  if (us.length === 0) return null;
                  return (
                    <details key={w.start} className="rounded-lg border border-line px-3 py-1.5">
                      <summary className="cursor-pointer text-xs">
                        <b>{t("plan.kw", { n: isoWeek(w.start) })}</b> · {w.phase ? t(`plan.phase.${w.phase}` as TextKey) : ""} · {t("plan.load")} {num(w.load)}
                        {w.monotony != null && ` · ${t("form.monotonyShort")} ${num(w.monotony, 2)}`}
                        {w.low_zone_pct != null && ` · Z1–2 ${num(w.low_zone_pct)} %`}
                      </summary>
                      <ul className="mt-1 grid gap-0.5">
                        {us.map((u) => (
                          <li key={u.date} className="grid grid-cols-[4.5rem_minmax(0,1fr)_auto] gap-2 text-xs">
                            <span className="text-muted">{dayMonth(u.date)}</span>
                            <span className="truncate">{u.title}</span>
                            <span className="num text-muted">
                              {hm(u.load.minutes)} · {num(u.load.total)}
                            </span>
                          </li>
                        ))}
                      </ul>
                    </details>
                  );
                })}
                {p.fixed_units > 0 && (
                  <p className="inline-flex items-center gap-1 text-xs text-muted">
                    <Lock size={12} /> {t("plan.suggest.fixedHint")}
                  </p>
                )}
              </div>
            </>
          )}
        </div>
      </div>
    </Dialog>
  );
}
