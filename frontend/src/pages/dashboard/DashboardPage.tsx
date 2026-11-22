import { useQuery } from "@tanstack/react-query";
import { Link, useNavigate } from "react-router-dom";
import { ArrowRight } from "lucide-react";
import { apiGet, type Activity, type ActivityListResponse, type DashboardStatsResponse, type PeriodBlock } from "../../api/client";
import { Card } from "../../components/Card";
import { Problem } from "../../components/Problem";
import { ActivitySportIcon } from "../../components/SportIcon";
import { useConfig, useSubSportLabel } from "../../hooks/useConfig";
import { t } from "../../i18n";
import { dateTime, duration, hoursMinutes, km, num, speed } from "../../utils/format";
import type { DetailOrigin } from "../activities/ActivityDetailPage";
import type { ImportHandoff } from "../import/ImportPage";
import { DropZone } from "../import/parts";
import { seasonRange } from "../seasons/seasonModel";
import { classesIn, dayBars, monthTitle, totalOf, weekBars, type Dashboard } from "./dashboardModel";
import { DayBars } from "./DayBars";

export const FEED_SIZE = 10;

function Kpi({ label, value, unit }: { label: string; value: string; unit?: string }) {
  return (
    <div className="min-w-0">
      <div className="text-xs text-muted">{label}</div>
      <div className="num text-lg font-semibold">
        {value}
        {unit && <span className="ml-1 text-xs font-normal text-muted">{unit}</span>}
      </div>
    </div>
  );
}

/** Eine Aktivität im Feed: Kopf mit Icon, Titel, Untertyp, Ort, Datum und KPI-Strip (RFC-0018). */
function FeedItem({ a, timeZone }: { a: Activity; timeZone?: string }) {
  const subLabel = useSubSportLabel();
  const catalog = useConfig().data?.data.sub_sport_catalog;
  const navigate = useNavigate();
  const running = a.sport_class === "running";
  const avg = speed(a.avg_speed_kmh, running);
  const origin: DetailOrigin = { from: "dashboard" };
  return (
    <li
      className="cursor-pointer border-b border-line px-1 py-3 last:border-b-0 hover:bg-card-2"
      onClick={() => void navigate(`/aktivitaeten/${a.id}`, { state: origin })}
    >
      <div className="flex items-center gap-3">
        <span className="grid h-9 w-9 shrink-0 place-items-center rounded-xl bg-card-2">
          <ActivitySportIcon sport={a.sport_class} subKey={a.sub_sport} catalog={catalog} size={18} />
        </span>
        <div className="min-w-0 flex-1">
          <Link to={`/aktivitaeten/${a.id}`} state={origin} className="block truncate font-semibold hover:underline" onClick={(e) => e.stopPropagation()}>
            {a.title}
          </Link>
          <div className="flex items-center gap-1.5 truncate text-xs text-muted">
            <span className="truncate">{[dateTime(a.start_time, timeZone, true), subLabel(a.sport_class, a.sub_sport), a.location].filter(Boolean).join(" · ")}</span>
          </div>
        </div>
      </div>
      <div className="mt-2 grid grid-cols-3 gap-2 pl-12 sm:grid-cols-5">
        <Kpi label={t("detail.kpi.distance")} value={km(a.distance_m)} unit="km" />
        <Kpi label={t("detail.kpi.time")} value={duration(a.total_time_ms)} />
        <Kpi label={t(running ? "detail.kpi.pace" : "detail.kpi.avg")} value={avg.value} unit={avg.unit} />
        <Kpi label={t("detail.kpi.gain")} value={a.elevation_gain_m == null ? "–" : num(a.elevation_gain_m)} unit="m" />
        <Kpi label={t("dashboard.hr")} value={a.avg_heart_rate_bpm == null ? "–" : String(a.avg_heart_rate_bpm)} unit="bpm" />
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

/** Laufende oder zuletzt beendete Saison: Kennzahlen und Distanz je ISO-Woche (RFC-0018). */
function SeasonCard({ block }: { block: NonNullable<Dashboard["season"]> }) {
  const total = totalOf(block);
  const s = block.season;
  return (
    <Card>
      <div className="mb-3 flex flex-wrap items-baseline gap-x-2 gap-y-1">
        <h2 className="text-xs font-semibold uppercase tracking-wide text-muted">
          <Link to={`/aktivitaeten?saison=${encodeURIComponent(s.id)}`} className="hover:text-text hover:underline">
            {t("dashboard.season", { name: s.name })}
          </Link>
        </h2>
        {s.state === "ended" && <span className="rounded-full bg-card-2 px-2 py-0.5 text-xs text-muted">{t("dashboard.seasonEnded")}</span>}
        <span className="ml-auto font-mono text-xs text-muted">{seasonRange(s)}</span>
      </div>
      <div className="grid grid-cols-3 gap-2">
        <Kpi label={t("dashboard.count")} value={num(total.count)} />
        <Kpi label={t("dashboard.time")} value={hoursMinutes(total.total_time_ms)} unit="h" />
        <Kpi label={t("dashboard.distance")} value={km(total.distance_m)} unit="km" />
      </div>
      <div className="mt-3">
        <DayBars bars={weekBars(block)} classes={classesIn(block)} />
      </div>
    </Card>
  );
}

/** Startseite nach RFC-0018: Feed der letzten Aktivitäten, laufender Monat und Saison, Import-Zone. */
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

  return (
    <div className="grid grid-cols-1 gap-4 lg:grid-cols-12">
      <Card title={t("dashboard.feed")} className="lg:col-span-7">
        {feed.error ? (
          <Problem error={feed.error} />
        ) : !feed.data ? (
          <p className="text-muted">{t("state.loading")}</p>
        ) : list.length === 0 ? (
          <p className="text-muted">{t("dashboard.empty")}</p>
        ) : (
          <>
            <ul>
              {list.map((a) => (
                <FeedItem key={a.id} a={a} timeZone={cfg?.timezone} />
              ))}
            </ul>
            <Link to="/aktivitaeten" className="mt-3 flex items-center gap-1.5 text-sm font-medium text-accent hover:underline">
              {t("dashboard.all", { n: num(total) })}
              <ArrowRight size={14} />
            </Link>
          </>
        )}
      </Card>

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
            {/* Ohne „Diese Woche“, damit die Import-Ablage ohne Scrollen sichtbar bleibt (Uli 14.11.2026) */}
            <MonthCard block={stats.data.data.month} />
            {stats.data.data.season && <SeasonCard block={stats.data.data.season} />}
          </>
        )}
        <DropZone compact maxMb={cfg?.import_limits.max_file_size_mb ?? 50} onFiles={toImport} />
      </div>
    </div>
  );
}
