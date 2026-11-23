import { useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link } from "react-router-dom";
import { ChevronLeft, ChevronRight, Flag, Plus } from "lucide-react";
import {
  apiDelete,
  apiGet,
  apiPut,
  type ActivityListResponse,
  type FormResponse,
  type HrZonesResponse,
  type PlanGoal,
  type PlanResponse,
  type PlannedUnit,
} from "../../api/client";
import { Card } from "../../components/Card";
import { Dialog, primaryButton, secondaryButton } from "../../components/Dialog";
import { Problem } from "../../components/Problem";
import { SportIcon } from "../../components/SportIcon";
import { useConfig } from "../../hooks/useConfig";
import { t, type TextKey } from "../../i18n";
import { addDays, localDate, num } from "../../utils/format";
import { FormChart, LoadChart } from "../form/FormPage";
import { STATE_CLASS, chartPoints, formState, signed } from "../form/formModel";
import { germanDate } from "../seasons/seasonModel";
import { UnitDialog } from "./UnitDialog";
import { WEEKS, defaultStart, hm, isoWeek, weekDays, weekMonotony } from "./planModel";

const WEEKDAYS = ["Mo", "Di", "Mi", "Do", "Fr", "Sa", "So"];
const BORDER: Record<PlannedUnit["status"], string> = {
  planned: "border-l-accent",
  done: "border-l-good",
  skipped: "border-l-muted opacity-60 line-through",
};
const ZONE_COLORS = ["var(--color-z1, #94a3b8)", "var(--color-z2, #22c55e)", "var(--color-z3, #eab308)", "var(--color-z4, #f97316)", "var(--color-z5, #ef4444)"];
const small = "inline-flex items-center gap-1 rounded-lg border border-line bg-card-2 px-2.5 py-1 text-sm hover:border-accent disabled:opacity-50";

function GoalDialog({ goal, today, onClose }: { goal: PlanGoal | null; today: string; onClose: () => void }) {
  const [date, setDate] = useState(goal?.date ?? addDays(today, 28));
  const [name, setName] = useState(goal?.name ?? "");
  const queryClient = useQueryClient();
  const done = () => {
    void queryClient.invalidateQueries({ queryKey: ["plan"] });
    void queryClient.invalidateQueries({ queryKey: ["form"] });
    onClose();
  };
  const save = useMutation({ mutationFn: () => apiPut("/plan/goal", { date, name: name.trim() }), onSuccess: done });
  const remove = useMutation({ mutationFn: () => apiDelete("/plan/goal"), onSuccess: done });
  const busy = save.isPending || remove.isPending;
  const input = "w-full rounded-lg border border-line bg-card-2 px-2 py-1.5 text-sm text-text";
  return (
    <Dialog
      title={t("plan.goal.title")}
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
          <button type="button" className={secondaryButton} onClick={onClose} disabled={busy}>
            {t("edit.cancel")}
          </button>
          <button type="button" className={primaryButton} disabled={busy || !name.trim() || !date} onClick={() => save.mutate()}>
            {t("edit.save")}
          </button>
        </>
      }
    >
      <div className="grid gap-3 text-sm">
        <label className="grid gap-1">
          <span className="text-xs font-medium text-muted">{t("plan.goal.name")}</span>
          <input className={input} value={name} maxLength={80} placeholder={t("plan.goal.namePlaceholder")} onChange={(e) => setName(e.target.value)} />
        </label>
        <label className="grid gap-1">
          <span className="text-xs font-medium text-muted">{t("plan.field.date")}</span>
          <input type="date" className={input} value={date} onChange={(e) => setDate(e.target.value)} />
        </label>
        <p className="text-xs text-muted">{t("plan.goal.hint")}</p>
        {(save.error ?? remove.error) && <Problem error={save.error ?? remove.error} />}
      </div>
    </Dialog>
  );
}

/** Seite „Plan“ (E8): Wochenkalender mit geplanten Einheiten und Ist-Aktivitäten, Ziel und Formvorschau des Plans. */
export function PlanPage() {
  const config = useConfig().data?.data;
  const tz = config?.timezone;
  const today = localDate(new Date(), tz);
  const [start, setStart] = useState<string | null>(null);
  const first = start ?? defaultStart(today);
  const last = addDays(first, WEEKS * 7 - 1);
  const weeks = weekDays(first);
  const [dialog, setDialog] = useState<{ unit: PlannedUnit | null; date: string } | null>(null);
  const [goalOpen, setGoalOpen] = useState(false);

  const plan = useQuery({
    queryKey: ["plan", first, last],
    queryFn: () => apiGet<PlanResponse>(`/plan?from=${first}&to=${last}`),
    placeholderData: (p) => p,
  });
  const pastEnd = last < today ? last : today;
  const acts = useQuery({
    queryKey: ["activities", "plan", first, pastEnd],
    enabled: first <= today,
    queryFn: () => apiGet<ActivityListResponse>(`/activities?from=${first}&to=${pastEnd}&limit=200&sort=start_time`),
  });
  const formFrom = addDays(today, -28) < first ? addDays(today, -28) : first;
  const form = useQuery({
    queryKey: ["form", "plan", formFrom],
    queryFn: () => apiGet<FormResponse>(`/form?from=${formFrom}&forecast=plan&forecast_days=28`),
    placeholderData: (p) => p,
  });
  const zones = useQuery({ queryKey: ["hr-zones"], queryFn: () => apiGet<HrZonesResponse>("/hr-zones") });

  const units = useMemo(() => plan.data?.data.units ?? [], [plan.data]);
  const goal = plan.data?.data.goal ?? null;
  const f = form.data?.data;
  // Tagesbelastung: bis heute gemessen, danach geplant (Vorschau „Plan“)
  const dayLoad = useMemo(() => {
    const m = new Map<string, number>();
    for (const d of f?.days ?? []) m.set(d.date, d.load);
    for (const d of f?.forecast.days ?? []) m.set(d.date, d.load);
    return m;
  }, [f]);
  const actsByDay = useMemo(() => {
    const m = new Map<string, NonNullable<ActivityListResponse["data"]>>();
    for (const a of acts.data?.data ?? []) {
      const day = localDate(new Date(a.start_time), tz);
      m.set(day, [...(m.get(day) ?? []), a]);
    }
    return m;
  }, [acts.data, tz]);
  const linked = new Set(units.map((u) => u.activity_id).filter(Boolean));

  const points = f ? chartPoints(f.days.filter((d) => d.date >= addDays(today, -28)), f.forecast.days) : [];
  const atGoal = goal ? f?.forecast.days.find((d) => d.date === goal.date) : undefined;
  const open = units.filter((u) => u.status === "planned" && u.date > today);
  const plannedLoad = open.reduce((a, u) => a + u.load.total, 0);
  const state = (v: number) => {
    const s = formState(v);
    return <span className={`rounded-full px-2 py-0.5 text-xs font-semibold ${STATE_CLASS[s]}`}>{t(`form.state.${s}` as TextKey)}</span>;
  };

  return (
    <div className="grid gap-4">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <h1 className="text-2xl font-bold">{t("plan.title")}</h1>
        <div className="flex flex-wrap items-center gap-2">
          <button type="button" className={small} aria-label={t("plan.prev")} onClick={() => setStart(addDays(first, -7))}>
            <ChevronLeft size={15} />
          </button>
          <button type="button" className={small} onClick={() => setStart(null)}>
            {t("plan.today")}
          </button>
          <button type="button" className={small} aria-label={t("plan.next")} onClick={() => setStart(addDays(first, 7))}>
            <ChevronRight size={15} />
          </button>
          <button type="button" className={small} onClick={() => setGoalOpen(true)}>
            <Flag size={14} />
            {goal ? `${goal.name} · ${germanDate(goal.date)}` : t("plan.goal.set")}
          </button>
          <button type="button" className={`${primaryButton} inline-flex items-center gap-1`} onClick={() => setDialog({ unit: null, date: today < first ? first : addDays(today, 1) })}>
            <Plus size={15} />
            {t("plan.unit.add")}
          </button>
        </div>
      </div>

      {plan.error && <Problem error={plan.error} />}
      {plan.data && !plan.data.data.zones_configured && (
        <p className="rounded-card border border-dashed border-line px-4 py-3 text-sm text-muted">
          {t("plan.noZonesHint")}{" "}
          <Link to="/konto" className="text-accent hover:underline">
            {t("form.noZonesLink")}
          </Link>
        </p>
      )}

      <Card title={t("plan.weeks")}>
        <div className="grid gap-px overflow-hidden rounded-lg border border-line bg-line lg:grid-cols-[88px_repeat(7,minmax(0,1fr))_120px]" data-testid="plan-calendar">
          <div className="hidden bg-card-2 lg:block" />
          {WEEKDAYS.map((w) => (
            <div key={w} className="hidden bg-card-2 px-2 py-1 text-center text-[11px] font-semibold text-muted lg:block">
              {w}
            </div>
          ))}
          <div className="hidden bg-card-2 px-2 py-1 text-center text-[11px] font-semibold text-muted lg:block">{t("plan.week")}</div>
          {weeks.map((days) => {
            const loads = days.map((d) => dayLoad.get(d) ?? 0);
            const mono = weekMonotony(loads);
            const weekUnits = units.filter((u) => u.date >= days[0]! && u.date <= days[6]!);
            const zoneMin = [0, 1, 2, 3, 4].map((i) => weekUnits.reduce((a, u) => a + (u.zone_minutes?.[i] ?? 0), 0));
            const zoneSum = zoneMin.reduce((a, b) => a + b, 0);
            return (
              <div key={days[0]} className="contents">
                <div className="bg-card-2 px-2 py-2 text-xs text-muted">
                  <b className="block text-sm text-text">{t("plan.kw", { n: isoWeek(days[0]!) })}</b>
                  {germanDate(days[0]!).slice(0, 6)}–{germanDate(days[6]!).slice(0, 6)}
                </div>
                {days.map((d, i) => {
                  const dayUnits = units.filter((u) => u.date === d);
                  const dayActs = (actsByDay.get(d) ?? []).filter((a) => !linked.has(a.id));
                  return (
                    <div
                      key={d}
                      role="button"
                      tabIndex={0}
                      aria-label={t("plan.dayAdd", { date: germanDate(d) })}
                      data-day={d}
                      onClick={(e) => {
                        if ((e.target as HTMLElement).closest("[data-unit],[data-act]")) return;
                        setDialog({ unit: null, date: d });
                      }}
                      onKeyDown={(e) => {
                        if (e.key === "Enter") setDialog({ unit: null, date: d });
                      }}
                      className={`relative grid min-h-10 cursor-pointer lg:min-h-24 content-start gap-1 p-1.5 hover:bg-card-2 ${d < today ? "bg-card/80" : "bg-card"} ${d === today ? "ring-2 ring-inset ring-accent" : ""}`}
                    >
                      <span className="text-[11px] text-muted">
                        <span className="lg:hidden">{WEEKDAYS[i]} </span>
                        {germanDate(d).slice(0, 6)}
                      </span>
                      {dayUnits.map((u) => (
                        <button
                          key={u.id}
                          type="button"
                          data-unit={u.id}
                          onClick={() => setDialog({ unit: u, date: u.date })}
                          className={`grid gap-0.5 rounded-md border border-l-[3px] border-line bg-card-2 px-1.5 py-1 text-left text-xs ${BORDER[u.status]}`}
                        >
                          <span className="flex items-center gap-1 truncate font-semibold">
                            <SportIcon sport={u.sport_class} size={13} decorative />
                            <span className="truncate">{u.title}</span>
                          </span>
                          <span className="num flex gap-1.5 text-[11px] text-muted">
                            <span>{hm(u.load.minutes)}</span>
                            <span>{num(u.load.total)}</span>
                            {u.status === "done" && <span className="text-good">✓</span>}
                          </span>
                        </button>
                      ))}
                      {dayActs.map((a) => (
                        <Link
                          key={a.id}
                          to={`/aktivitaeten/${a.id}`}
                          data-act={a.id}
                          className="grid gap-0.5 rounded-md border border-dashed border-line px-1.5 py-1 text-xs hover:border-accent"
                          title={t("plan.actual")}
                        >
                          <span className="flex items-center gap-1 truncate">
                            <SportIcon sport={a.sport_class} size={13} decorative />
                            <span className="truncate">{a.title}</span>
                          </span>
                          <span className="num flex gap-1.5 text-[11px] text-muted">
                            <span>{hm(a.total_time_ms / 60000)}</span>
                            {a.load != null && <span>{num(a.load)}</span>}
                            <span>{t("plan.actualShort")}</span>
                          </span>
                        </Link>
                      ))}
                      {goal?.date === d && (
                        <span className="justify-self-end rounded-md bg-accent-strong px-1.5 text-[11px] font-bold text-white">🏁 {goal.name}</span>
                      )}
                    </div>
                  );
                })}
                <div className="grid content-start gap-1 bg-card px-2 py-2 text-xs" data-testid="plan-week-summary">
                  <span className="text-muted">{t(days[0]! > today ? "plan.sum.planned" : days[6]! < today ? "plan.sum.actual" : "plan.sum.mixed")}</span>
                  <span className="num text-base font-bold">{num(loads.reduce((a, b) => a + b, 0))}</span>
                  <span className="text-muted">
                    {t("form.monotonyShort")}{" "}
                    <b className={`num ${mono != null && mono > 2 ? "text-danger" : "text-text"}`}>{mono == null ? "–" : num(mono, 2)}</b>
                  </span>
                  {zoneSum > 0 && (
                    <span className="flex h-1.5 overflow-hidden rounded bg-card-2" title={t("plan.zoneShare")}>
                      {zoneMin.map((m, k) => (
                        <i key={k} style={{ width: `${(m / zoneSum) * 100}%`, background: ZONE_COLORS[k] }} />
                      ))}
                    </span>
                  )}
                </div>
              </div>
            );
          })}
        </div>
        <p className="mt-2 text-xs text-muted">{t("plan.legend")}</p>
      </Card>

      <Card title={t("plan.preview")} className="grid gap-3">
        {f && (
          <div className="grid grid-cols-2 gap-2 md:grid-cols-4">
            <div className="rounded-lg border border-line bg-card-2 px-3 py-2">
              <div className="text-xs text-muted">{t("plan.tile.formToday")}</div>
              <div className="num text-xl font-bold">{signed(f.current.form)}</div>
              {state(f.current.form)}
            </div>
            <div className="rounded-lg border border-line bg-card-2 px-3 py-2">
              <div className="text-xs text-muted">{goal ? t("plan.tile.formGoal", { date: germanDate(goal.date) }) : t("plan.tile.noGoal")}</div>
              <div className="num text-xl font-bold">{atGoal ? signed(atGoal.form) : "–"}</div>
              {atGoal && state(atGoal.form)}
            </div>
            <div className="rounded-lg border border-line bg-card-2 px-3 py-2">
              <div className="text-xs text-muted">{t(atGoal ? "plan.tile.fitnessGoal" : "plan.tile.fitnessEnd")}</div>
              <div className="num text-xl font-bold">{num((atGoal ?? f.forecast.days.at(-1) ?? f.current).fitness)}</div>
              <div className="text-xs text-muted">{t("plan.tile.today", { v: num(f.current.fitness) })}</div>
            </div>
            <div className="rounded-lg border border-line bg-card-2 px-3 py-2">
              <div className="text-xs text-muted">{t("plan.tile.planned")}</div>
              <div className="num text-xl font-bold">{num(plannedLoad)}</div>
              <div className="text-xs text-muted">{t("plan.tile.units", { n: open.length })}</div>
            </div>
          </div>
        )}
        {form.error && <Problem error={form.error} />}
        {points.length > 0 && (
          <div className="grid min-w-0 gap-4 xl:grid-cols-2">
            <LoadChart points={points} parts={false} goal={goal} />
            <FormChart points={points} goal={goal} />
          </div>
        )}
        <p className="text-xs text-muted">
          {t("plan.previewHint")}{" "}
          <Link to="/form?vorschau=plan" className="text-accent hover:underline">
            {t("plan.toForm")}
          </Link>
        </p>
      </Card>

      {dialog && (
        <UnitDialog
          unit={dialog.unit}
          date={dialog.date}
          zoneSets={zones.data?.data ?? []}
          settings={f?.settings}
          catalog={config?.sub_sport_catalog}
          onClose={() => setDialog(null)}
        />
      )}
      {goalOpen && <GoalDialog goal={goal} today={today} onClose={() => setGoalOpen(false)} />}
    </div>
  );
}
