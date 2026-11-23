import { useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { ExternalLink, WandSparkles } from "lucide-react";
import { apiDelete, apiPatch, apiPost, type PlanGoal, type PlanGoalResponse } from "../../api/client";
import { Dialog, primaryButton, secondaryButton } from "../../components/Dialog";
import { useFlash } from "../../components/Flash";
import { Problem } from "../../components/Problem";
import { t, type TextKey } from "../../i18n";
import { SPORT_CLASSES } from "../../utils/sports";
import { goalFormOf, goalInput, goalProblem, type GoalForm } from "./goalModel";

const input = "w-full rounded-lg border border-line bg-card-2 px-2 py-1.5 text-sm text-text disabled:opacity-60";
const CLASSES = SPORT_CLASSES.filter((c) => c !== "unknown");

/**
 * Ziel (Wettkampf, Tour) anlegen oder ändern. „Plan vorschlagen“ speichert offene Änderungen und öffnet die
 * Planungshilfe für dieses Ziel; die vorgeschlagenen Einheiten sind danach normale Einheiten.
 */
export function GoalDialog({
  goal,
  today,
  date,
  sportClass,
  onClose,
  onSuggest,
}: {
  goal: PlanGoal | null;
  today: string;
  /** Vorbelegtes Datum für ein neues Ziel */
  date?: string;
  /** Vorbelegte Sportklasse für ein neues Ziel */
  sportClass?: GoalForm["sportClass"];
  onClose: () => void;
  onSuggest: (goal: PlanGoal) => void;
}) {
  const [form, setForm] = useState<GoalForm>(() => {
    const f = goalFormOf(goal, today, sportClass);
    return date && !goal ? { ...f, date } : f;
  });
  const [initial] = useState(form);
  const set = (patch: Partial<GoalForm>) => setForm((f) => ({ ...f, ...patch }));
  const problem = goalProblem(form);
  const dirty = !goal || JSON.stringify(form) !== JSON.stringify(initial);
  const queryClient = useQueryClient();
  const flash = useFlash();
  const refresh = () => {
    void queryClient.invalidateQueries({ queryKey: ["plan"] });
    void queryClient.invalidateQueries({ queryKey: ["form"] });
  };
  const write = () =>
    goal ? apiPatch<PlanGoalResponse>(`/plan/goals/${goal.id}`, goalInput(form)) : apiPost<PlanGoalResponse>("/plan/goals", goalInput(form));
  const save = useMutation({
    mutationFn: write,
    onSuccess: () => {
      refresh();
      flash(t("plan.goal.saved"));
      onClose();
    },
  });
  const suggest = useMutation({
    mutationFn: async () => (dirty ? (await write()).data : goal!),
    onSuccess: (g) => {
      if (dirty) refresh();
      onSuggest(g);
    },
  });
  const remove = useMutation({
    mutationFn: () => apiDelete(`/plan/goals/${goal!.id}`),
    onSuccess: () => {
      refresh();
      flash(t("plan.goal.deleted"));
      onClose();
    },
  });
  const busy = save.isPending || suggest.isPending || remove.isPending;
  const error = save.error ?? suggest.error ?? remove.error;
  const url = form.url.trim();

  return (
    <Dialog
      title={t(goal ? "plan.goal.edit" : "plan.goal.new")}
      onClose={onClose}
      busy={busy}
      footer={
        <>
          {goal && (
            <button type="button" className={`${secondaryButton} text-danger`} disabled={busy} onClick={() => remove.mutate()}>
              {t("plan.goal.remove")}
            </button>
          )}
          <span className="flex-1" />
          <button
            type="button"
            className={`${secondaryButton} inline-flex items-center gap-1`}
            disabled={busy || problem != null}
            onClick={() => suggest.mutate()}
          >
            <WandSparkles size={14} aria-hidden />
            {t("plan.suggest.open")}
          </button>
          <button type="button" className={secondaryButton} onClick={onClose} disabled={busy}>
            {t("edit.cancel")}
          </button>
          <button type="button" className={primaryButton} disabled={busy || problem != null} onClick={() => save.mutate()}>
            {save.isPending ? t("edit.saving") : t("edit.save")}
          </button>
        </>
      }
    >
      <div className="grid gap-3 text-sm">
        <div className="grid gap-3 sm:grid-cols-2">
          <label className="grid gap-1">
            <span className="text-xs font-medium text-muted">{t("plan.field.date")}</span>
            <input type="date" className={input} value={form.date} disabled={busy} onChange={(e) => set({ date: e.target.value })} />
          </label>
          <label className="grid gap-1">
            <span className="text-xs font-medium text-muted">{t("plan.field.sport")}</span>
            <select className={input} value={form.sportClass} disabled={busy} onChange={(e) => set({ sportClass: e.target.value as GoalForm["sportClass"] })}>
              {CLASSES.map((c) => (
                <option key={c} value={c}>
                  {t(`sport.${c}` as TextKey)}
                </option>
              ))}
            </select>
          </label>
        </div>
        <label className="grid gap-1">
          <span className="text-xs font-medium text-muted">{t("plan.goal.name")}</span>
          <input
            className={input}
            value={form.name}
            maxLength={80}
            placeholder={t("plan.goal.namePlaceholder")}
            disabled={busy}
            onChange={(e) => set({ name: e.target.value })}
          />
        </label>
        <div className="grid gap-3 sm:grid-cols-2">
          <label className="grid gap-1">
            <span className="text-xs font-medium text-muted">{t("plan.goal.distance")}</span>
            <input className={`${input} num`} inputMode="decimal" value={form.distance} disabled={busy} onChange={(e) => set({ distance: e.target.value })} />
          </label>
          <label className="grid gap-1">
            <span className="text-xs font-medium text-muted">{t("plan.goal.targetTime")}</span>
            <input
              className={`${input} num`}
              value={form.targetTime}
              placeholder="h:mm:ss"
              disabled={busy}
              onChange={(e) => set({ targetTime: e.target.value })}
            />
          </label>
        </div>
        <label className="grid gap-1">
          <span className="flex items-center gap-1 text-xs font-medium text-muted">
            {t("plan.goal.url")}
            {url && goalProblem({ ...form, distance: "", targetTime: "" }) !== "url" && (
              <a href={url} target="_blank" rel="noopener noreferrer" className="text-accent hover:underline" aria-label={t("plan.goal.openUrl")}>
                <ExternalLink size={12} aria-hidden />
              </a>
            )}
          </span>
          <input className={input} type="url" value={form.url} placeholder="https://" maxLength={500} disabled={busy} onChange={(e) => set({ url: e.target.value })} />
        </label>
        <label className="grid gap-1">
          <span className="text-xs font-medium text-muted">{t("plan.goal.description")}</span>
          <textarea className={`${input} min-h-16`} rows={3} maxLength={2000} value={form.description} disabled={busy} onChange={(e) => set({ description: e.target.value })} />
        </label>
        {problem && problem !== "name" && <p className="text-xs text-danger">{t(`plan.goal.problem.${problem}` as TextKey)}</p>}
        <p className="text-xs text-muted">{t("plan.goal.hint")}</p>
        {error && <Problem error={error} />}
      </div>
    </Dialog>
  );
}
