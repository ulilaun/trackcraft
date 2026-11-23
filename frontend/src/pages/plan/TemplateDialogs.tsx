import { useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  apiDelete,
  apiGet,
  apiPost,
  type PlanResponse,
  type PlanTemplate,
  type PlanTemplateApplyResponse,
  type PlanTemplatesResponse,
  type PlannedUnit,
} from "../../api/client";
import { Dialog, primaryButton, secondaryButton } from "../../components/Dialog";
import { useFlash } from "../../components/Flash";
import { Problem } from "../../components/Problem";
import { SportIcon } from "../../components/SportIcon";
import { t } from "../../i18n";
import { addDays, num } from "../../utils/format";
import { monday } from "../form/formModel";
import { germanDate } from "../seasons/seasonModel";
import { MAX_TEMPLATE_WEEKS, hm, isoWeek, templateDates } from "./planModel";

const WEEKDAYS = ["Mo", "Di", "Mi", "Do", "Fr", "Sa", "So"];
const input = "w-full rounded-lg border border-line bg-card-2 px-2 py-1.5 text-sm text-text disabled:opacity-60";
const weekdayOf = (day: string) => (new Date(`${day}T12:00:00Z`).getUTCDay() + 6) % 7;

function useTemplates() {
  return useQuery({ queryKey: ["plan-templates"], queryFn: () => apiGet<PlanTemplatesResponse>("/plan/templates") });
}

function UnitLine({ weekday, unit }: { weekday: number; unit: Pick<PlannedUnit, "sport_class" | "title" | "load"> }) {
  return (
    <li className="flex items-center gap-2">
      <span className="w-6 text-muted">{WEEKDAYS[weekday]}</span>
      <SportIcon sport={unit.sport_class} size={13} decorative />
      <span className="min-w-0 flex-1 truncate">{unit.title}</span>
      <span className="num text-muted">{hm(unit.load.minutes)}</span>
      <span className="num w-10 text-right text-muted">{num(unit.load.total)}</span>
    </li>
  );
}

/** Woche als Vorlage speichern; ein vorhandener Name (ohne Groß/Klein) ersetzt diese Vorlage. */
export function SaveTemplateDialog({ weekStart, units, onClose }: { weekStart: string; units: PlannedUnit[]; onClose: () => void }) {
  const templates = useTemplates();
  const [name, setName] = useState("");
  const queryClient = useQueryClient();
  const flash = useFlash();
  const existing = templates.data?.data.find((x) => x.name.toLowerCase() === name.trim().toLowerCase());
  const minutes = units.reduce((a, u) => a + u.load.minutes, 0);
  const load = units.reduce((a, u) => a + u.load.total, 0);
  const save = useMutation({
    mutationFn: () => apiPost("/plan/templates", { name: name.trim(), week_start: weekStart, replace: !!existing }),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["plan-templates"] });
      flash(t(existing ? "plan.template.replaced" : "plan.template.saved", { name: name.trim() }));
      onClose();
    },
  });
  return (
    <Dialog
      title={t("plan.template.saveTitle", { n: isoWeek(weekStart) })}
      onClose={onClose}
      busy={save.isPending}
      footer={
        <>
          <span className="flex-1" />
          <button type="button" className={secondaryButton} onClick={onClose} disabled={save.isPending}>
            {t("edit.cancel")}
          </button>
          <button type="button" className={primaryButton} disabled={save.isPending || !name.trim()} onClick={() => save.mutate()}>
            {t(existing ? "plan.template.replace" : "plan.template.save")}
          </button>
        </>
      }
    >
      <div className="grid gap-3 text-sm">
        <label className="grid gap-1">
          <span className="text-xs font-medium text-muted">{t("plan.template.name")}</span>
          <input
            className={input}
            value={name}
            maxLength={80}
            list="plan-template-names"
            placeholder={t("plan.template.namePlaceholder")}
            onChange={(e) => setName(e.target.value)}
          />
          <datalist id="plan-template-names">
            {templates.data?.data.map((x) => <option key={x.id} value={x.name} />)}
          </datalist>
        </label>
        {existing && <p className="text-xs text-warn">{t("plan.template.exists", { name: existing.name })}</p>}
        <div className="grid gap-1 rounded-lg bg-card-2 px-3 py-2">
          <span className="text-xs text-muted">
            {t("plan.template.summary", { n: units.length, time: hm(minutes), load: num(load) })}
          </span>
          <ul className="grid gap-0.5 text-xs">
            {units.map((u) => (
              <UnitLine key={u.id} weekday={weekdayOf(u.date)} unit={u} />
            ))}
          </ul>
        </div>
        <p className="text-xs text-muted">{t("plan.template.saveHint")}</p>
        {save.error && <Problem error={save.error} />}
      </div>
    </Dialog>
  );
}

/** Vorlage auf eine oder mehrere Wochen anwenden; vorhandene Einheiten bleiben, Tage vor heute entfallen. */
export function ApplyTemplateDialog({ today, onClose }: { today: string; onClose: () => void }) {
  const templates = useTemplates();
  const list = templates.data?.data ?? [];
  const [chosen, setChosen] = useState<string | null>(null);
  const tpl: PlanTemplate | undefined = list.find((x) => x.id === chosen) ?? list[0];
  const thisWeek = monday(today);
  const [weekStart, setWeekStart] = useState(addDays(thisWeek, 7));
  const [weeksText, setWeeksText] = useState("4");
  const weeks = /^\d{1,2}$/.test(weeksText.trim()) ? Number(weeksText.trim()) : NaN;
  const weeksOk = weeks >= 1 && weeks <= MAX_TEMPLATE_WEEKS;
  const n = weeksOk ? weeks : 1;
  const queryClient = useQueryClient();
  const flash = useFlash();
  const mondays = useMemo(() => Array.from({ length: 27 }, (_, i) => addDays(thisWeek, i * 7)), [thisWeek]);

  const rangeEnd = addDays(weekStart, n * 7 - 1);
  const existing = useQuery({
    queryKey: ["plan", weekStart, rangeEnd],
    queryFn: () => apiGet<PlanResponse>(`/plan?from=${weekStart}&to=${rangeEnd}`),
    placeholderData: (p) => p,
  });
  const dates = tpl ? templateDates(tpl.units.map((u) => u.weekday), weekStart, n, today) : [];
  const perWeek = Array.from({ length: n }, (_, w) => {
    const from = addDays(weekStart, w * 7);
    const to = addDays(from, 6);
    return {
      from,
      created: dates.filter((d) => d.week === w).length,
      existing: (existing.data?.data.units ?? []).filter((u) => u.date >= from && u.date <= to).length,
    };
  });

  const apply = useMutation({
    mutationFn: () => apiPost<PlanTemplateApplyResponse>(`/plan/templates/${tpl!.id}/apply`, { week_start: weekStart, weeks }),
    onSuccess: (r) => {
      void queryClient.invalidateQueries({ queryKey: ["plan"] });
      void queryClient.invalidateQueries({ queryKey: ["form"] });
      flash(t("plan.template.applied", { n: r.data.created }));
      onClose();
    },
  });
  const remove = useMutation({
    mutationFn: () => apiDelete(`/plan/templates/${tpl!.id}`),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["plan-templates"] });
      flash(t("plan.template.deleted", { name: tpl!.name }));
      setChosen(null);
    },
  });
  const busy = apply.isPending || remove.isPending;

  return (
    <Dialog
      title={t("plan.template.applyTitle")}
      onClose={onClose}
      busy={busy}
      footer={
        <>
          {tpl && (
            <button type="button" className={`${secondaryButton} text-danger`} disabled={busy} onClick={() => remove.mutate()}>
              {t("plan.template.delete")}
            </button>
          )}
          <span className="flex-1" />
          <button type="button" className={secondaryButton} onClick={onClose} disabled={busy}>
            {t("edit.cancel")}
          </button>
          <button type="button" className={primaryButton} disabled={busy || !tpl || !weeksOk || dates.length === 0} onClick={() => apply.mutate()}>
            {t("plan.template.applyButton", { n: dates.length })}
          </button>
        </>
      }
    >
      <div className="grid gap-3 text-sm">
        {templates.error && <Problem error={templates.error} />}
        {templates.data && list.length === 0 && <p className="text-muted">{t("plan.template.none")}</p>}
        {tpl && (
          <>
            <div className="grid gap-3 sm:grid-cols-[minmax(0,1fr)_auto_auto]">
              <label className="grid gap-1">
                <span className="text-xs font-medium text-muted">{t("plan.template.template")}</span>
                <select className={input} value={tpl.id} disabled={busy} onChange={(e) => setChosen(e.target.value)}>
                  {list.map((x) => (
                    <option key={x.id} value={x.id}>
                      {x.name}
                    </option>
                  ))}
                </select>
              </label>
              <label className="grid gap-1">
                <span className="text-xs font-medium text-muted">{t("plan.template.fromWeek")}</span>
                <select className={input} value={weekStart} disabled={busy} onChange={(e) => setWeekStart(e.target.value)}>
                  {mondays.map((m) => (
                    <option key={m} value={m}>
                      {t("plan.template.weekOption", { n: isoWeek(m), date: germanDate(m).slice(0, 6) })}
                    </option>
                  ))}
                </select>
              </label>
              <label className="grid gap-1">
                <span className="text-xs font-medium text-muted">{t("plan.template.weeks")}</span>
                <input
                  className={`${input} num w-20`}
                  inputMode="numeric"
                  aria-label={t("plan.template.weeks")}
                  value={weeksText}
                  disabled={busy}
                  onChange={(e) => setWeeksText(e.target.value)}
                />
              </label>
            </div>
            {!weeksOk && <p className="text-xs text-danger">{t("plan.template.weeksProblem", { n: MAX_TEMPLATE_WEEKS })}</p>}
            <div className="grid gap-1 rounded-lg bg-card-2 px-3 py-2">
              <span className="text-xs text-muted">{t("plan.template.summary", { n: tpl.units.length, time: hm(tpl.minutes), load: num(tpl.load) })}</span>
              <ul className="grid gap-0.5 text-xs">
                {tpl.units.map((u, i) => (
                  <UnitLine key={i} weekday={u.weekday} unit={u} />
                ))}
              </ul>
            </div>
            <div className="grid grid-cols-[repeat(auto-fill,minmax(7.5rem,1fr))] gap-2" data-testid="plan-template-weeks">
              {perWeek.map((w) => (
                <div key={w.from} className="rounded-lg border border-line px-2 py-1.5 text-xs">
                  <b className="block">{t("plan.kw", { n: isoWeek(w.from) })}</b>
                  <span className="block text-accent">{t("plan.template.new", { n: w.created })}</span>
                  {w.existing > 0 && <span className="block text-muted">{t("plan.template.existing", { n: w.existing })}</span>}
                </div>
              ))}
            </div>
            <p className="text-xs text-muted">{t("plan.template.applyHint")}</p>
          </>
        )}
        {(apply.error ?? remove.error) && <Problem error={apply.error ?? remove.error} />}
      </div>
    </Dialog>
  );
}
