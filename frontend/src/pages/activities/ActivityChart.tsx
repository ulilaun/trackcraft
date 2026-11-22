import { memo, useMemo, useState } from "react";
import { Area, CartesianGrid, ComposedChart, Line, ReferenceArea, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import { t, type TextKey } from "../../i18n";
import { duration, num } from "../../utils/format";
import { available, chartData, nearest, zoomed, type Channel, type TrackPoint, type XAxisKind } from "./trackModel";

interface Props {
  points: TrackPoint[];
  running: boolean;
  onHover: (index: number | null) => void;
  /** Hervorgehobener Abschnitt (Segmentversuch), Indizes in `points`. */
  highlight?: [number, number] | null;
}

const SECOND = "#f43f5e";
const CHANNELS: Channel[] = ["ele", "speed", "hr"];

function channelLabel(c: Channel, running: boolean): string {
  if (c === "speed") return t(running ? "detail.chart.pace" : "detail.chart.speed");
  return t(`detail.chart.${c}` as TextKey);
}

function unit(c: Channel, running: boolean): string {
  return c === "ele" ? "m" : c === "hr" ? "bpm" : running ? "/km" : "km/h";
}

/** Achsen- und Tooltipwert; Pace in Minuten je km als `m:ss`. */
function fmt(c: Channel, running: boolean, v: number): string {
  if (c === "speed" && running) return duration(v * 60_000);
  return num(v, c === "speed" ? 1 : 0);
}

function xFmt(x: XAxisKind, v: number, precise = false): string {
  return x === "distance" ? `${num(v, precise ? 2 : 0)} km` : duration(v * 1000);
}

function Select<T extends string>(props: { label: string; value: T; options: { value: T; label: string }[]; onChange: (v: T) => void }) {
  return (
    <label className="flex items-center gap-1.5 text-xs text-muted">
      {props.label}
      <select
        className="rounded-lg border border-line bg-card-2 px-2 py-1 text-sm text-text"
        value={props.value}
        onChange={(e) => props.onChange(e.target.value as T)}
      >
        {props.options.map((o) => (
          <option key={o.value} value={o.value}>
            {o.label}
          </option>
        ))}
      </select>
    </label>
  );
}

/**
 * Verlauf mit zwei Kurven (RFC-0006): Kurve 1 als Fläche mit Achse links, Kurve 2 als Linie mit Achse rechts,
 * x über Distanz oder Zeit, Zoom durch Ziehen, „Gesamt“ setzt zurück. Hover meldet den Punkt an die Karte.
 */
export const ActivityChart = memo(function ActivityChart({ points, running, onHover, highlight = null }: Props) {
  const has = useMemo(() => Object.fromEntries(CHANNELS.map((c) => [c, available(points, c)])) as Record<Channel, boolean>, [points]);
  const firstAvailable = CHANNELS.find((c) => has[c]) ?? "ele";
  const [c1, setC1] = useState<Channel>(has.ele ? "ele" : firstAvailable);
  const [c2, setC2] = useState<Channel | "none">(has.hr ? "hr" : "none");
  const [xAxis, setXAxis] = useState<XAxisKind>(points.some((p) => p.distM != null) ? "distance" : "time");
  const [zoom, setZoom] = useState<[number, number] | null>(null);
  const [drag, setDrag] = useState<{ a: number; b: number | null } | null>(null);

  const second = c2 === "none" || c2 === c1 ? null : c2;
  const all = useMemo(() => chartData(points, xAxis, c1, second, running), [points, xAxis, c1, second, running]);
  const data = useMemo(() => zoomed(all, zoom), [all, zoom]);

  const channelOptions = CHANNELS.filter((c) => has[c]).map((c) => ({ value: c, label: channelLabel(c, running) }));
  const reversed = (c: Channel) => c === "speed" && running;
  const xAt = (i: number) => {
    const p = points[i];
    if (!p) return null;
    return xAxis === "distance" ? (p.distM != null ? p.distM / 1000 : null) : p.timeS;
  };
  const marked = highlight ? [xAt(highlight[0]), xAt(highlight[1])] : null;
  const xOf = (s: { activeLabel?: string | number | undefined }) => (s.activeLabel == null ? null : Number(s.activeLabel));

  if (all.length < 2) return <p className="text-sm text-muted">{t("detail.chart.empty")}</p>;

  type ChartState = Parameters<typeof xOf>[0];
  const dragStart = (st: ChartState) => {
    const x = xOf(st);
    if (x != null) setDrag({ a: x, b: null });
  };
  const dragMove = (st: ChartState) => {
    const x = xOf(st);
    if (x == null) return;
    onHover(nearest(data, x)?.i ?? null);
    if (drag) setDrag({ ...drag, b: x });
  };
  const dragEnd = () => {
    if (drag && drag.b != null && drag.a !== drag.b) setZoom([Math.min(drag.a, drag.b), Math.max(drag.a, drag.b)]);
    setDrag(null);
  };
  const leave = () => {
    onHover(null);
    setDrag(null);
  };

  return (
    <div>
      <div className="mb-3 flex flex-wrap items-center gap-3">
        <h2 className="text-xs font-semibold uppercase tracking-wide text-muted">{t("detail.chart.title")}</h2>
        <span className="flex-1" />
        <Select label={t("detail.chart.curve1")} value={c1} options={channelOptions} onChange={setC1} />
        <Select
          label={t("detail.chart.curve2")}
          value={c2}
          options={[...channelOptions, { value: "none" as const, label: t("detail.chart.none") }]}
          onChange={setC2}
        />
        <Select
          label={t("detail.chart.x")}
          value={xAxis}
          options={[
            { value: "distance" as const, label: t("detail.chart.overDistance") },
            { value: "time" as const, label: t("detail.chart.overTime") },
          ]}
          onChange={(v) => {
            setXAxis(v);
            setZoom(null);
          }}
        />
        <button
          type="button"
          className="rounded-lg border border-line bg-card-2 px-3 py-1 text-sm hover:border-accent disabled:opacity-40"
          disabled={!zoom}
          onClick={() => setZoom(null)}
        >
          {t("detail.chart.reset")}
        </button>
      </div>
      {/* Finger: waagrecht aufziehen zoomt, senkrecht scrollt die Seite (touch-pan-y) */}
      <div className="h-[260px] touch-pan-y select-none" data-testid="activity-chart">
        <ResponsiveContainer width="100%" height="100%">
          <ComposedChart
            data={data}
            margin={{ top: 4, right: 4, bottom: 0, left: 4 }}
            onMouseDown={dragStart}
            onMouseMove={dragMove}
            onMouseUp={dragEnd}
            onMouseLeave={leave}
            onTouchStart={dragStart}
            onTouchMove={dragMove}
            onTouchEnd={() => {
              dragEnd();
              onHover(null);
            }}
          >
            <defs>
              <linearGradient id="tc-curve1" x1="0" y1="0" x2="0" y2="1">
                <stop offset="0%" stopColor="var(--tc-accent)" stopOpacity={0.35} />
                <stop offset="100%" stopColor="var(--tc-accent)" stopOpacity={0} />
              </linearGradient>
            </defs>
            <CartesianGrid stroke="var(--tc-line)" vertical={false} />
            <XAxis
              dataKey="x"
              type="number"
              domain={["dataMin", "dataMax"]}
              allowDataOverflow
              tickFormatter={(v: number) => xFmt(xAxis, v)}
              tick={{ fill: "var(--tc-muted)", fontSize: 11 }}
              stroke="var(--tc-line)"
            />
            <YAxis
              yAxisId="a"
              orientation="left"
              reversed={reversed(c1)}
              domain={["auto", "auto"]}
              tickFormatter={(v: number) => fmt(c1, running, v)}
              tick={{ fill: "var(--tc-accent)", fontSize: 11 }}
              stroke="var(--tc-line)"
              width={48}
            />
            {second && (
              <YAxis
                yAxisId="b"
                orientation="right"
                reversed={reversed(second)}
                domain={["auto", "auto"]}
                tickFormatter={(v: number) => fmt(second, running, v)}
                tick={{ fill: SECOND, fontSize: 11 }}
                stroke="var(--tc-line)"
                width={48}
              />
            )}
            <Tooltip
              isAnimationActive={false}
              contentStyle={{ background: "var(--tc-card)", border: "1px solid var(--tc-line)", borderRadius: 10, fontSize: 12 }}
              labelFormatter={(v) => xFmt(xAxis, Number(v), true)}
              formatter={(value, name) => {
                const c = name === "a" ? c1 : second;
                if (c == null || typeof value !== "number") return ["–", ""];
                return [`${fmt(c, running, value)} ${unit(c, running)}`, channelLabel(c, running)];
              }}
            />
            <Area
              yAxisId="a"
              dataKey="a"
              name="a"
              type="monotone"
              stroke="var(--tc-accent)"
              strokeWidth={1.6}
              fill="url(#tc-curve1)"
              baseValue={reversed(c1) ? "dataMax" : "dataMin"}
              isAnimationActive={false}
              dot={false}
              activeDot={{ r: 3 }}
              connectNulls={false}
            />
            {second && (
              <Line
                yAxisId="b"
                dataKey="b"
                name="b"
                type="monotone"
                stroke={SECOND}
                strokeWidth={1.4}
                isAnimationActive={false}
                dot={false}
                activeDot={{ r: 3 }}
                connectNulls={false}
              />
            )}
            {marked && marked[0] != null && marked[1] != null && (
              <ReferenceArea yAxisId="a" x1={marked[0]} x2={marked[1]} fill="#f59e0b" fillOpacity={0.2} stroke="none" />
            )}
            {drag && drag.b != null && (
              <ReferenceArea yAxisId="a" x1={drag.a} x2={drag.b} fill="var(--tc-accent)" fillOpacity={0.15} stroke="none" />
            )}
          </ComposedChart>
        </ResponsiveContainer>
      </div>
      <p className="mt-1.5 text-xs text-muted">{t(xAxis === "distance" ? "detail.chart.hintDistance" : "detail.chart.hint")}</p>
    </div>
  );
});
