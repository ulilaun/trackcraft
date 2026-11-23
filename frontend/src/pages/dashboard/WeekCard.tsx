import type { PeriodBlock, PlannedUnit } from "../../api/client";
import { Card } from "../../components/Card";
import { SportIcon } from "../../components/SportIcon";
import { t } from "../../i18n";
import { hoursMinutes, km, num } from "../../utils/format";
import { STACK_ORDER, plannedMinutes, totalOf, weekDays, weekRange } from "./dashboardModel";

const BG: Record<string, string> = {
  cycling: "bg-cycling",
  ebike: "bg-ebike",
  running: "bg-running",
  skiing: "bg-skiing",
  swimming: "bg-swimming",
  studio: "bg-studio",
  unknown: "bg-unknown",
};

const hm = (min: number) => hoursMinutes(min * 60_000);

function Progress({ value, of }: { value: number; of: number }) {
  return (
    <div className="mt-1 h-1 overflow-hidden rounded-full bg-card-2">
      <div className="h-full rounded-full bg-accent" style={{ width: `${Math.min(100, (value / of) * 100)}%` }} />
    </div>
  );
}

/**
 * Laufende Woche (Uli 23.11.2026): Summen „erledigt von geplant“ und je Tag ein Balken mit der erledigten Zeit
 * je Sportklasse vor der gestrichelt umrandeten geplanten Zeit. Ohne Plan nur Ist.
 */
export function WeekCard({ block, units, today }: { block: PeriodBlock; units: PlannedUnit[]; today: string }) {
  const days = weekDays(block, units);
  const total = totalOf(block);
  const open = units.filter((u) => u.status !== "skipped");
  const plannedCount = open.length;
  const plannedMin = open.reduce((s, u) => s + plannedMinutes(u), 0);
  const max = Math.max(60, ...days.map((d) => Math.max(d.doneTotal, d.planned)));

  return (
    <Card>
      <div className="mb-3 flex items-baseline gap-2">
        <h2 className="text-xs font-semibold uppercase tracking-wide text-muted">{t("dashboard.week")}</h2>
        <span className="num ml-auto text-xs text-muted">{weekRange(block.from, block.to)}</span>
      </div>
      <div className="grid grid-cols-3 gap-3">
        <div className="min-w-0">
          <div className="text-xs text-muted">{t("dashboard.count")}</div>
          <div className="num text-lg font-semibold">
            {num(total.count)}
            {plannedCount > 0 && <span className="ml-1 text-xs font-normal text-muted">{t("dashboard.ofPlanned", { n: num(plannedCount) })}</span>}
          </div>
          {plannedCount > 0 && <Progress value={total.count} of={plannedCount} />}
        </div>
        <div className="min-w-0">
          <div className="text-xs text-muted">{t("dashboard.time")}</div>
          <div className="num text-lg font-semibold">
            {hoursMinutes(total.total_time_ms)}
            <span className="ml-1 text-xs font-normal text-muted">
              {plannedMin > 0 ? t("dashboard.ofPlannedHours", { n: hm(plannedMin) }) : "h"}
            </span>
          </div>
          {plannedMin > 0 && <Progress value={total.total_time_ms / 60_000} of={plannedMin} />}
        </div>
        <div className="min-w-0">
          <div className="text-xs text-muted">{t("dashboard.distance")}</div>
          <div className="num text-lg font-semibold">
            {km(total.distance_m)}
            <span className="ml-1 text-xs font-normal text-muted">km</span>
          </div>
        </div>
      </div>

      <div className="mt-4 grid h-32 grid-cols-7 items-end gap-2" role="list" aria-label={t("dashboard.week")}>
        {days.map((d) => {
          const isToday = d.date === today;
          const label = t("dashboard.dayTitle", { day: isToday ? t("dashboard.today") : d.label, done: hm(d.doneTotal), planned: hm(d.planned) });
          return (
            <div key={d.date} role="listitem" title={label} aria-label={label} className="relative flex h-full items-end justify-center">
              {d.planned > 0 && (
                <div
                  className="absolute inset-x-[18%] bottom-0 rounded-t-md border-[1.5px] border-dashed border-muted/60"
                  style={{ height: `${(d.planned / max) * 100}%` }}
                  data-testid="planned-bar"
                />
              )}
              {d.doneTotal > 0 && (
                <div className="relative flex w-[64%] flex-col-reverse overflow-hidden rounded-t-md" style={{ height: `${(d.doneTotal / max) * 100}%` }}>
                  {STACK_ORDER.filter((c) => d.done[c] > 0).map((c) => (
                    <div key={c} className={BG[c]} style={{ height: `${(d.done[c] / d.doneTotal) * 100}%` }} />
                  ))}
                </div>
              )}
            </div>
          );
        })}
      </div>
      <div className="mt-1.5 grid grid-cols-7 gap-2 text-center text-[11px] text-muted">
        {days.map((d) => (
          <div key={d.date} className={`grid justify-items-center gap-0.5 ${d.date === today ? "font-semibold text-accent" : ""}`}>
            <span className="grid h-4 place-items-center">{d.sport && <SportIcon sport={d.sport} size={16} decorative />}</span>
            {d.date === today ? t("dashboard.today") : d.label}
          </div>
        ))}
      </div>
      <div className="mt-3 flex flex-wrap gap-4 text-[11px] text-muted">
        <span className="inline-flex items-center gap-1.5">
          <i className="inline-block size-3 rounded-full bg-accent" />
          {t("dashboard.weekDone")}
        </span>
        <span className="inline-flex items-center gap-1.5">
          <i className="inline-block size-3 rounded-full border-[1.5px] border-dashed border-muted" />
          {t("dashboard.weekPlanned")}
        </span>
      </div>
    </Card>
  );
}
