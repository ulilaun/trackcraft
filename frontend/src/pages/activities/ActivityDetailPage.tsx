import { useMemo, useState } from "react";
import { subSportLabel } from "../../utils/sports";
import { useQuery } from "@tanstack/react-query";
import { Link, useLocation, useNavigate, useParams } from "react-router-dom";
import { ArrowLeft, ChevronLeft, ChevronRight, Flag, Pencil, Route as RouteIcon, Trash2 } from "lucide-react";
import {
  ApiError,
  apiGet,
  type Activity,
  type ActivityListResponse,
  type ActivityResponse,
  type Effort,
  type EffortListResponse,
  type TrackResponse,
} from "../../api/client";
import { Card, KpiCard } from "../../components/Card";
import { useFlash } from "../../components/Flash";
import { Problem } from "../../components/Problem";
import { ActivitySportIcon } from "../../components/SportIcon";
import { useConfig } from "../../hooks/useConfig";
import { t, type TextKey } from "../../i18n";
import { dateTime, duration, km, localDate, num, speed } from "../../utils/format";
import { ActivityChart } from "./ActivityChart";
import { ActivityMap } from "./ActivityMap";
import { CreateSegmentDialog } from "./CreateSegmentDialog";
import { DeleteDialog } from "./DeleteDialog";
import { EditDialog } from "./EditDialog";
import { LoadCard } from "./LoadCard";
import { PlannedUnitCard } from "./PlannedUnitCard";
import { EffortTable, TopResults } from "./SegmentEfforts";
import { hasPosition, mergeSections, pointRange, trackStartMs } from "./trackModel";

/** Woher die Detailseite geöffnet wurde (RFC-0006 „Navigation“). */
export interface DetailOrigin {
  from?: "list" | "dashboard" | "segment";
}

/** Nachbarn in zeitlicher Reihenfolge über alle Aktivitäten: ältere (‹) und neuere (›). */
function useNeighbours(a: Activity | undefined, timeZone: string | undefined) {
  const day = a ? localDate(new Date(a.start_time), timeZone) : "";
  const older = useQuery({
    queryKey: ["activities", "neighbour", "older", a?.id],
    enabled: !!a && timeZone !== undefined,
    queryFn: () => apiGet<ActivityListResponse>(`/activities?to=${day}&sort=-start_time&limit=50`),
  });
  const newer = useQuery({
    queryKey: ["activities", "neighbour", "newer", a?.id],
    enabled: !!a && timeZone !== undefined,
    queryFn: () => apiGet<ActivityListResponse>(`/activities?from=${day}&sort=start_time&limit=50`),
  });
  const after = (r: ActivityListResponse | undefined) => {
    const list = r?.data ?? [];
    const i = list.findIndex((x) => x.id === a?.id);
    return i >= 0 ? list[i + 1] : undefined;
  };
  return { older: after(older.data), newer: after(newer.data) };
}

function originText(origin: string | null, key: "elevation" | "distance" | "time"): string | undefined {
  if (!origin) return undefined;
  const k = `detail.origin.${key}.${origin}` as TextKey;
  const text = t(k);
  return text === k ? undefined : text;
}

/** Aktivitätsdetail nach RFC-0006 „Aktivitätsdetail“ mit Segmentversuchen, Top-Ergebnissen und Segment erstellen (RFC-0016). */
export function ActivityDetailPage() {
  const { id = "" } = useParams();
  const location = useLocation();
  const navigate = useNavigate();
  const origin = (location.state ?? {}) as DetailOrigin;
  const config = useConfig();
  const cfg = config.data?.data;
  const [hover, setHover] = useState<number | null>(null);
  const [dialog, setDialog] = useState<"edit" | "delete" | "segment" | "route" | null>(null);
  const [hoverEffort, setHoverEffort] = useState<Effort | null>(null);
  const flash = useFlash();

  const activity = useQuery({ queryKey: ["activities", "one", id], queryFn: () => apiGet<ActivityResponse>(`/activities/${id}`) });
  const maxPoints = cfg?.map.track_max_points ?? 2000;
  const track = useQuery({
    queryKey: ["activities", "track", id, maxPoints],
    enabled: config.isSuccess,
    queryFn: () => apiGet<TrackResponse>(`/activities/${id}/track?max_points=${maxPoints}`),
  });
  const efforts = useQuery({ queryKey: ["activities", "efforts", id], queryFn: () => apiGet<EffortListResponse>(`/activities/${id}/efforts`) });
  const a = activity.data?.data;
  const { older, newer } = useNeighbours(a, cfg?.timezone);
  const points = useMemo(() => (track.data ? mergeSections(track.data.data.sections) : []), [track.data]);
  const t0 = useMemo(() => (track.data ? trackStartMs(track.data.data.sections) : null), [track.data]);
  const highlight = useMemo(
    () => (hoverEffort ? pointRange(points, t0, hoverEffort.start_time, hoverEffort.end_time) : null),
    [hoverEffort, points, t0],
  );
  const effortList = efforts.data?.data ?? [];
  const matchedRoute = effortList.find((e) => e.segment?.kind === "route");

  const back =
    origin.from ? (
      <button type="button" className="flex items-center gap-1.5 text-sm text-muted hover:text-text" onClick={() => navigate(-1)}>
        <ArrowLeft size={14} />
        {t(origin.from === "list" ? "detail.backToList" : origin.from === "segment" ? "detail.backToSegment" : "detail.backToDashboard")}
      </button>
    ) : (
      <Link to="/aktivitaeten" className="flex items-center gap-1.5 text-sm text-muted hover:text-text">
        <ArrowLeft size={14} />
        {t("detail.toList")}
      </Link>
    );

  if (activity.error) {
    const notFound = activity.error instanceof ApiError && activity.error.status === 404;
    return (
      <div className="grid grid-cols-1 gap-3">
        <div>{back}</div>
        <Card>{notFound ? <p className="text-muted">{t("detail.notFound")}</p> : <Problem error={activity.error} />}</Card>
      </div>
    );
  }
  if (!a) return <p className="text-muted">{t("state.loading")}</p>;

  const running = a.sport_class === "running";
  const avg = speed(a.avg_speed_kmh, running);
  const afterDelete = () => {
    flash(t("delete.done", { title: a.title }));
    if (origin.from) navigate(-1);
    else navigate("/aktivitaeten", { replace: true });
  };
  const goTo = (target: Activity | undefined) => target && navigate(`/aktivitaeten/${target.id}`, { replace: true, state: origin });
  const iconButton = "rounded-lg border border-line bg-card-2 p-2 hover:border-accent disabled:opacity-40 disabled:hover:border-line";
  const textButton =
    "flex items-center gap-1.5 rounded-lg border border-line bg-card-2 px-3 py-1.5 text-sm hover:border-accent disabled:opacity-40 disabled:hover:border-line";

  return (
    <div className="grid grid-cols-1 gap-4">
      <div>{back}</div>
      <Card>
        <div className="flex flex-wrap items-center gap-4">
          <span className="grid h-10 w-10 place-items-center rounded-xl bg-card-2">
            <ActivitySportIcon sport={a.sport_class} subKey={a.sub_sport} catalog={cfg?.sub_sport_catalog} size={22} />
          </span>
          <div className="min-w-0 flex-1">
            <h1 className="truncate text-2xl font-bold">{a.title}</h1>
            <div className="flex flex-wrap items-center gap-x-1 text-sm text-muted" data-testid="detail-meta">
              {[
                a.start_time_origin === "manual_corrected"
                  ? `${dateTime(a.start_time, cfg?.timezone, true)} ${t("detail.corrected")}`
                  : dateTime(a.start_time, cfg?.timezone, true),
                t(`sport.${a.sport_class}` as TextKey),
                // Untertyp nur als Text, das Symbol steht groß im Kopf
                a.sub_sport ? subSportLabel(cfg?.sub_sport_catalog, a.sport_class, a.sub_sport) : null,
                a.location,
              ]
                .filter(Boolean)
                .map((x, i) => (
                  <span key={i} className="inline-flex items-center gap-1">
                    {i > 0 && <span>·</span>}
                    {x}
                  </span>
                ))}
            </div>
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <button type="button" className={iconButton} disabled={!older} onClick={() => goTo(older)} title={t("detail.older")} aria-label={t("detail.older")}>
              <ChevronLeft size={16} />
            </button>
            <button type="button" className={iconButton} disabled={!newer} onClick={() => goTo(newer)} title={t("detail.newer")} aria-label={t("detail.newer")}>
              <ChevronRight size={16} />
            </button>
            <button type="button" className={textButton} onClick={() => setDialog("edit")}>
              <Pencil size={14} />
              {t("detail.edit")}
            </button>
            <button
              type="button"
              className={textButton}
              disabled={!a.has_position || !cfg}
              title={a.has_position ? undefined : t("create.noGps")}
              onClick={() => setDialog("segment")}
            >
              <Flag size={14} />
              {t("detail.createSegment")}
            </button>
            <span className="flex flex-col items-end gap-0.5">
              <button
                type="button"
                className={textButton}
                disabled={!a.has_position || !cfg || !!matchedRoute || !efforts.isSuccess}
                title={a.has_position ? undefined : t("create.noGps")}
                onClick={() => setDialog("route")}
              >
                <RouteIcon size={14} />
                {t("detail.createRoute")}
              </button>
              {matchedRoute && (
                <span className="text-xs text-muted">
                  {t("detail.routeMatched")}{" "}
                  <Link to={`/strecken/${matchedRoute.segment_id}`} state={{ from: "activity", activityId: a.id }} className="text-accent hover:underline">
                    „{matchedRoute.segment_name}“
                  </Link>
                </span>
              )}
            </span>
            <button type="button" className={textButton} onClick={() => setDialog("delete")}>
              <Trash2 size={14} />
              {t("detail.delete")}
            </button>
          </div>
        </div>
      </Card>

      <TopResults efforts={effortList} />

      <div className="grid grid-cols-1 gap-4 lg:grid-cols-12">
        <div className="grid grid-cols-2 content-start gap-4 lg:col-span-5">
          <KpiCard label={t("detail.kpi.distance")} value={km(a.distance_m)} unit="km" sub={originText(a.distance_origin, "distance")} />
          <KpiCard label={t("detail.kpi.time")} value={duration(a.total_time_ms)} sub={originText(a.total_time_origin, "time")} />
          <KpiCard label={t(running ? "detail.kpi.pace" : "detail.kpi.avg")} value={avg.value} unit={avg.unit} />
          <KpiCard
            label={t("detail.kpi.gain")}
            value={a.elevation_gain_m == null ? "–" : num(a.elevation_gain_m)}
            unit="m"
            sub={originText(a.elevation_origin, "elevation")}
          />
          <KpiCard label={t("detail.kpi.hr")} value={a.avg_heart_rate_bpm ?? "–"} unit="bpm" />
          <KpiCard
            label={t("detail.kpi.load")}
            value={a.training_load ? num(a.training_load.trimp) : "–"}
            sub={
              a.training_load &&
              (a.training_load.method === "estimated"
                ? t("form.estimated")
                : t("detail.kpi.loadSub", { aerobic: num(a.training_load.aerobic), anaerobic: num(a.training_load.anaerobic) }))
            }
          />
          {/* Segmente flach über beide Spalten (Uli 22.11.2026) */}
          <section className="col-span-2 flex items-baseline gap-3 rounded-card border border-line bg-card px-4 py-2.5 shadow-sm" data-testid="kpi-segments">
            <span className="text-xs font-medium text-muted">{t("detail.kpi.segments")}</span>
            <span className="num text-lg font-semibold">{efforts.isSuccess ? effortList.length : "–"}</span>
            <span className="text-xs text-muted">
              {efforts.data?.meta.calculation.status === "running"
                ? t("efforts.calculating")
                : t("detail.kpi.segmentsPrs", { count: a.segment_success_count })}
            </span>
          </section>
        </div>
        <Card className="p-1.5 lg:col-span-7">
          {track.error ? (
            <div className="p-3">
              <Problem error={track.error} />
            </div>
          ) : !track.data || !cfg ? (
            <p className="p-3 text-muted">{t("state.loading")}</p>
          ) : hasPosition(points) ? (
            <ActivityMap points={points} tileUrl={cfg.map.tile_url} attribution={cfg.map.attribution} hoverIndex={hover} highlight={highlight} />
          ) : (
            <p className="grid min-h-[200px] place-items-center text-sm text-muted">{t("detail.noGps")}</p>
          )}
        </Card>
      </div>

      {track.data && points.length > 0 && (
        <Card>
          <ActivityChart key={a.id} points={points} running={running} onHover={setHover} highlight={highlight} />
        </Card>
      )}

      <PlannedUnitCard activityId={a.id} />

      <LoadCard load={a.training_load} />

      {efforts.error ? (
        <Card>
          <Problem error={efforts.error} />
        </Card>
      ) : (
        efforts.isSuccess && (
          <EffortTable
            efforts={effortList}
            running={running}
            calculating={efforts.data.meta.calculation.status === "running"}
            onHover={setHoverEffort}
          />
        )
      )}

      <Card title={t("detail.description")}>
        {a.description ? <p className="whitespace-pre-line">{a.description}</p> : <p className="text-sm text-muted">{t("detail.noDescription")}</p>}
      </Card>

      {dialog === "edit" && (
        <EditDialog activity={a} timeZone={cfg?.timezone} catalog={cfg?.sub_sport_catalog} onClose={() => setDialog(null)} />
      )}
      {(dialog === "segment" || dialog === "route") && cfg && (
        <CreateSegmentDialog
          activity={a}
          kind={dialog}
          tileUrl={cfg.map.tile_url}
          attribution={cfg.map.attribution}
          minLengthM={cfg.segments.minimum_length_m}
          routeEdgeM={cfg.segments.route_edge_m}
          onClose={() => setDialog(null)}
        />
      )}
      {dialog === "delete" && (
        <DeleteDialog activity={a} timeZone={cfg?.timezone} onClose={() => setDialog(null)} onDeleted={afterDelete} />
      )}
    </div>
  );
}
