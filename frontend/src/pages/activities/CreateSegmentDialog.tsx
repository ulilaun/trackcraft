import { useEffect, useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link } from "react-router-dom";
import {
  apiGet,
  apiPost,
  type Activity,
  type RankedSportClass,
  type Segment,
  type SegmentCreate,
  type SegmentKind,
  type SegmentResponse,
  type SimilarRoute,
  type TrackResponse,
} from "../../api/client";
import { Dialog, primaryButton, secondaryButton } from "../../components/Dialog";
import { IconSelect } from "../../components/IconSelect";
import { Problem, problemText } from "../../components/Problem";
import { SportIcon } from "../../components/SportIcon";
import { t, type TextKey } from "../../i18n";
import { duration, num, segmentTime, speed } from "../../utils/format";
import {
  clamp,
  createProblem,
  initialRange,
  initialSport,
  makeAxis,
  passages,
  selectionPoints,
  selectionStats,
  similarSegments,
  total,
  type CreateForm,
  type Range,
  type SelPoint,
} from "./createModel";
import { DetailProfile, SelectionBar, distanceText } from "./SelectionBar";
import { fetchAll, segmentPath } from "../segments/segmentModel";
import { SelectionMap } from "./SelectionMap";
import { preventContextMenu } from "./pointerDrag";
import { RANKED_CLASSES } from "../../utils/sports";

interface Props {
  activity: Activity;
  kind: SegmentKind;
  tileUrl: string;
  attribution: string;
  minLengthM: number;
  routeEdgeM: number;
  onClose: () => void;
}

const SPORTS: RankedSportClass[] = RANKED_CLASSES;

/** Wert erst übernehmen, wenn er `ms` Millisekunden unverändert blieb (z. B. beim Verschieben der Auswahl). */
function useSettled<T>(value: T, ms: number): T {
  const [settled, setSettled] = useState(value);
  useEffect(() => {
    const h = setTimeout(() => setSettled(value), ms);
    return () => clearTimeout(h);
  }, [value, ms]);
  return settled;
}
const input = "w-full rounded-lg border border-line bg-card-2 px-2 py-1.5 text-sm text-text";

function Kpi({ label, value, wide = false }: { label: string; value: string; wide?: boolean }) {
  return (
    <div className={`rounded-xl border border-line bg-card-2 px-3 py-2 ${wide ? "col-span-2" : ""}`}>
      <div className="text-[11px] tracking-wide text-muted uppercase">{label}</div>
      <div className="num text-lg font-semibold">{value}</div>
    </div>
  );
}

/**
 * Segment oder Strecke aus einer Aktivität erstellen (RFC-0016). Zwei Einstiege, ein Aufbau: Karte, Höhenprofil mit
 * zwei gekoppelten Reglern, Kennzahlen der Auswahl, Name, Sportklasse und Beschreibung.
 */
export function CreateSegmentDialog({ activity: a, kind, tileUrl, attribution, minLengthM, routeEdgeM, onClose }: Props) {
  const [section, setSection] = useState(0);
  const track = useQuery({
    queryKey: ["activities", "track", a.id, "section", section],
    queryFn: () => apiGet<TrackResponse>(`/activities/${a.id}/track?section=${section}`),
  });
  const points = useMemo(() => {
    const s = track.data?.data.sections[0];
    return s ? selectionPoints(s) : [];
  }, [track.data]);
  const route = kind === "route";
  const [form, setForm] = useState<CreateForm>({ name: route ? a.title : "", description: "", sport: initialSport(a.sport_class), location: route ? (a.location ?? "") : "" });
  const [created, setCreated] = useState<string | null>(null);
  const title = t(route ? "create.titleRoute" : "create.titleSegment");

  const sectionChoice =
    a.section_count > 1 ? (
      <label className="mb-2 grid gap-1">
        <span className="text-xs font-medium text-muted">{t("create.section")}</span>
        <select className={input} value={section} onChange={(ev) => setSection(Number(ev.target.value))}>
          {Array.from({ length: a.section_count }, (_, k) => (
            <option key={k} value={k}>
              {t("create.sectionN", { n: k + 1 })}
            </option>
          ))}
        </select>
      </label>
    ) : null;

  if (created) return <CreatedView id={created} kind={kind} activityId={a.id} onClose={onClose} />;

  let body;
  if (track.error) body = <Problem error={track.error} />;
  else if (!track.data) body = <p className="text-muted">{t("state.loading")}</p>;
  else if (points.length < 2) body = <p className="text-muted">{t("create.noGps")}</p>;
  else
    body = (
      <Editor
        key={`${section}-${kind}`}
        activity={a}
        kind={kind}
        points={points}
        form={form}
        setForm={setForm}
        tileUrl={tileUrl}
        attribution={attribution}
        minLengthM={minLengthM}
        routeEdgeM={routeEdgeM}
        section={section}
        sectionChoice={sectionChoice}
        onCreated={setCreated}
        onClose={onClose}
        title={title}
      />
    );
  if (points.length >= 2 && track.data) return body;
  return (
    <Dialog
      title={title}
      wide
      onClose={onClose}
      footer={
        <button type="button" className={secondaryButton} onClick={onClose}>
          {t("edit.cancel")}
        </button>
      }
    >
      {sectionChoice}
      {body}
    </Dialog>
  );
}

function Editor({
  activity: a,
  kind,
  points,
  form,
  setForm,
  tileUrl,
  attribution,
  minLengthM,
  routeEdgeM,
  section,
  sectionChoice,
  onCreated,
  onClose,
  title,
}: {
  activity: Activity;
  kind: SegmentKind;
  points: SelPoint[];
  form: CreateForm;
  setForm: (f: CreateForm) => void;
  tileUrl: string;
  attribution: string;
  minLengthM: number;
  routeEdgeM: number;
  section: number;
  sectionChoice: React.ReactNode;
  onCreated: (id: string) => void;
  onClose: () => void;
  title: string;
}) {
  const route = kind === "route";
  const [range, setRangeRaw] = useState<Range>(() => initialRange(kind, points, routeEdgeM, minLengthM));
  const [hover, setHover] = useState<number | null>(null);
  const setRange = (r: Range) => setRangeRaw(clamp(kind, points, r, routeEdgeM, minLengthM));
  const axis = useMemo(() => makeAxis(kind, total(points), routeEdgeM, minLengthM), [kind, points, routeEdgeM, minLengthM]);
  const stats = selectionStats(points, range);
  const problem = createProblem(form, stats.lengthM, minLengthM);
  const [ps, pe] = [passages(points, range[0]), passages(points, range[1])];
  const queryClient = useQueryClient();
  // Hinweis auf ein fast gleiches Segment gleicher Art und Sportklasse (RFC-0016): Segmente über Start und Ziel,
  // Strecken über den Verlauf im Backend, erst wenn die Auswahl kurz ruht
  const existing = useQuery({
    queryKey: ["segments", "similar", kind, form.sport],
    enabled: !route && !!form.sport,
    queryFn: () => fetchAll<Segment>(`/segments?kind=${kind}&sport_class=${form.sport}`),
  });
  const [p0, p1] = [points[range[0]]!, points[range[1]]!];
  const settled = useSettled(`${p0.index}:${p1.index}`, 400);
  const similarRoutes = useQuery({
    queryKey: ["segments", "similar-routes", a.id, section, form.sport, settled],
    enabled: route && !!form.sport,
    queryFn: () => {
      const [start_index, end_index] = settled.split(":").map(Number);
      return apiPost<{ data: SimilarRoute[] }>("/segments/similar", {
        activity_id: a.id,
        section_index: section,
        start_index,
        end_index,
        sport_class: form.sport,
      }).then((r) => r.data);
    },
    placeholderData: (prev) => prev,
  });
  const similar: { id: string; name: string }[] = route
    ? (similarRoutes.data ?? [])
    : existing.data
      ? similarSegments(existing.data, p0, p1, stats.lengthM)
      : [];
  // Ausschnitt nur, wenn die Auswahl im Gesamtprofil schmal ist
  const showDetail = !route && stats.lengthM < 0.5 * total(points);

  const save = useMutation({
    mutationFn: () => {
      const body: SegmentCreate = {
        activity_id: a.id,
        section_index: section,
        start_index: points[range[0]]!.index,
        end_index: points[range[1]]!.index,
        name: form.name.trim(),
        description: form.description.trim() || null,
        sport_class: form.sport as RankedSportClass,
        kind,
        ...(route && form.location.trim() ? { location: form.location.trim() } : {}),
      };
      return apiPost<SegmentResponse>("/segments/from-activity", body);
    },
    onSuccess: (res) => {
      void queryClient.invalidateQueries({ queryKey: ["segments"] });
      onCreated(res.data.id);
    },
  });

  const running = form.sport === "running";
  const avg = stats.durationS && stats.durationS > 0 ? speed((stats.lengthM / stats.durationS) * 3.6, running) : null;
  const set = (patch: Partial<CreateForm>) => {
    setForm({ ...form, ...patch });
    save.reset();
  };
  const passNote = (which: "start" | "end", p: { count: number; own: number }, i: number) =>
    p.count > 1 ? (
      <li key={which}>
        {t(which === "start" ? "create.passStart" : "create.passEnd", {
          count: p.count,
          own: p.own,
          at: distanceText((points[i]?.d ?? 0) - (points[0]?.d ?? 0)),
        })}
      </li>
    ) : null;
  const notes = route ? [] : [passNote("start", ps, range[0]), passNote("end", pe, range[1])].filter(Boolean);
  const message = save.error ? (
    <span className="text-danger">{problemText(save.error)}</span>
  ) : problem ? (
    <span className="text-muted">{t(problem, { min: num(minLengthM), length: num(stats.lengthM) })}</span>
  ) : (
    <span className="text-muted">{t(route ? "create.afterRoute" : "create.afterSegment")}</span>
  );

  return (
    <Dialog
      title={title}
      wide
      busy={save.isPending}
      onClose={onClose}
      footer={
        <div className="flex w-full flex-wrap items-center gap-2">
          <div className="min-w-0 flex-1 text-sm" data-testid="create-message">
            {message}
          </div>
          <button type="button" className={secondaryButton} onClick={onClose} disabled={save.isPending}>
            {t("edit.cancel")}
          </button>
          <button type="button" className={primaryButton} disabled={!!problem || save.isPending} onClick={() => save.mutate()}>
            {save.isPending ? t("create.creating") : title}
          </button>
        </div>
      }
    >
      <p className="mb-3 text-sm text-muted">{t("create.from", { title: a.title })}</p>
      <div className="grid grid-cols-1 gap-4 lg:grid-cols-[minmax(0,1fr)_300px]">
        <div className="grid min-w-0 content-start gap-3">
          {sectionChoice}
          {/* Karte und Profil: kein Kontextmenü bei langem Druck auf Regler, Marker, Zoomknöpfe, Kacheln */}
          <div className="tc-no-callout grid min-w-0 content-start gap-3" onContextMenuCapture={preventContextMenu} data-testid="selection-controls">
            <SelectionMap points={points} range={range} onChange={setRange} hoverIndex={hover} tileUrl={tileUrl} attribution={attribution} />
            <SelectionBar points={points} axis={axis} kind={kind} range={range} onChange={setRange} onHover={setHover} />
            {showDetail && <DetailProfile points={points} range={range} onChange={setRange} onHover={setHover} />}
          </div>
        </div>
        <div className="grid content-start gap-3">
          <div className="grid grid-cols-2 gap-2">
            <Kpi label={t("create.kpi.length")} value={distanceText(stats.lengthM)} wide={route} />
            {!route && <Kpi label={t("create.kpi.grade")} value={stats.gradePct == null ? "–" : `${num(stats.gradePct, 1)} %`} />}
            <Kpi label={t("create.kpi.gain")} value={stats.gainM == null ? "–" : `${num(stats.gainM)} m`} />
            <Kpi label={t("create.kpi.loss")} value={stats.lossM == null ? "–" : `${num(stats.lossM)} m`} />
            <Kpi
              wide
              label={t("create.kpi.time")}
              value={stats.durationS == null ? "–" : `${duration(stats.durationS * 1000)}${avg ? ` · ${avg.value} ${avg.unit}` : ""}`}
            />
          </div>
          {notes.length > 0 && <ul className="rounded-lg border border-accent/30 bg-accent/10 px-3 py-2 text-sm">{notes}</ul>}
          {similar.length > 0 && (
            <div className="rounded-lg border border-warn/40 bg-warn/10 px-3 py-2 text-sm" data-testid="create-similar">
              {t(route ? "create.similar.route" : "create.similar.segment")}{" "}
              {similar.map((x, k) => (
                <span key={x.id}>
                  {k > 0 && ", "}
                  <Link to={segmentPath(kind, x.id)} className="font-medium underline hover:text-accent">
                    {x.name}
                  </Link>
                </span>
              ))}
            </div>
          )}
          <label className="grid gap-1">
            <span className="text-xs font-medium text-muted">{t("create.name")}</span>
            <input
              className={input}
              value={form.name}
              maxLength={120}
              placeholder={t(route ? "create.namePlaceholderRoute" : "create.namePlaceholder")}
              onChange={(ev) => set({ name: ev.target.value })}
            />
          </label>
          {/* eigene Auswahlliste mit Symbolen; nicht in <label> */}
          <div className="grid gap-1">
            <span className="text-xs font-medium text-muted">{t("create.sport")}</span>
            <IconSelect
              label={t("create.sport")}
              value={form.sport}
              onChange={(v) => set({ sport: v as RankedSportClass })}
              options={[
                ...(a.sport_class === "unknown" ? [{ value: "" as const, label: t("create.sportChoose") }] : []),
                ...SPORTS.map((s) => ({ value: s, label: t(`sport.${s}` as TextKey), icon: <SportIcon sport={s} decorative /> })),
              ]}
            />
            <span className="text-xs text-muted">
              {a.sport_class === "unknown" ? t("create.sportUnknown") : t("create.sportTaken", { sport: t(`sport.${a.sport_class}` as TextKey) })}
            </span>
          </div>
          {route && (
            <label className="grid gap-1">
              <span className="text-xs font-medium text-muted">{t("create.location")}</span>
              <input
                className={input}
                value={form.location}
                maxLength={120}
                placeholder={t("segmentEdit.locationPlaceholder")}
                onChange={(ev) => set({ location: ev.target.value })}
              />
            </label>
          )}
          <label className="grid gap-1">
            <span className="text-xs font-medium text-muted">{t("create.description")}</span>
            <textarea className={`${input} min-h-14`} value={form.description} onChange={(ev) => set({ description: ev.target.value })} />
          </label>
          <p className="text-xs text-muted">{t(route ? "create.fixedRoute" : "create.fixedSegment", { edge: num(routeEdgeM / 1000, 1) })}</p>
        </div>
      </div>
    </Dialog>
  );
}

/** Nach dem Anlegen: Fortschritt der Auswertung gegen die vorhandenen Aktivitäten, dann das Ergebnis. */
function CreatedView({ id, kind, activityId, onClose }: { id: string; kind: SegmentKind; activityId: string; onClose: () => void }) {
  const queryClient = useQueryClient();
  const seg = useQuery({
    queryKey: ["segments", "one", id],
    queryFn: () => apiGet<SegmentResponse>(`/segments/${id}`),
    refetchInterval: (q) => (q.state.data?.data.history.status === "running" ? 1000 : false),
  });
  const s = seg.data?.data;
  const done = s?.history.status === "complete";
  useEffect(() => {
    if (!done) return;
    void queryClient.invalidateQueries({ queryKey: ["activities", "efforts", activityId] });
    void queryClient.invalidateQueries({ queryKey: ["activities", "one", activityId] });
  }, [done, activityId, queryClient]);
  const h = s?.history;
  const pct = h && h.total_items > 0 ? Math.round((h.processed_items / h.total_items) * 100) : 0;
  return (
    <Dialog
      title={t(kind === "route" ? "create.createdRoute" : "create.createdSegment", { name: s?.name ?? "" })}
      onClose={onClose}
      footer={
        <button type="button" className={primaryButton} onClick={onClose} data-autofocus>
          {t("dialog.close")}
        </button>
      }
    >
      {seg.error ? (
        <Problem error={seg.error} />
      ) : !s || !h ? (
        <p className="text-muted">{t("state.loading")}</p>
      ) : h.status === "failed" ? (
        <p className="text-danger">{t("create.evalFailed")}</p>
      ) : !done ? (
        <div className="grid gap-2">
          <p className="text-sm">{t("create.evaluating", { done: h.processed_items, total: h.total_items })}</p>
          <div className="h-2 overflow-hidden rounded bg-card-2">
            <div className="h-full bg-accent transition-[width]" style={{ width: `${pct}%` }} />
          </div>
        </div>
      ) : (
        <p className="text-sm">
          {t("create.evaluated", { attempts: s.summary.attempt_count, valid: s.summary.valid_attempt_count })}
          {s.summary.best?.duration_ms != null && ` ${t("create.best", { time: segmentTime(s.summary.best.duration_ms) })}`}
        </p>
      )}
    </Dialog>
  );
}
