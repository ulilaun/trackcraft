import { useEffect, useState } from "react";
import { useInfiniteQuery, useQuery } from "@tanstack/react-query";
import { Link, useNavigate, useSearchParams } from "react-router-dom";
import { X } from "lucide-react";
import {
  apiGet,
  type Activity,
  type ActivityListResponse,
  type ActivitySummary,
  type SegmentResponse,
  type StatisticsPeriodsResponse,
} from "../../api/client";
import { Card, KpiCard } from "../../components/Card";
import { PeriodButton, SeasonPill, YearPill, periodSelect } from "../../components/PeriodButton";
import { Problem } from "../../components/Problem";
import { ActivitySportIcon } from "../../components/SportIcon";
import { useConfig } from "../../hooks/useConfig";
import { t, type TextKey } from "../../i18n";
import { useSeasons } from "../seasons/useSeasons";
import { germanDate, seasonRange } from "../seasons/seasonModel";
import { segmentPath } from "../segments/segmentModel";
import { dateTime, duration, hoursMinutes, km, localDate, num, speed } from "../../utils/format";
import {
  PAGE_SIZE,
  SPORT_CLASSES,
  apiQuery,
  rangeFrom,
  readFilter,
  selectSport,
  selectSportSub,
  toggleSort,
  writeFilter,
  yearChoices,
  type ListFilter,
  type ListPeriod,
  type SortField,
} from "./listModel";
import { SportFilterPill } from "../../components/SportFilterPill";

/** Zeitraum unter der Kennzahl „Aktivitäten“. */
function periodText(z: ListPeriod, season: { start_date: string; end_date: string } | null): string {
  switch (z.kind) {
    case "all":
      return t("list.allTime");
    case "last12":
      return t("list.last12");
    case "year":
      return t("list.year", { year: z.year });
    case "season":
      return season ? seasonRange(season) : "";
    case "range":
      if (z.from && z.to) return `${germanDate(z.from)} – ${germanDate(z.to)}`;
      if (z.from) return t("list.rangeFrom", { day: germanDate(z.from) });
      if (z.to) return t("list.rangeTo", { day: germanDate(z.to) });
      return t("list.allTime");
  }
}

function avg(kmh: number | null, running: boolean): string {
  const s = speed(kmh, running);
  return s.value === "–" ? "–" : `${s.value} ${s.unit}`;
}

function gain(m: number | null): string {
  return m == null ? "–" : `${num(m)} m`;
}

function SumRow({ s, shown, sport }: { s: ActivitySummary; shown: number; sport: Activity["sport_class"] | undefined }) {
  return (
    <tr className="bg-card-2 font-semibold">
      <td className="px-3 py-2.5 text-left">
        {t("list.sum", { n: num(s.count) })}
        {s.count > shown && <span className="ml-1 text-xs font-normal text-muted">{t("list.sumShown", { n: num(shown) })}</span>}
      </td>
      <td />
      <td />
      <td className="num px-3 py-2.5 text-right">{km(s.distance_m)} km</td>
      <td className="num px-3 py-2.5 text-right">{duration(s.total_time_ms)}</td>
      <td className="num px-3 py-2.5 text-right">
        {s.mixed_sport_classes ? <span className="font-normal text-muted">{t("list.mixed")}</span> : avg(s.avg_speed_kmh, sport === "running")}
      </td>
      <td className="num px-3 py-2.5 text-right">{gain(s.elevation_gain_m)}</td>
      <td className="num px-3 py-2.5 text-right">{s.avg_heart_rate_bpm ?? "–"}</td>
      <td className="num px-3 py-2.5 text-right">{s.load == null ? "–" : num(s.load)}</td>
    </tr>
  );
}

/** Aktivitätenliste nach RFC-0006 „Aktivitätenliste“ mit Auswahl einer Strecke (RFC-0016 „Filter“). */
export function ActivityListPage() {
  const [params, setParams] = useSearchParams();
  const filter = readFilter(params);
  const navigate = useNavigate();
  const config = useConfig();
  const tz = config.data?.data.timezone;
  const catalog = config.data?.data.sub_sport_catalog ?? {};
  const [search, setSearch] = useState(filter.q);
  const seasons = useSeasons();
  const seasonList = seasons.data?.data ?? [];
  const period = filter.period;
  const pickedSeason = period.kind === "season" ? (seasonList.find((x) => x.id === period.id) ?? null) : null;
  // Knopf „Saison“ ohne Auswahl zeigt die Vorgabesaison (laufende oder zuletzt beendete)
  const buttonSeason = pickedSeason ?? seasonList.find((x) => x.id === seasons.data?.meta.default_season_id) ?? seasonList[0] ?? null;
  // Jahre mit Aktivitäten für die Jahresauswahl
  const periods = useQuery({ queryKey: ["stats", "periods", "list"], queryFn: () => apiGet<StatisticsPeriodsResponse>("/stats/periods") });
  const [seenQ, setSeenQ] = useState(filter.q);
  // Suche von außen geändert (z. B. Klick auf „Aktivitäten“ in der Kopfzeile): Eingabe nachziehen.
  if (filter.q !== seenQ) {
    setSeenQ(filter.q);
    setSearch(filter.q);
  }

  const apply = (f: ListFilter) => setParams(writeFilter(f), { replace: true });

  // Suche erst nach einer kurzen Tipp-Pause an die API geben.
  useEffect(() => {
    const current = readFilter(params);
    if (search.trim() === current.q.trim()) return;
    const id = setTimeout(() => setParams(writeFilter({ ...current, q: search }), { replace: true }), 300);
    return () => clearTimeout(id);
  }, [search, params, setParams]);

  const today = localDate(new Date(), tz);
  const list = useInfiniteQuery({
    queryKey: [
      "activities",
      "list",
      writeFilter(filter).toString(),
      period.kind === "last12" ? today : null,
      pickedSeason ? `${pickedSeason.start_date}/${pickedSeason.end_date}` : null,
    ],
    // Mit Saison erst, wenn deren Zeitraum bekannt ist
    enabled: config.isSuccess && (period.kind !== "season" || !!pickedSeason),
    initialPageParam: 0,
    queryFn: ({ pageParam }) => apiGet<ActivityListResponse>(`/activities?${apiQuery(filter, today, pageParam, undefined, pickedSeason)}`),
    getNextPageParam: (last) => (last.meta.offset + last.data.length < last.meta.total ? last.meta.offset + last.data.length : undefined),
  });

  // Streckenfilter nur über die Streckenseite („Alle Fahrten“), hier als entfernbarer Filter (Uli 12.11.2026)
  const route = useQuery({
    queryKey: ["segments", "one", filter.route],
    enabled: !!filter.route,
    queryFn: () => apiGet<SegmentResponse>(`/segments/${filter.route}`),
  });

  // Gewählte Saison gibt es nicht mehr: Filter fallen lassen
  const staleSeason = period.kind === "season" && seasons.isSuccess && !pickedSeason;
  useEffect(() => {
    if (staleSeason) setParams(writeFilter({ ...readFilter(params), period: { kind: "all" } }), { replace: true });
  }, [staleSeason, params, setParams]);

  const thisYear = Number(today.slice(0, 4));
  const years = yearChoices(
    (periods.data?.data.years ?? []).filter((y) => y.count > 0).map((y) => y.year),
    thisYear,
    period.kind === "year" ? period.year : null,
  );
  const shownYear = period.kind === "year" ? period.year : thisYear;
  const setPeriod = (z: ListPeriod) => apply({ ...filter, period: z });
  const setRange = (side: "from" | "to", day: string) => {
    if (period.kind !== "range") return;
    setPeriod({ ...period, [side]: day || null });
  };

  const rows = list.data?.pages.flatMap((p) => p.data) ?? [];
  const meta = list.data?.pages[0]?.meta;
  const s = meta?.summary;
  const remaining = meta ? meta.total - rows.length : 0;

  const sortTh = (field: SortField, key: TextKey) => (
    <th
      className="cursor-pointer select-none px-3 py-2.5 text-right font-medium hover:text-text"
      aria-sort={filter.sort === field ? (filter.desc ? "descending" : "ascending") : "none"}
      onClick={() => apply(toggleSort(filter, field))}
    >
      {t(key)}
      {filter.sort === field ? (filter.desc ? " ↓" : " ↑") : ""}
    </th>
  );

  return (
    <div className="grid gap-4">
      <h1 className="text-2xl font-bold">{t("nav.activities")}</h1>
      <div className="grid grid-cols-2 gap-4 lg:grid-cols-4">
        <KpiCard label={t("list.kpi.count")} value={s ? num(s.count) : "–"} sub={periodText(period, pickedSeason)} />
        <KpiCard label={t("list.kpi.distance")} value={s ? num(s.distance_m / 1000) : "–"} unit="km" />
        <KpiCard label={t("list.kpi.time")} value={s ? hoursMinutes(s.total_time_ms) : "–"} unit="h" />
        <KpiCard label={t("list.kpi.gain")} value={s ? num(s.elevation_gain_m) : "–"} unit="m" />
      </div>

      <Card>
        <div className="grid gap-3">
          <div className="flex flex-wrap items-center gap-2">
            <PeriodButton on={period.kind === "all"} onClick={() => setPeriod({ kind: "all" })}>
              {t("list.periodAll")}
            </PeriodButton>
            <PeriodButton on={period.kind === "last12"} onClick={() => setPeriod({ kind: "last12" })}>
              {t("list.last12")}
            </PeriodButton>
            {/* Jahr und Saison als geteilte Knöpfe mit Menü */}
            <YearPill
              on={period.kind === "year"}
              year={shownYear}
              years={years}
              onPick={() => setPeriod({ kind: "year", year: shownYear })}
              onSelect={(year) => setPeriod({ kind: "year", year })}
            />
            {buttonSeason && (
              <SeasonPill
                on={period.kind === "season"}
                season={buttonSeason}
                seasons={seasonList}
                onPick={() => setPeriod({ kind: "season", id: buttonSeason.id })}
                onSelect={(id) => setPeriod({ kind: "season", id })}
              />
            )}
            <PeriodButton on={period.kind === "range"} onClick={() => setPeriod(rangeFrom(period, today, pickedSeason))}>
              {t("list.range")}
            </PeriodButton>
            {period.kind === "range" && (
              <span className="flex items-center gap-1.5">
                <input
                  type="date"
                  className={periodSelect}
                  aria-label={t("list.rangeStart")}
                  value={period.from ?? ""}
                  onChange={(e) => setRange("from", e.target.value)}
                />
                <span className="text-muted">–</span>
                <input
                  type="date"
                  className={periodSelect}
                  aria-label={t("list.rangeEnd")}
                  value={period.to ?? ""}
                  onChange={(e) => setRange("to", e.target.value)}
                />
              </span>
            )}
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <input
              type="search"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              placeholder={t("list.search")}
              aria-label={t("list.search")}
              className="min-w-[240px] max-w-[420px] flex-1 rounded-lg border border-line bg-card-2 px-3 py-1.5 text-sm"
            />
            {filter.route && (
              <span className="flex items-center gap-1.5 rounded-full bg-accent-strong py-1.5 pr-1.5 pl-3 text-sm text-white">
                <Link to={segmentPath("route", filter.route)} className="max-w-[260px] truncate hover:underline">
                  {t("list.routeChip", { name: route.data?.data.name ?? "…" })}
                </Link>
                <button
                  type="button"
                  className="rounded-full p-0.5 hover:bg-white/20"
                  aria-label={t("list.routeChipRemove")}
                  title={t("list.routeChipRemove")}
                  onClick={() => apply({ ...filter, route: null })}
                >
                  <X size={14} />
                </button>
              </span>
            )}
          </div>
          <div className="flex flex-wrap items-center gap-1.5">
            <button
              type="button"
              aria-pressed={filter.sport === null}
              onClick={() => apply(selectSport(filter, null))}
              className={`rounded-full border px-3 py-1 text-sm ${
                filter.sport === null ? "border-transparent bg-accent-strong text-white" : "border-line bg-card-2 text-muted hover:text-text"
              }`}
            >
              {t("list.allData")}
            </button>
            {SPORT_CLASSES.map((c) => (
              <SportFilterPill
                key={c}
                sport={c}
                label={t(`sport.${c}` as TextKey)}
                on={filter.sport === c}
                sub={filter.sport === c ? filter.sub : null}
                subs={catalog[c] ?? []}
                onPick={() => apply(selectSport(filter, c))}
                onPickSub={(sub) => apply(selectSportSub(filter, c, sub))}
              />
            ))}
          </div>
        </div>
      </Card>

      <Card className="overflow-hidden p-0">
        {list.error || config.error ? (
          <div className="p-4">
            <Problem error={list.error ?? config.error} />
          </div>
        ) : !list.data ? (
          <p className="p-4 text-muted">{t("state.loading")}</p>
        ) : rows.length === 0 ? (
          <p className="p-4 text-muted">{t("list.empty")}</p>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full border-collapse whitespace-nowrap text-sm">
              <thead className="text-xs text-muted">
                <tr>
                  <th className="px-3 py-2.5 text-left font-medium">{t("list.col.title")}</th>
                  {sortTh("start_time", "list.col.start")}
                  {sortTh("distance_m", "list.col.distance")}
                  {sortTh("total_time_ms", "list.col.time")}
                  <th className="px-3 py-2.5 text-right font-medium">{t("list.col.avg")}</th>
                  {sortTh("elevation_gain_m", "list.col.gain")}
                  <th className="px-3 py-2.5 text-right font-medium">{t("list.col.hr")}</th>
                  {sortTh("load", "list.col.load")}
                </tr>
              </thead>
              <tbody>
                {rows.map((a) => (
                  <tr
                    key={a.id}
                    className="cursor-pointer border-t border-line hover:bg-card-2"
                    onClick={() => navigate(`/aktivitaeten/${a.id}`, { state: { from: "list" } })}
                  >
                    <td className="max-w-[420px] px-3 py-2.5 text-left">
                      <span className="flex items-center gap-2.5">
                        <ActivitySportIcon sport={a.sport_class} subKey={a.sub_sport} catalog={catalog} size={18} />
                        <Link
                          to={`/aktivitaeten/${a.id}`}
                          state={{ from: "list" }}
                          className="truncate font-medium hover:text-accent"
                          onClick={(e) => e.stopPropagation()}
                        >
                          {a.title}
                        </Link>
                      </span>
                    </td>
                    <td className="num px-3 py-2.5 text-right">{dateTime(a.start_time, tz)}</td>
                    <td className="num px-3 py-2.5 text-right">{a.distance_m == null ? "–" : `${km(a.distance_m)} km`}</td>
                    <td className="num px-3 py-2.5 text-right">{duration(a.total_time_ms)}</td>
                    <td className="num px-3 py-2.5 text-right">{avg(a.avg_speed_kmh, a.sport_class === "running")}</td>
                    <td className="num px-3 py-2.5 text-right">{gain(a.elevation_gain_m)}</td>
                    <td className="num px-3 py-2.5 text-right">{a.avg_heart_rate_bpm ?? "–"}</td>
                    <td className="num px-3 py-2.5 text-right" title={a.load_method === "estimated" ? t("list.loadEstimated") : undefined}>
                      {a.load == null ? "–" : num(a.load)}
                      {a.load_method === "estimated" && <span className="ml-0.5 text-muted">*</span>}
                    </td>
                  </tr>
                ))}
              </tbody>
              {s && (
                <tfoot className="border-t border-line">
                  <SumRow s={s} shown={rows.length} sport={filter.sport ?? rows[0]?.sport_class} />
                </tfoot>
              )}
            </table>
          </div>
        )}
      </Card>
      {remaining > 0 && (
        <div className="text-center">
          <button
            type="button"
            className="rounded-lg border border-line bg-card-2 px-4 py-2 text-sm hover:border-accent disabled:opacity-50"
            disabled={list.isFetchingNextPage}
            onClick={() => void list.fetchNextPage()}
          >
            {list.isFetchingNextPage ? t("state.loading") : t("list.more", { n: num(Math.min(remaining, PAGE_SIZE)), rest: num(remaining) })}
          </button>
        </div>
      )}
    </div>
  );
}
