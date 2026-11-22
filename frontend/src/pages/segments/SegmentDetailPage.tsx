import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Link, Navigate, useLocation, useNavigate, useParams } from "react-router-dom";
import { ArrowLeft, ListFilter, Pencil, Trash2 } from "lucide-react";
import { ApiError, apiGet, type Effort, type SegmentResponse, type SegmentTrackResponse } from "../../api/client";
import { Card, KpiCard } from "../../components/Card";
import { useFlash } from "../../components/Flash";
import { CategoryIcon } from "../../components/CategoryIcon";
import { PeriodButton, SeasonPill, YearPill } from "../../components/PeriodButton";
import { Problem } from "../../components/Problem";
import { ActivitySportIcon, SportIcon } from "../../components/SportIcon";
import { useConfig } from "../../hooks/useConfig";
import { t, type TextKey } from "../../i18n";
import { dateOnly, km, num, segmentTime, speed } from "../../utils/format";
import { ActivityMap } from "../activities/ActivityMap";
import { Medal } from "../activities/SegmentEfforts";
import { useSeasons } from "../seasons/useSeasons";
import { ElevationProfile, ProgressionChart } from "./SegmentCharts";
import { SegmentDeleteDialog, SegmentEditDialog } from "./SegmentDialogs";
import {
  board,
  effortKmh,
  effortYears,
  fetchAll,
  lastEffort,
  listPath,
  ownEffort,
  progression,
  referencePoints,
  segmentPath,
  type BoardPeriod,
} from "./segmentModel";

/** Woher die Segmentseite geöffnet wurde (RFC-0006 „Navigation“); `activityId` hebt deren Versuch hervor. */
export interface SegmentOrigin {
  from?: "list" | "activity";
  activityId?: string;
}

/** Segmentseite nach RFC-0006 „Segmentseite“ für ein Segment oder eine Strecke. */
export function SegmentDetailPage() {
  const { id = "" } = useParams();
  const location = useLocation();
  const navigate = useNavigate();
  const origin = (location.state ?? {}) as SegmentOrigin;
  // Bis das Segment geladen ist, sagt die Adresse, ob es um eine Strecke geht.
  const pathKind = location.pathname.startsWith("/strecken") ? "route" : "segment";
  const config = useConfig();
  const cfg = config.data?.data;
  const tz = cfg?.timezone;
  const flash = useFlash();
  const [hover, setHover] = useState<number | null>(null);
  const [dialog, setDialog] = useState<"edit" | "delete" | null>(null);
  // Zeitraum: Art, gewähltes Jahr und gewählte Saison (ohne Wahl laufendes Jahr bzw. Vorgabesaison)
  const [mode, setMode] = useState<BoardPeriod["kind"]>("all");
  const [pickedYear, setPickedYear] = useState<number | null>(null);
  const [pickedSeason, setPickedSeason] = useState<string | null>(null);
  const seasons = useSeasons();
  // Versuch unter der Maus, gemeinsam für Balken und Tabellenzeile
  const [hoverEffort, setHoverEffort] = useState<string | null>(null);

  const segment = useQuery({
    queryKey: ["segments", "one", id],
    queryFn: () => apiGet<SegmentResponse>(`/segments/${id}`),
    // Bis die Historie ausgewertet ist, den Fortschritt nachladen.
    refetchInterval: (q) => (q.state.data?.data.history.status === "running" ? 1500 : false),
  });
  const s = segment.data?.data;
  const maxPoints = cfg?.map.track_max_points ?? 2000;
  const track = useQuery({
    queryKey: ["segments", "track", id, maxPoints],
    enabled: config.isSuccess,
    queryFn: () => apiGet<SegmentTrackResponse>(`/segments/${id}/track?max_points=${maxPoints}`),
  });
  // Schlüssel mit Stand der Auswertung: ist sie fertig oder kommen Versuche dazu, wird neu geladen.
  const attempts = useQuery({
    queryKey: ["segments", "attempts", id, s?.history.status, s?.history.processed_items, s?.summary.attempt_count],
    enabled: !!s,
    queryFn: () => fetchAll<Effort>(`/segments/${id}/attempts?sort=duration_ms`),
    placeholderData: (prev) => prev,
  });
  const points = useMemo(() => (track.data ? referencePoints(track.data.data) : []), [track.data]);
  const efforts = useMemo(() => attempts.data ?? [], [attempts.data]);
  const tzName = cfg?.timezone;
  const years = useMemo(() => effortYears(efforts, new Date(), tzName), [efforts, tzName]);
  const seasonList = useMemo(() => seasons.data?.data ?? [], [seasons.data]);
  const season =
    seasonList.find((x) => x.id === pickedSeason) ??
    seasonList.find((x) => x.id === seasons.data?.meta.default_season_id) ??
    seasonList[0] ??
    null;
  const year = pickedYear ?? years[0]!;
  const period: BoardPeriod = useMemo(
    () =>
      mode === "year"
        ? { kind: "year", year }
        : mode === "season" && season
          ? { kind: "season", start_date: season.start_date, end_date: season.end_date }
          : { kind: "all" },
    [mode, year, season],
  );
  const view = useMemo(() => board(efforts, period, new Date(), tzName, seasonList), [efforts, period, tzName, seasonList]);
  const progress = useMemo(
    () => progression(period.kind === "all" ? efforts : view.rows.map((r) => r.effort), origin.activityId),
    [efforts, view, period, origin.activityId],
  );

  const back = origin.from ? (
    <button type="button" className="flex items-center gap-1.5 text-sm text-muted hover:text-text" onClick={() => navigate(-1)}>
      <ArrowLeft size={14} />
      {t(origin.from === "list" ? (pathKind === "route" ? "segment.backToRouteList" : "segment.backToList") : "segment.backToActivity")}
    </button>
  ) : (
    <Link to={listPath(pathKind)} className="flex items-center gap-1.5 text-sm text-muted hover:text-text">
      <ArrowLeft size={14} />
      {t(pathKind === "route" ? "segment.toRouteList" : "segment.toList")}
    </Link>
  );

  if (segment.error) {
    const notFound = segment.error instanceof ApiError && segment.error.status === 404;
    return (
      <div className="grid gap-3">
        <div>{back}</div>
        <Card>{notFound ? <p className="text-muted">{t("segment.notFound")}</p> : <Problem error={segment.error} />}</Card>
      </div>
    );
  }
  if (!s) return <p className="text-muted">{t("state.loading")}</p>;
  // Unter der falschen Adresse geöffnet (z. B. nach einem Wechsel der Art): zur richtigen, damit die Kopfzeile stimmt.
  if (s.kind !== pathKind) return <Navigate to={segmentPath(s.kind, s.id)} replace state={location.state} />;

  const running = s.sport_class === "running";
  const avgOf = (ms: number | null | undefined) => {
    const v = speed(effortKmh(s.length_m, ms), running);
    return v.value === "–" ? "–" : `${v.value} ${v.unit}`;
  };
  const best = s.summary.best;
  const valid = s.summary.valid_attempt_count;
  const last = lastEffort(efforts);
  const own = ownEffort(efforts, origin.activityId);
  const rankSub = (e: Effort, withDate: boolean) =>
    e.rank == null
      ? t("segment.kpi.notRanked")
      : withDate
        ? t("segment.kpi.rank", { date: dateOnly(e.start_time, tz), rank: e.rank, of: valid })
        : t("segment.kpi.rankOnly", { rank: e.rank, of: valid });
  const afterDelete = () => {
    flash(t("segmentDelete.done", { name: s.name }));
    if (origin.from) navigate(-1);
    else navigate(listPath(s.kind), { replace: true });
  };
  const textButton = "flex items-center gap-1.5 rounded-lg border border-line bg-card-2 px-3 py-1.5 text-sm hover:border-accent";
  const th = "px-2 py-1.5 text-left text-xs font-medium text-muted";
  const td = "px-2 py-1.5";

  return (
    <div className="grid gap-4">
      <div>{back}</div>
      <Card>
        <div className="flex flex-wrap items-center gap-4">
          <span className="grid h-10 w-10 place-items-center rounded-xl bg-card-2">
            <SportIcon sport={s.sport_class} size={22} />
          </span>
          <div className="min-w-0 flex-1">
            <h1 className="truncate text-2xl font-bold">{s.name}</h1>
            <div className="flex flex-wrap items-center gap-x-1.5 text-sm text-muted">
              {s.category ? <CategoryIcon category={s.category} label /> : t(`segment.kind.${s.kind}` as TextKey)}
              <span>· {t(`sport.${s.sport_class}` as TextKey)}</span>
              {s.location && <span>· {s.location}</span>}
            </div>
          </div>
          <div className="flex flex-wrap items-center gap-2">
            {s.kind === "route" && (
              <Link to={`/aktivitaeten?zeitraum=alle&strecke=${s.id}&sort=total_time_ms`} className={textButton}>
                <ListFilter size={14} />
                {t("segment.compare")}
              </Link>
            )}
            <button type="button" className={textButton} onClick={() => setDialog("edit")}>
              <Pencil size={14} />
              {t("segment.edit")}
            </button>
            <button type="button" className={textButton} onClick={() => setDialog("delete")}>
              <Trash2 size={14} />
              {t("segment.delete")}
            </button>
          </div>
        </div>
        {s.description && <p className="mt-3 whitespace-pre-line text-sm">{s.description}</p>}
        {s.history.status === "running" && (
          <p className="mt-3 text-sm text-muted" role="status">
            {t("segment.history.running", { done: s.history.processed_items, total: s.history.total_items })}
          </p>
        )}
        {s.history.status === "failed" && <p className="mt-3 text-sm text-danger">{t("segment.history.failed")}</p>}
      </Card>

      <div className="grid grid-cols-2 gap-4 md:grid-cols-3 xl:grid-cols-6">
        <KpiCard label={t("segment.kpi.length")} value={km(s.length_m)} unit="km" />
        <KpiCard
          label={t("segment.kpi.gain")}
          value={s.elevation_gain_m == null ? "–" : num(s.elevation_gain_m)}
          unit="m"
          sub={s.kind === "route" || s.average_grade_pct == null ? undefined : t("segment.kpi.grade", { grade: num(s.average_grade_pct, 1) })}
        />
        <KpiCard label={t("segment.kpi.best")} value={segmentTime(best?.duration_ms)} sub={best ? avgOf(best.duration_ms) : undefined} />
        <KpiCard
          label={t("segment.kpi.attempts")}
          value={num(s.summary.attempt_count)}
          sub={[
            valid !== s.summary.attempt_count ? t("segment.kpi.attemptsValid", { valid }) : null,
            s.summary.first_attempt_time ? t("segment.kpi.since", { date: dateOnly(s.summary.first_attempt_time, tz, true) }) : null,
          ]
            .filter(Boolean)
            .join(" · ")}
        />
        <KpiCard label={t("segment.kpi.last")} value={segmentTime(last?.duration_ms)} sub={last ? rankSub(last, true) : undefined} />
        {own ? (
          <KpiCard label={t("segment.kpi.thisActivity")} value={segmentTime(own.duration_ms)} sub={rankSub(own, false)} />
        ) : (
          <KpiCard label={t("segment.kpi.average")} value={segmentTime(s.summary.average_duration_ms)} sub={avgOf(s.summary.average_duration_ms)} />
        )}
      </div>

      <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
        <Card className="p-1.5">
          {track.error ? (
            <div className="p-3">
              <Problem error={track.error} />
            </div>
          ) : !track.data || !cfg ? (
            <p className="p-3 text-muted">{t("state.loading")}</p>
          ) : (
            <ActivityMap points={points} tileUrl={cfg.map.tile_url} attribution={cfg.map.attribution} hoverIndex={hover} />
          )}
        </Card>
        <Card title={t("segment.profile")} className="grid grid-rows-[auto_1fr]">
          {track.data ? <ElevationProfile points={points} onHover={setHover} /> : <p className="text-muted">{t("state.loading")}</p>}
        </Card>
      </div>

      <div className="flex flex-wrap items-center gap-2" role="group" aria-label={t("segment.attempts.title")}>
        <PeriodButton on={period.kind === "all"} onClick={() => setMode("all")}>
          {t("segment.board.all")}
        </PeriodButton>
        {/* Jahr und Saison als geteilte Knöpfe mit Menü */}
        <YearPill
          on={period.kind === "year"}
          year={year}
          years={years}
          onPick={() => setMode("year")}
          onSelect={(y) => {
            setPickedYear(y);
            setMode("year");
          }}
        />
        {season && (
          <SeasonPill
            on={period.kind === "season"}
            season={season}
            seasons={seasonList}
            onPick={() => setMode("season")}
            onSelect={(sid) => {
              setPickedSeason(sid);
              setMode("season");
            }}
          />
        )}
      </div>

      <Card title={t("segment.progress.title")}>
        {attempts.error ? (
          <Problem error={attempts.error} />
        ) : !attempts.data ? (
          <p className="text-muted">{t("state.loading")}</p>
        ) : (
          <ProgressionChart data={progress} timeZone={tz} yearBestId={view.yearBestId} year={view.year} hoverId={hoverEffort} onHover={setHoverEffort} />
        )}
      </Card>

      <Card title={t("segment.attempts.title")}>
        {attempts.error ? (
          <Problem error={attempts.error} />
        ) : !attempts.data ? (
          <p className="text-muted">{t("state.loading")}</p>
        ) : view.rows.length === 0 ? (
          <p className="text-sm text-muted">{t("segment.attempts.none")}</p>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full whitespace-nowrap text-sm">
              <thead>
                <tr className="border-b border-line">
                  <th className={th}>{t("efforts.col.rank")}</th>
                  <th className={th}>{t("segment.attempts.col.date")}</th>
                  <th className={`${th} text-right`}>{t("efforts.col.time")}</th>
                  <th className={`${th} text-right`}>{t(running ? "efforts.col.pace" : "efforts.col.speed")}</th>
                  <th className={`${th} text-right`}>{t("efforts.col.hr")}</th>
                  <th className={th}>{t("segment.attempts.col.record")}</th>
                  <th className={`${th} text-right`} title={t("segment.board.behind")}>
                    {t("efforts.col.saved")}
                  </th>
                </tr>
              </thead>
              <tbody>
                {view.rows.map(({ effort: e, badge, rank, behindMs, hr }) => {
                  const mine = e.activity_id === origin.activityId;
                  const hovered = hoverEffort === e.id;
                  return (
                    <tr
                      key={e.id}
                      data-testid="attempt-row"
                      data-own={mine ? "true" : undefined}
                      data-hover={hovered ? "true" : undefined}
                      className={`cursor-pointer border-b border-line last:border-0 hover:bg-card-2 ${hovered ? "bg-card-2" : ""} ${
                        mine ? "outline-2 -outline-offset-2 outline-warn" : ""
                      }`}
                      onMouseEnter={() => setHoverEffort(e.id)}
                      onMouseLeave={() => setHoverEffort(null)}
                      onClick={() => navigate(`/aktivitaeten/${e.activity_id}`, { state: { from: "segment" } })}
                    >
                      <td className={td}>
                        {e.rank === 1 ? <Medal rank={1} /> : <span className="num inline-block w-7 text-center text-muted">{rank ?? "–"}</span>}
                      </td>
                      <td className={td}>
                        <span className="num">{dateOnly(e.start_time, tz)}</span>{" "}
                        {/* immer ein Symbol: Untertyp, sonst Sportklasse */}
                        {e.activity_sport_class && (
                          <span className="mr-1 inline-flex align-middle">
                            <ActivitySportIcon sport={e.activity_sport_class} subKey={e.activity_sub_sport} catalog={cfg?.sub_sport_catalog} size={16} />
                          </span>
                        )}
                        <Link
                          to={`/aktivitaeten/${e.activity_id}`}
                          state={{ from: "segment" }}
                          className="font-medium hover:text-accent"
                          onClick={(ev) => ev.stopPropagation()}
                        >
                          {e.activity_title}
                        </Link>
                        {mine && <span className="ml-2 rounded-full bg-warn/20 px-2 py-0.5 text-xs">{t("segment.attempts.own")}</span>}
                      </td>
                      <td className={`${td} num text-right`}>{segmentTime(e.duration_ms)}</td>
                      <td className={`${td} num text-right`}>{e.status === "valid" ? avgOf(e.duration_ms) : "–"}</td>
                      <td className={`${td} num text-right`}>
                        {hr && view.best?.avg_heart_rate_bpm != null && (
                          <span
                            data-testid={`hr-${hr}`}
                            className={`mr-1 text-[10px] ${hr === "up" ? "text-danger" : "text-good"}`}
                            title={t(hr === "up" ? "segment.board.hrUp" : "segment.board.hrDown", { hr: view.best.avg_heart_rate_bpm })}
                          >
                            {hr === "up" ? "▲" : "▼"}
                          </span>
                        )}
                        {e.avg_heart_rate_bpm ?? "–"} / {e.max_heart_rate_bpm ?? "–"}
                      </td>
                      <td className={td}>
                        {e.status === "valid" ? (
                          // Saison- vor Jahresbestzeit; bei der absoluten Bestzeit nichts
                          badge?.kind === "season" ? (
                            <span className="rounded-full bg-good/15 px-2 py-0.5 text-xs whitespace-nowrap text-good">
                              {t("segment.board.seasonBest", { name: badge.name })}
                            </span>
                          ) : badge?.kind === "year" ? (
                            <span className="rounded-full bg-accent/15 px-2 py-0.5 text-xs whitespace-nowrap text-accent">
                              {t("segment.board.yearBest", { year: badge.year })}
                            </span>
                          ) : null
                        ) : (
                          <span className="text-xs text-muted">{t(`efforts.notRanked.${e.not_ranked_reason ?? "time_invalid"}` as TextKey)}</span>
                        )}
                      </td>
                      <td className={`${td} num text-right text-muted`}>{behindMs ? `+${segmentTime(behindMs)}` : ""}</td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        )}
      </Card>

      {dialog === "edit" && <SegmentEditDialog segment={s} onClose={() => setDialog(null)} />}
      {dialog === "delete" && <SegmentDeleteDialog segment={s} onClose={() => setDialog(null)} onDeleted={afterDelete} />}
    </div>
  );
}
