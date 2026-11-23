import { useQuery } from "@tanstack/react-query";
import { Link, useNavigate } from "react-router-dom";
import { ArrowRight } from "lucide-react";
import {
  apiGet,
  type Activity,
  type ActivityListResponse,
  type DashboardStatsResponse,
  type PeriodBlock,
  type PlanResponse,
  type SportClass,
} from "../../api/client";
import { Card } from "../../components/Card";
import { Problem } from "../../components/Problem";
import { ActivitySportIcon } from "../../components/SportIcon";
import { useConfig, useSubSportLabel } from "../../hooks/useConfig";
import { t } from "../../i18n";
import { addDays, dateTime, duration, hoursMinutes, km, localDate, num, speed } from "../../utils/format";
import type { DetailOrigin } from "../activities/ActivityDetailPage";
import type { ImportHandoff } from "../import/ImportPage";
import { DropZone } from "../import/parts";
import { classesIn, dayBars, monthTitle, totalOf } from "./dashboardModel";
import { DayBars } from "./DayBars";
import { MiniMap } from "./MiniMap";
import { WeekCard } from "./WeekCard";

/** Weniger, dafür größere Einträge mit Karte (Uli 23.11.2026). */
export const FEED_SIZE = 5;

const BADGE: Record<SportClass, string> = {
  cycling: "bg-cycling",
  ebike: "bg-ebike",
  running: "bg-running",
  skiing: "bg-skiing",
  swimming: "bg-swimming",
  studio: "bg-studio",
  unknown: "bg-unknown",
};
const TINT: Record<SportClass, string> = {
  cycling: "bg-cycling/12",
  ebike: "bg-ebike/12",
  running: "bg-running/12",
  skiing: "bg-skiing/12",
  swimming: "bg-swimming/12",
  studio: "bg-studio/12",
  unknown: "bg-unknown/12",
};

function Kpi({ label, value, unit, className = "" }: { label: string; value: string; unit?: string; className?: string }) {
  return (
    <div className={`min-w-0 ${className}`}>
      <div className="text-xs text-muted">{label}</div>
      <div className="num text-lg font-semibold">
        {value}
        {unit && <span className="ml-1 text-xs font-normal text-muted">{unit}</span>}
      </div>
    </div>
  );
}

/**
 * Eine Aktivität im Feed als Kachel (RFC-0018, Uli 23.11.2026): links die feste Mini-Karte, ohne GPS das
 * Sport-Symbol auf getönter Fläche; rechts Titel, Datum, Untertyp, Ort, Kennzahlen und ggf. die Plan-Einheit.
 */
function FeedItem({ a, timeZone, map, plan }: { a: Activity; timeZone?: string; map?: { tile_url: string; attribution: string }; plan?: string }) {
  const subLabel = useSubSportLabel();
  const catalog = useConfig().data?.data.sub_sport_catalog;
  const navigate = useNavigate();
  const running = a.sport_class === "running";
  const avg = speed(a.avg_speed_kmh, running);
  const origin: DetailOrigin = { from: "dashboard" };
  const withMap = a.has_position && !!map;
  return (
    <li
      className="grid cursor-pointer overflow-hidden rounded-xl border border-line bg-card transition hover:-translate-y-px hover:border-accent sm:grid-cols-[210px_minmax(0,1fr)]"
      onClick={() => void navigate(`/aktivitaeten/${a.id}`, { state: origin })}
    >
      <div className={`relative bg-card-2 ${withMap ? "min-h-40" : "min-h-18 sm:min-h-36"}`}>
        {withMap ? (
          <>
            <MiniMap id={a.id} tileUrl={map.tile_url} attribution={map.attribution} />
            <span className={`absolute left-2 top-2 z-[500] grid size-8 place-items-center rounded-full text-white shadow ${BADGE[a.sport_class]}`}>
              <ActivitySportIcon sport={a.sport_class} subKey={a.sub_sport} catalog={catalog} size={17} plain decorative />
            </span>
          </>
        ) : (
          <div className={`absolute inset-0 grid place-items-center ${TINT[a.sport_class]}`}>
            <ActivitySportIcon sport={a.sport_class} subKey={a.sub_sport} catalog={catalog} size={48} decorative />
          </div>
        )}
      </div>
      <div className="grid min-w-0 content-center gap-2.5 px-4 py-3">
        <div className="min-w-0">
          <Link to={`/aktivitaeten/${a.id}`} state={origin} className="block truncate text-base font-semibold hover:underline" onClick={(e) => e.stopPropagation()}>
            {a.title}
          </Link>
          <div className="truncate text-xs text-muted">
            {[dateTime(a.start_time, timeZone, true), subLabel(a.sport_class, a.sub_sport), a.location].filter(Boolean).join(" · ")}
          </div>
        </div>
        <div className="grid grid-cols-3 gap-2 sm:grid-cols-5">
          <Kpi label={t("detail.kpi.distance")} value={km(a.distance_m)} unit="km" />
          <Kpi label={t("detail.kpi.time")} value={duration(a.total_time_ms)} />
          <Kpi label={t(running ? "detail.kpi.pace" : "detail.kpi.avg")} value={avg.value} unit={avg.unit} />
          <Kpi label={t("detail.kpi.gain")} value={a.elevation_gain_m == null ? "–" : num(a.elevation_gain_m)} unit="m" className="hidden sm:block" />
          <Kpi label={t("dashboard.hr")} value={a.avg_heart_rate_bpm == null ? "–" : String(a.avg_heart_rate_bpm)} unit="bpm" className="hidden sm:block" />
        </div>
        {plan && (
          <div>
            <span className="inline-flex rounded-full bg-good/15 px-2 py-0.5 text-[11px] text-good">{t("dashboard.planTag", { title: plan })}</span>
          </div>
        )}
      </div>
    </li>
  );
}

/** Laufender Monat: Einheiten, Zeit, Distanz und Distanz je Tag (RFC-0018). */
function MonthCard({ block }: { block: PeriodBlock }) {
  const total = totalOf(block);
  return (
    <Card title={t("dashboard.month", { month: monthTitle(block.period) })}>
      <div className="grid grid-cols-3 gap-2">
        <Kpi label={t("dashboard.count")} value={num(total.count)} />
        <Kpi label={t("dashboard.time")} value={hoursMinutes(total.total_time_ms)} unit="h" />
        <Kpi label={t("dashboard.distance")} value={km(total.distance_m)} unit="km" />
      </div>
      <div className="mt-3">
        <DayBars bars={dayBars(block, "month")} classes={classesIn(block)} />
      </div>
    </Card>
  );
}

/** Laufendes Kalenderjahr, nur Summen (Uli 23.11.2026, statt der Saison). */
function YearCard({ block }: { block: PeriodBlock }) {
  const total = totalOf(block);
  return (
    <Card title={t("dashboard.year", { year: block.period })}>
      <div className="grid grid-cols-3 gap-2">
        <Kpi label={t("dashboard.count")} value={num(total.count)} />
        <Kpi label={t("dashboard.time")} value={hoursMinutes(total.total_time_ms)} unit="h" />
        <Kpi label={t("dashboard.distance")} value={km(total.distance_m)} unit="km" />
      </div>
    </Card>
  );
}

/** Startseite nach RFC-0018: links Woche mit Plan und Feed, rechts laufender Monat, Jahr und Import-Zone. */
export function DashboardPage() {
  const navigate = useNavigate();
  const config = useConfig();
  const cfg = config.data?.data;
  const feed = useQuery({
    queryKey: ["activities", "feed"],
    queryFn: () => apiGet<ActivityListResponse>(`/activities?sort=-start_time&limit=${FEED_SIZE}`),
  });
  const stats = useQuery({ queryKey: ["stats", "dashboard"], queryFn: () => apiGet<DashboardStatsResponse>("/stats/dashboard") });
  const toImport = (files: File[]) => {
    const handoff: ImportHandoff = { files };
    void navigate("/import", { state: handoff });
  };

  const list = feed.data?.data ?? [];
  const total = feed.data?.meta.total ?? 0;
  const week = stats.data?.data.week;
  // Plan der Woche und, für die Plan-Marken im Feed, zurück bis zur ältesten gezeigten Aktivität (API: höchstens 400 Tage)
  const last = list[list.length - 1];
  const oldest = last ? localDate(new Date(last.start_time), cfg?.timezone) : undefined;
  const planFrom = week ? [week.from, oldest ?? week.from, addDays(week.to, -399)].sort()[1] : undefined;
  const plan = useQuery({
    queryKey: ["plan", planFrom, week?.to],
    queryFn: () => apiGet<PlanResponse>(`/plan?from=${planFrom}&to=${week?.to}`),
    enabled: !!week && !!feed.data,
  });
  const units = plan.data?.data.units ?? [];
  const planOf = new Map(units.flatMap((u) => (u.activity_id ? [[u.activity_id, u.title] as const] : [])));
  const today = plan.data?.data.today ?? localDate(new Date(), cfg?.timezone);

  return (
    <div className="grid grid-cols-1 gap-4 lg:grid-cols-12">
      <div className="grid content-start gap-4 lg:col-span-7">
        {week && <WeekCard block={week} units={units.filter((u) => u.date >= week.from && u.date <= week.to)} today={today} />}
        <Card title={t("dashboard.feed")}>
          {feed.error ? (
            <Problem error={feed.error} />
          ) : !feed.data ? (
            <p className="text-muted">{t("state.loading")}</p>
          ) : list.length === 0 ? (
            <p className="text-muted">{t("dashboard.empty")}</p>
          ) : (
            <>
              <ul className="grid gap-3">
                {list.map((a) => (
                  <FeedItem key={a.id} a={a} timeZone={cfg?.timezone} map={cfg?.map} plan={planOf.get(a.id)} />
                ))}
              </ul>
              <Link to="/aktivitaeten" className="mt-3 flex items-center gap-1.5 text-sm font-medium text-accent hover:underline">
                {t("dashboard.all", { n: num(total) })}
                <ArrowRight size={14} />
              </Link>
            </>
          )}
        </Card>
      </div>

      <div className="grid content-start gap-4 lg:col-span-5">
        {stats.error ? (
          <Card>
            <Problem error={stats.error} />
          </Card>
        ) : !stats.data ? (
          <Card>
            <p className="text-muted">{t("state.loading")}</p>
          </Card>
        ) : (
          <>
            <MonthCard block={stats.data.data.month} />
            <YearCard block={stats.data.data.year} />
          </>
        )}
        <DropZone compact maxMb={cfg?.import_limits.max_file_size_mb ?? 50} onFiles={toImport} />
      </div>
    </div>
  );
}
