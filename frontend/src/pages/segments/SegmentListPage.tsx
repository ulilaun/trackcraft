import { useEffect, useState, type ReactNode } from "react";
import { useInfiniteQuery } from "@tanstack/react-query";
import { Link, useNavigate, useSearchParams } from "react-router-dom";
import { apiGet, type SegmentKind, type SegmentListResponse } from "../../api/client";
import { Card, KpiCard } from "../../components/Card";
import { CategoryIcon, categoriesFor, SegmentKindIcon } from "../../components/CategoryIcon";
import { Problem } from "../../components/Problem";
import { SportIcon } from "../../components/SportIcon";
import { useConfig } from "../../hooks/useConfig";
import { t, type TextKey } from "../../i18n";
import { dateOnly, km, num, segmentTime } from "../../utils/format";
import {
  PAGE_SIZE,
  RANKED_CLASSES,
  apiQuery,
  readFilter,
  segmentPath,
  toggleSort,
  writeFilter,
  type SegmentFilter,
  type SegmentSort,
} from "./segmentModel";

function Pill({ on, onClick, children }: { on: boolean; onClick: () => void; children: ReactNode }) {
  return (
    <button
      type="button"
      aria-pressed={on}
      onClick={onClick}
      className={`rounded-full border px-3 py-1.5 text-sm ${
        on ? "border-transparent bg-accent-strong text-white" : "border-line bg-card-2 text-muted hover:text-text"
      }`}
    >
      {children}
    </button>
  );
}

/** Liste der Segmente bzw. der Strecken nach RFC-0006 „Segmentliste“: Kennzahlen, Suche, Sportklassen, Tabelle. */
export function SegmentListPage({ kind }: { kind: SegmentKind }) {
  const [params, setParams] = useSearchParams();
  const filter = readFilter(params, kind);
  const navigate = useNavigate();
  const tz = useConfig().data?.data.timezone;
  const [search, setSearch] = useState(filter.q);
  const [seenQ, setSeenQ] = useState(filter.q);
  if (filter.q !== seenQ) {
    setSeenQ(filter.q);
    setSearch(filter.q);
  }

  const apply = (f: SegmentFilter) => setParams(writeFilter(f), { replace: true });

  // Suche erst nach einer kurzen Tipp-Pause an die API geben.
  useEffect(() => {
    const current = readFilter(params, kind);
    if (search.trim() === current.q.trim()) return;
    const id = setTimeout(() => setParams(writeFilter({ ...current, q: search }), { replace: true }), 300);
    return () => clearTimeout(id);
  }, [search, params, setParams, kind]);

  const list = useInfiniteQuery({
    queryKey: ["segments", "list", kind, writeFilter(filter).toString()],
    initialPageParam: 0,
    queryFn: ({ pageParam }) => apiGet<SegmentListResponse>(`/segments?${apiQuery(filter, pageParam)}`),
    getNextPageParam: (last) => (last.meta.offset + last.data.length < last.meta.total ? last.meta.offset + last.data.length : undefined),
    // Solange ein Segment noch gegen die Historie ausgewertet wird, nachladen.
    refetchInterval: (q) => (q.state.data?.pages.some((p) => p.data.some((s) => s.history.status === "running")) ? 2000 : false),
  });

  const rows = list.data?.pages.flatMap((p) => p.data) ?? [];
  const meta = list.data?.pages[0]?.meta;
  const s = meta?.summary;
  const remaining = meta ? meta.total - rows.length : 0;

  const sortTh = (field: SegmentSort, key: TextKey, align: "left" | "right") => (
    <th
      className={`cursor-pointer select-none px-3 py-2.5 text-${align} font-medium hover:text-text`}
      aria-sort={filter.sort === field ? (filter.desc ? "descending" : "ascending") : "none"}
      onClick={() => apply(toggleSort(filter, field))}
    >
      {t(key)}
      {filter.sort === field ? (filter.desc ? " ↓" : " ↑") : ""}
    </th>
  );
  const th = "px-3 py-2.5 font-medium";
  const route = kind === "route";

  return (
    <div className="grid grid-cols-1 gap-4">
      <h1 className="flex items-center gap-2 text-2xl font-bold">
        <SegmentKindIcon kind={kind} size={26} className="text-muted" />
        {t(kind === "route" ? "nav.routes" : "nav.segments")}
      </h1>

      <div className="grid grid-cols-2 gap-4 lg:grid-cols-4">
        <KpiCard label={t(`segments.kpi.count.${filter.kind}` as TextKey)} value={s ? num(s.count) : "–"} />
        <KpiCard label={t("segments.kpi.attempts")} value={s ? num(s.attempt_count) : "–"} />
        <KpiCard label={t("segments.kpi.length")} value={s ? num(s.length_m / 1000, 1) : "–"} unit="km" />
        <KpiCard label={t("segments.kpi.improvements")} value={s ? num(s.improvement_count) : "–"} />
      </div>

      <Card>
        <div className="flex flex-wrap items-center gap-2">
          <input
            type="search"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder={t(route ? "segments.searchRoute" : "segments.search")}
            aria-label={t(route ? "segments.searchRoute" : "segments.search")}
            className="min-w-[240px] max-w-[420px] flex-1 rounded-lg border border-line bg-card-2 px-3 py-1.5 text-sm"
          />
          <span className="w-2" />
          <Pill on={filter.sport === null} onClick={() => apply({ ...filter, sport: null })}>
            {t("segments.all")}
          </Pill>
          {RANKED_CLASSES.map((c) => (
            <Pill key={c} on={filter.sport === c} onClick={() => apply({ ...filter, sport: c })}>
              {t(`sport.${c}` as TextKey)}
            </Pill>
          ))}
          <span className="w-2" />
          <Pill on={filter.category === null} onClick={() => apply({ ...filter, category: null })}>
            {t("category.all")}
          </Pill>
          {categoriesFor(kind).map((c) => (
            <Pill key={c} on={filter.category === c} onClick={() => apply({ ...filter, category: c })}>
              <CategoryIcon category={c} size={14} label />
            </Pill>
          ))}
        </div>
      </Card>

      <Card className="overflow-hidden p-0">
        {list.error ? (
          <div className="p-4">
            <Problem error={list.error} />
          </div>
        ) : !list.data ? (
          <p className="p-4 text-muted">{t("state.loading")}</p>
        ) : rows.length === 0 ? (
          <p className="p-4 text-muted">{t(`segments.empty.${filter.kind}` as TextKey)}</p>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full border-collapse whitespace-nowrap text-sm">
              <thead className="text-xs text-muted">
                <tr>
                  {sortTh("name", "segments.col.name", "left")}
                  <th className={`${th} text-left`}>{t("segments.col.category")}</th>
                  {route && sortTh("location", "segments.col.location", "left")}
                  <th className={`${th} text-left`}>{t("segments.col.sport")}</th>
                  {sortTh("length_m", "segments.col.length", "right")}
                  <th className={`${th} text-right`}>{t("segments.col.gain")}</th>
                  {!route && <th className={`${th} text-right`}>{t("segments.col.grade")}</th>}
                  <th className={`${th} text-right`}>{t("segments.col.attempts")}</th>
                  <th className={`${th} text-right`}>{t("segments.col.best")}</th>
                  <th className={`${th} text-right`}>{t("segments.col.last")}</th>
                </tr>
              </thead>
              <tbody>
                {rows.map((seg) => (
                  <tr
                    key={seg.id}
                    className="cursor-pointer border-t border-line hover:bg-card-2"
                    onClick={() => navigate(segmentPath(seg.kind, seg.id), { state: { from: "list" } })}
                  >
                    <td className="max-w-[420px] px-3 py-2.5 text-left">
                      <span className="flex items-center gap-2.5">
                        <SportIcon sport={seg.sport_class} size={18} />
                        <Link
                          to={segmentPath(seg.kind, seg.id)}
                          state={{ from: "list" }}
                          className="truncate font-medium hover:text-accent"
                          onClick={(e) => e.stopPropagation()}
                        >
                          {seg.name}
                        </Link>
                        {seg.history.status === "running" && (
                          <span className="rounded-full border border-line bg-card-2 px-2 py-0.5 text-xs text-muted">{t("segments.evaluating")}</span>
                        )}
                      </span>
                    </td>
                    <td className="px-3 py-2.5 text-left text-muted">{seg.category ? <CategoryIcon category={seg.category} label /> : "–"}</td>
                    {route && <td className="max-w-[220px] truncate px-3 py-2.5 text-left text-muted">{seg.location ?? "–"}</td>}
                    <td className="px-3 py-2.5 text-left text-muted">{t(`sport.${seg.sport_class}` as TextKey)}</td>
                    <td className="num px-3 py-2.5 text-right">{km(seg.length_m)} km</td>
                    <td className="num px-3 py-2.5 text-right">{seg.elevation_gain_m == null ? "–" : `${num(seg.elevation_gain_m)} m`}</td>
                    {!route && <td className="num px-3 py-2.5 text-right">{seg.average_grade_pct == null ? "–" : `${num(seg.average_grade_pct, 1)} %`}</td>}
                    <td className="num px-3 py-2.5 text-right">{num(seg.summary.attempt_count)}</td>
                    <td className="num px-3 py-2.5 text-right">{segmentTime(seg.summary.best?.duration_ms)}</td>
                    <td className="num px-3 py-2.5 text-right">{dateOnly(seg.summary.last_attempt_time, tz)}</td>
                  </tr>
                ))}
              </tbody>
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
            {list.isFetchingNextPage ? t("state.loading") : t("segments.more", { n: num(Math.min(remaining, PAGE_SIZE)), rest: num(remaining) })}
          </button>
        </div>
      )}
    </div>
  );
}
