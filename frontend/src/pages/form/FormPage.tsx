import { useMemo, useState, type ReactNode } from "react";
import { useQuery } from "@tanstack/react-query";
import { Link, useSearchParams } from "react-router-dom";
import { ArrowDown, ArrowUp, Settings2 } from "lucide-react";
import {
  Area,
  Bar,
  BarChart,
  CartesianGrid,
  ComposedChart,
  Line,
  ReferenceArea,
  ReferenceLine,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import { apiGet, type ForecastMode, type FormResponse, type FormSnapshot, type PlanGoal, type StatisticsPeriodsResponse } from "../../api/client";
import { Card } from "../../components/Card";
import { PeriodButton, SeasonPill, YearPill, periodSelect } from "../../components/PeriodButton";
import { Problem } from "../../components/Problem";
import { useConfig } from "../../hooks/useConfig";
import { t, type TextKey } from "../../i18n";
import { localDate, num } from "../../utils/format";
import { periodDays, rangeFrom, readPeriod, writePeriod, yearChoices, type ListPeriod } from "../activities/listModel";
import { useSeasons } from "../seasons/useSeasons";
import { germanDate } from "../seasons/seasonModel";
import { FormSettingsDialog } from "./FormSettingsDialog";
import {
  FORECAST_DAYS,
  MONOTONY_AXIS_MAX,
  STATE_CLASS,
  axisTicks,
  chartPoints,
  fosterPoints,
  formState,
  signed,
  trend,
  weekBars,
  zeroOffset,
  type ChartPoint,
  type FosterPoint,
  type WeekBar,
} from "./formModel";

const MONTHS = ["Jan", "Feb", "Mär", "Apr", "Mai", "Jun", "Jul", "Aug", "Sep", "Okt", "Nov", "Dez"];
/** Gemeinsame Zeitachse: Überfahren zeigt in allen Tagesdiagrammen denselben Tag (#25). */
const SYNC = "form-days";
/** Schalter „aerob/anaerob einblenden“ wird im Browser gemerkt (Uli 23.11.2026). */
const PARTS_KEY = "trackcraft.form.fatigueParts";

/** Achsenbeschriftung: Tag.Monat, bei langen Zeiträumen Monat bzw. Jahr. */
function tickOf(span: number) {
  return (d: string) => {
    if (span > 3 * 365) return d.slice(0, 4);
    if (span > 120) return `${MONTHS[Number(d.slice(5, 7)) - 1]}${d.slice(5, 7) === "01" ? ` ${d.slice(2, 4)}` : ""}`;
    return `${d.slice(8, 10)}.${d.slice(5, 7)}.`;
  };
}

function readForecast(p: URLSearchParams): ForecastMode | null {
  const v = p.get("vorschau");
  if (v === "aus") return null;
  if (v === "plan") return "plan";
  return v === "wie-bisher" ? "steady" : "rest";
}

/** Zieltermin als senkrechte Linie mit Fahne (E8), nur wenn der Tag im Diagramm liegt. */
function GoalLine({ goal, points, yAxisId }: { goal?: PlanGoal | null; points: { date: string }[]; yAxisId?: string }) {
  if (!goal || !points.some((p) => p.date === goal.date)) return null;
  return (
    <ReferenceLine
      x={goal.date}
      yAxisId={yAxisId}
      stroke="var(--color-accent)"
      strokeWidth={2}
      label={{ value: `🏁 ${goal.name}`, position: "insideTopLeft", fill: "var(--color-accent)", fontSize: 11 }}
    />
  );
}

function readParts(): boolean {
  try {
    return window.localStorage.getItem(PARTS_KEY) === "1";
  } catch {
    return false;
  }
}

function storeParts(on: boolean) {
  try {
    window.localStorage.setItem(PARTS_KEY, on ? "1" : "0");
  } catch {
    // Ohne Speicher gilt die Wahl nur bis zum Neuladen
  }
}

// ---------------------------------------------------------------------------------------------------------
// Kachelreihen (#25): aktueller Wert, Änderung zu vor einem Monat und einer Woche, Vorschau in einer Woche

function Delta({ delta, digits = 0 }: { delta: number | null; digits?: number }) {
  if (delta == null) return <span className="text-muted">–</span>;
  const { dir, abs } = trend(delta);
  if (dir === "flat") return <span aria-label="±0">±0</span>;
  const Icon = dir === "up" ? ArrowUp : ArrowDown;
  return (
    <span className="inline-flex items-center gap-0.5" aria-label={signed(delta)}>
      <Icon size={14} aria-hidden="true" className="text-muted" />
      {num(abs, digits)}
    </span>
  );
}

type CompareKey = "now" | "month" | "week" | "ahead";
const COLUMNS: CompareKey[] = ["now", "month", "week", "ahead"];

interface TileRow {
  label: TextKey;
  className?: string;
  now: ReactNode;
  month: number | null;
  week: number | null;
  ahead: number | null;
  digits?: number;
}

/** Kopf mit „Gegenwärtig · Monat · Woche · +1 Woche“, je Zeile ein Wert und drei Änderungen. */
function Tiles({ rows, testId }: { rows: TileRow[]; testId: string }) {
  return (
    <div className="grid grid-cols-[auto_repeat(4,minmax(0,1fr))] gap-px overflow-hidden rounded-lg border border-line bg-line text-center" data-testid={testId}>
      <div className="bg-card-2" />
      {COLUMNS.map((k) => (
        <div key={k} className="bg-card-2 px-1 py-1 text-[11px] font-medium text-muted">
          {t(`form.compare.${k}` as TextKey)}
        </div>
      ))}
      {rows.map((r) => (
        <div key={r.label} className="contents">
          <div className={`bg-card px-2 py-2 text-left text-xs font-medium ${r.className ?? ""}`}>{t(r.label)}</div>
          <div className="num bg-card px-1 py-2 text-lg font-semibold">{r.now}</div>
          <div className="num bg-card px-1 py-2 text-lg font-semibold">
            <Delta delta={r.month} digits={r.digits} />
          </div>
          <div className="num bg-card px-1 py-2 text-lg font-semibold">
            <Delta delta={r.week} digits={r.digits} />
          </div>
          <div className="num bg-card px-1 py-2 text-lg font-semibold">
            <Delta delta={r.ahead} digits={r.digits} />
          </div>
        </div>
      ))}
    </div>
  );
}

/** Änderungen eines Werts: jetzt minus vor einem Monat bzw. einer Woche, in einer Woche minus jetzt. */
function deltas(now: number | null, c: { month_ago: FormSnapshot; week_ago: FormSnapshot; week_ahead: FormSnapshot | null }, key: keyof FormSnapshot) {
  const v = (s: FormSnapshot | null) => (s == null ? null : (s[key] as number | null));
  const diff = (a: number | null, b: number | null) => (a == null || b == null ? null : a - b);
  return { month: diff(now, v(c.month_ago)), week: diff(now, v(c.week_ago)), ahead: diff(v(c.week_ahead), now) };
}

// ---------------------------------------------------------------------------------------------------------
// Diagramme

const axisProps = { tick: { fill: "var(--tc-muted)", fontSize: 11 }, stroke: "var(--tc-line)", tickLine: false } as const;

function DayAxis({ points }: { points: { date: string }[] }) {
  const ticks = axisTicks(points.map((p) => p.date));
  return <XAxis dataKey="date" ticks={ticks} interval={ticks ? 0 : "preserveEnd"} tickFormatter={tickOf(points.length)} minTickGap={24} {...axisProps} />;
}

function TipBox({ date, forecast, children }: { date: string; forecast: boolean; children: ReactNode }) {
  return (
    <div className="rounded-lg border border-line bg-card px-3 py-2 text-xs shadow">
      <div className="mb-1 font-semibold">
        {germanDate(date)}
        {forecast && <span className="ml-1 font-normal text-muted">· {t("form.forecastLabel")}</span>}
      </div>
      {children}
    </div>
  );
}

function TipRow({ label, value, color }: { label: TextKey; value: string | null; color: string }) {
  if (value == null) return null;
  return (
    <div className="flex justify-between gap-4">
      <span style={{ color }}>{t(label)}</span>
      <span className="num">{value}</span>
    </div>
  );
}

const n = (v: number | undefined) => (v == null ? null : num(v));

function LoadTooltip({ active, payload, parts }: { active?: boolean; payload?: { payload: ChartPoint }[]; parts: boolean }) {
  const p = payload?.[0]?.payload;
  if (!active || !p) return null;
  return (
    <TipBox date={p.date} forecast={p.fitness == null}>
      <TipRow label="form.fitness" value={n(p.fitness ?? p.fitnessForecast)} color="var(--color-fitness)" />
      <TipRow label="form.fatigue" value={n(p.fatigue ?? p.fatigueForecast)} color="var(--color-fatigue)" />
      {parts && <TipRow label="form.fatigueAerobic" value={n(p.fatigueAerobic ?? p.fatigueAerobicForecast)} color="var(--color-aerobic)" />}
      {parts && <TipRow label="form.fatigueAnaerobic" value={n(p.fatigueAnaerobic ?? p.fatigueAnaerobicForecast)} color="var(--color-anaerobic)" />}
      {p.fitness != null && <TipRow label="form.load" value={n(p.load)} color="var(--tc-muted)" />}
    </TipBox>
  );
}

/** Diagramm 1: Fitness und Ermüdung als Flächen; im Zwei-Kurven-Modell auf Wunsch die beiden Teile. */
export function LoadChart({ points, parts, goal }: { points: ChartPoint[]; parts: boolean; goal?: PlanGoal | null }) {
  return (
    <div className="h-64" data-testid="form-load-chart">
      <ResponsiveContainer width="100%" height="100%">
        <ComposedChart data={points} syncId={SYNC} margin={{ top: 6, right: 6, bottom: 0, left: -14 }}>
          <CartesianGrid stroke="var(--tc-line)" vertical={false} />
          <DayAxis points={points} />
          <YAxis {...axisProps} width={44} allowDecimals={false} />
          <Tooltip content={<LoadTooltip parts={parts} />} isAnimationActive={false} />
          <GoalLine goal={goal} points={points} />
          <Area dataKey="fitness" stroke="var(--color-fitness)" strokeWidth={2} fill="var(--color-fitness)" fillOpacity={0.28} isAnimationActive={false} />
          <Area dataKey="fatigue" stroke="var(--color-fatigue)" strokeWidth={1.6} fill="var(--color-fatigue)" fillOpacity={0.28} isAnimationActive={false} />
          <Area
            dataKey="fitnessForecast"
            stroke="var(--color-fitness)"
            strokeWidth={2}
            strokeDasharray="4 3"
            fill="var(--color-fitness)"
            fillOpacity={0.1}
            isAnimationActive={false}
          />
          <Area
            dataKey="fatigueForecast"
            stroke="var(--color-fatigue)"
            strokeWidth={1.6}
            strokeDasharray="4 3"
            fill="var(--color-fatigue)"
            fillOpacity={0.1}
            isAnimationActive={false}
          />
          {parts && (
            <>
              <Line dataKey="fatigueAerobic" stroke="var(--color-aerobic)" strokeWidth={1} dot={false} isAnimationActive={false} />
              <Line dataKey="fatigueAerobicForecast" stroke="var(--color-aerobic)" strokeWidth={1} strokeDasharray="4 3" dot={false} isAnimationActive={false} />
              <Line dataKey="fatigueAnaerobic" stroke="var(--color-anaerobic)" strokeWidth={1} dot={false} isAnimationActive={false} />
              <Line
                dataKey="fatigueAnaerobicForecast"
                stroke="var(--color-anaerobic)"
                strokeWidth={1}
                strokeDasharray="4 3"
                dot={false}
                isAnimationActive={false}
              />
            </>
          )}
        </ComposedChart>
      </ResponsiveContainer>
    </div>
  );
}

function FormTooltip({ active, payload }: { active?: boolean; payload?: { payload: ChartPoint }[] }) {
  const p = payload?.[0]?.payload;
  if (!active || !p) return null;
  const v = p.form ?? p.formForecast;
  return (
    <TipBox date={p.date} forecast={p.form == null}>
      <TipRow label="form.form" value={v == null ? null : signed(v)} color="var(--tc-text)" />
      {v != null && <div className="text-muted">{t(`form.state.${formState(v)}` as TextKey)}</div>}
    </TipBox>
  );
}

/** Diagramm 2: nur die Form, über null grün, darunter orange; Vorschau heller. */
export function FormChart({ points, goal }: { points: ChartPoint[]; goal?: PlanGoal | null }) {
  const offset = zeroOffset(points);
  return (
    <div className="h-64" data-testid="form-form-chart">
      <ResponsiveContainer width="100%" height="100%">
        <ComposedChart data={points} syncId={SYNC} margin={{ top: 6, right: 6, bottom: 0, left: -14 }}>
          <defs>
            <linearGradient id="formFill" x1="0" y1="0" x2="0" y2="1">
              <stop offset={offset} stopColor="var(--color-good)" stopOpacity={0.35} />
              <stop offset={offset} stopColor="var(--color-warn)" stopOpacity={0.35} />
            </linearGradient>
          </defs>
          <CartesianGrid stroke="var(--tc-line)" vertical={false} />
          <DayAxis points={points} />
          <YAxis {...axisProps} width={44} allowDecimals={false} />
          <Tooltip content={<FormTooltip />} isAnimationActive={false} />
          <GoalLine goal={goal} points={points} />
          <ReferenceLine y={0} stroke="var(--tc-muted)" strokeOpacity={0.6} />
          <Area dataKey="form" stroke="var(--color-good)" strokeWidth={1.4} fill="url(#formFill)" isAnimationActive={false} connectNulls={false} />
          <Area
            dataKey="formForecast"
            stroke="var(--color-good)"
            strokeWidth={1.4}
            strokeDasharray="4 3"
            fill="url(#formFill)"
            fillOpacity={0.45}
            isAnimationActive={false}
          />
        </ComposedChart>
      </ResponsiveContainer>
    </div>
  );
}

function FosterTooltip({ active, payload }: { active?: boolean; payload?: { payload: FosterPoint }[] }) {
  const p = payload?.[0]?.payload;
  if (!active || !p) return null;
  const m = p.monotonyRaw ?? p.monotony ?? p.monotonyForecast;
  const s = p.strain ?? p.strainForecast;
  return (
    <TipBox date={p.date} forecast={p.monotony == null && p.strain == null}>
      <TipRow label="form.monotony" value={m == null ? null : num(m, 2)} color="var(--tc-text)" />
      <TipRow label="form.strain" value={s == null ? null : num(s)} color="var(--tc-muted)" />
    </TipBox>
  );
}

/** Diagramm 3: Monotonie (links, gekappt bei 4, Warnbereich ab 2) und Strain (rechts) nach Foster. */
function FosterChart({ points, warn, goal }: { points: FosterPoint[]; warn: number; goal?: PlanGoal | null }) {
  return (
    <div className="h-56" data-testid="form-foster-chart">
      <ResponsiveContainer width="100%" height="100%">
        <ComposedChart data={points} syncId={SYNC} margin={{ top: 6, right: 0, bottom: 0, left: -14 }}>
          <CartesianGrid stroke="var(--tc-line)" vertical={false} />
          <DayAxis points={points} />
          <YAxis yAxisId="m" {...axisProps} width={44} domain={[0, MONOTONY_AXIS_MAX]} ticks={[0, 1, 2, 3, 4]} />
          <YAxis yAxisId="s" orientation="right" {...axisProps} width={48} allowDecimals={false} />
          <Tooltip content={<FosterTooltip />} isAnimationActive={false} />
          <GoalLine goal={goal} points={points} yAxisId="m" />
          <ReferenceArea yAxisId="m" y1={warn} y2={MONOTONY_AXIS_MAX} fill="var(--color-danger)" fillOpacity={0.08} ifOverflow="hidden" />
          <ReferenceLine yAxisId="m" y={warn} stroke="var(--color-danger)" strokeOpacity={0.5} strokeDasharray="3 3" />
          <Area yAxisId="s" dataKey="strain" stroke="none" fill="var(--tc-muted)" fillOpacity={0.22} isAnimationActive={false} />
          <Area yAxisId="s" dataKey="strainForecast" stroke="none" fill="var(--tc-muted)" fillOpacity={0.1} isAnimationActive={false} />
          <Line yAxisId="m" dataKey="monotony" stroke="var(--tc-text)" strokeWidth={1.4} dot={false} isAnimationActive={false} connectNulls={false} />
          <Line
            yAxisId="m"
            dataKey="monotonyForecast"
            stroke="var(--tc-text)"
            strokeWidth={1.4}
            strokeDasharray="4 3"
            dot={false}
            isAnimationActive={false}
          />
          <Line yAxisId="m" dataKey="monotonyHigh" stroke="var(--color-danger)" strokeWidth={2} dot={false} isAnimationActive={false} connectNulls={false} />
          <Line
            yAxisId="m"
            dataKey={(p: FosterPoint) => (p.monotonyRaw != null ? MONOTONY_AXIS_MAX : undefined)}
            name="clipped"
            stroke="none"
            dot={{ r: 3, fill: "var(--color-danger)", stroke: "none" }}
            isAnimationActive={false}
            legendType="none"
          />
        </ComposedChart>
      </ResponsiveContainer>
    </div>
  );
}

function WeekTooltip({ active, payload }: { active?: boolean; payload?: { payload: WeekBar }[] }) {
  const w = payload?.[0]?.payload;
  if (!active || !w) return null;
  return (
    <div className="rounded-lg border border-line bg-card px-3 py-2 text-xs shadow">
      <div className="mb-1 font-semibold">{t("form.weekOf", { day: germanDate(w.week) })}</div>
      {(
        [
          ["form.aerobic", w.aerobic, "var(--color-aerobic)"],
          ["form.anaerobic", w.anaerobic, "var(--color-anaerobic)"],
          ["form.estimated", w.estimated, "var(--tc-muted)"],
        ] as const
      )
        .filter(([, v]) => v > 0)
        .map(([k, v, c]) => (
          <div key={k} className="flex justify-between gap-4">
            <span style={{ color: c }}>{t(k)}</span>
            <span className="num">{num(v)}</span>
          </div>
        ))}
      <div className="mt-1 flex justify-between gap-4 border-t border-line pt-1 font-semibold">
        <span>{t("form.load")}</span>
        <span className="num">{num(w.total)}</span>
      </div>
    </div>
  );
}

/**
 * Wochenbalken: bei vielen Wochen nur die erste Woche jedes Monats (über drei Jahre: jedes Jahres) beschriften,
 * bei vielen Wechseln nur jeden zweiten bzw. dritten – das Diagramm hat seit #25 nur die halbe Breite.
 */
export function weekTicks(bars: WeekBar[]): string[] | undefined {
  if (bars.length <= 17) return undefined;
  const key = (w: string) => (bars.length * 7 > 3 * 365 ? w.slice(0, 4) : w.slice(0, 7));
  const changes = bars.filter((b, i) => i > 0 && key(b.week) !== key(bars[i - 1]!.week)).map((b) => b.week);
  const step = changes.length > 24 ? 3 : changes.length > 12 ? 2 : 1;
  return changes.filter((_, i) => i % step === 0);
}

function Weeks({ bars }: { bars: WeekBar[] }) {
  return (
    <div className="h-36" data-testid="form-weeks">
      <ResponsiveContainer width="100%" height="100%">
        <BarChart data={bars} margin={{ top: 4, right: 6, bottom: 0, left: -14 }}>
          <defs>
            <pattern id="hatch" width="4" height="4" patternUnits="userSpaceOnUse" patternTransform="rotate(45)">
              <rect width="2" height="4" fill="var(--tc-muted)" fillOpacity={0.7} />
            </pattern>
          </defs>
          <CartesianGrid stroke="var(--tc-line)" vertical={false} />
          <XAxis
            dataKey="week"
            ticks={weekTicks(bars)}
            interval={bars.length > 17 ? 0 : "preserveEnd"}
            tickFormatter={tickOf(bars.length * 7)}
            minTickGap={24}
            tick={{ fill: "var(--tc-muted)", fontSize: 11 }}
            stroke="var(--tc-line)"
            tickLine={false}
          />
          <YAxis tick={{ fill: "var(--tc-muted)", fontSize: 11 }} stroke="var(--tc-line)" tickLine={false} width={44} allowDecimals={false} />
          <Tooltip cursor={{ fill: "var(--tc-card-2)" }} content={<WeekTooltip />} isAnimationActive={false} />
          <Bar dataKey="aerobic" stackId="w" fill="var(--color-aerobic)" isAnimationActive={false} />
          <Bar dataKey="anaerobic" stackId="w" fill="var(--color-anaerobic)" isAnimationActive={false} />
          <Bar dataKey="estimated" stackId="w" fill="url(#hatch)" radius={[3, 3, 0, 0]} isAnimationActive={false} />
        </BarChart>
      </ResponsiveContainer>
    </div>
  );
}

function Legend({ items }: { items: [TextKey, string, boolean?][] }) {
  return (
    <div className="flex flex-wrap gap-x-4 gap-y-1 text-xs text-muted">
      {items.map(([k, color, box]) => (
        <span key={k} className="inline-flex items-center gap-1.5">
          <i className={`inline-block w-3.5 rounded-sm ${box ? "h-2" : "h-[3px]"}`} style={{ background: color }} />
          {t(k)}
        </span>
      ))}
    </div>
  );
}


/**
 * Seite „Form“ (E7, #25): Zeitraum und Vorschau oben, darunter getrennte Diagramme für Belastung (Fitness/Ermüdung),
 * Form sowie Monotonie & Strain, je mit Kachelreihe, und die Belastung je Woche. Breit zwei Spalten.
 */
export function FormPage() {
  const [params, setParams] = useSearchParams();
  const period = readPeriod(params);
  const forecast = readForecast(params);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [parts, setPartsState] = useState(readParts);
  const setParts = (on: boolean) => {
    setPartsState(on);
    storeParts(on);
  };
  const tz = useConfig().data?.data.timezone;
  const today = localDate(new Date(), tz);
  const seasons = useSeasons();
  const seasonList = useMemo(() => seasons.data?.data ?? [], [seasons.data]);
  const pickedSeason = period.kind === "season" ? (seasonList.find((x) => x.id === period.id) ?? null) : null;
  const buttonSeason = pickedSeason ?? seasonList.find((x) => x.id === seasons.data?.meta.default_season_id) ?? seasonList[0] ?? null;
  const periods = useQuery({ queryKey: ["stats", "periods", "list"], queryFn: () => apiGet<StatisticsPeriodsResponse>("/stats/periods") });
  const thisYear = Number(today.slice(0, 4));
  const years = yearChoices(
    (periods.data?.data.years ?? []).filter((y) => y.count > 0).map((y) => y.year),
    thisYear,
    period.kind === "year" ? period.year : null,
  );
  const shownYear = period.kind === "year" ? period.year : thisYear;

  const write = (z: ListPeriod, f: ForecastMode | null) => {
    const p = new URLSearchParams();
    writePeriod(p, z);
    if (f !== "rest") p.set("vorschau", f === "steady" ? "wie-bisher" : f === "plan" ? "plan" : "aus");
    setParams(p, { replace: true });
  };
  const setPeriod = (z: ListPeriod) => write(z, forecast);
  const setRange = (side: "from" | "to", day: string) => {
    if (period.kind === "range") setPeriod({ ...period, [side]: day || null });
  };

  const span = periodDays(period, today, pickedSeason);
  const q = new URLSearchParams();
  // „Gesamt“: ab dem ersten Tag mit Belastung
  q.set("from", span.from ?? "1900-01-01");
  if (span.to) q.set("to", span.to);
  q.set("forecast", forecast ?? "rest");
  q.set("forecast_days", String(forecast ? FORECAST_DAYS : 0));
  const query = q.toString();
  const form = useQuery({
    queryKey: ["form", query],
    enabled: period.kind !== "season" || !!pickedSeason,
    queryFn: () => apiGet<FormResponse>(`/form?${query}`),
    placeholderData: (prev) => prev,
    // Solange gerechnet wird, regelmäßig nachsehen
    refetchInterval: (q) => ((q.state.data?.data.pending ?? 0) > 0 ? 3000 : false),
  });

  const d = form.data?.data;
  const first = d?.first_date;
  // Leere Tage vor der ersten Belastung nicht zeigen
  const days = useMemo(() => (d ? d.days.filter((x) => !first || x.date >= first) : []), [d, first]);
  const points = useMemo(() => chartPoints(days, d?.forecast.days ?? []), [days, d]);
  const foster = useMemo(() => fosterPoints(points, d?.monotony_warn ?? 2), [points, d]);
  const bars = useMemo(() => weekBars(days), [days]);
  const c = d?.current;
  const state = c ? formState(c.form) : null;
  const twoCurve = d?.settings.curve_model === "two_curve";
  const monotonous = c?.monotony != null && d != null && c.monotony > d.monotony_warn;
  const empty = d != null && days.length === 0;
  // Ohne Vorschau gibt es keinen Wert „in einer Woche“
  const cmp = c && { ...c.compare, week_ahead: forecast ? c.compare.week_ahead : null };
  // „Plan“ nur, wenn es offene Einheiten gibt (oder schon gewählt ist)
  const hasPlan = d?.plan?.last_date != null || forecast === "plan";
  const goal = forecast === "plan" ? (d?.plan?.goal ?? null) : null;

  return (
    <div className="grid grid-cols-1 gap-4">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <h1 className="text-2xl font-bold">{t("form.title")}</h1>
        <button
          type="button"
          className="inline-flex items-center gap-1.5 rounded-lg border border-line bg-card-2 px-3 py-1.5 text-sm hover:border-accent"
          onClick={() => setSettingsOpen(true)}
          disabled={!d}
        >
          <Settings2 size={15} />
          {t("form.settings")}
        </button>
      </div>

      {form.error && <Problem error={form.error} />}
      {d && !d.zones_configured && (
        <p className="rounded-card border border-dashed border-line px-4 py-3 text-sm text-muted">
          {t("form.noZones")}{" "}
          <Link to="/konto" className="text-accent hover:underline">
            {t("form.noZonesLink")}
          </Link>
        </p>
      )}
      {d && d.pending > 0 && (
        <p className="text-sm text-muted" role="status">
          {t("form.pending", { count: num(d.pending) })}
        </p>
      )}

      <Card>
        <div className="grid gap-3">
          <div className="flex flex-wrap items-center gap-2">
            <PeriodButton on={period.kind === "all"} onClick={() => setPeriod({ kind: "all" })}>
              {t("list.periodAll")}
            </PeriodButton>
            <PeriodButton on={period.kind === "last12"} onClick={() => setPeriod({ kind: "last12" })}>
              {t("list.last12")}
            </PeriodButton>
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
                <input type="date" className={periodSelect} aria-label={t("list.rangeStart")} value={period.from ?? ""} onChange={(e) => setRange("from", e.target.value)} />
                <span className="text-muted">–</span>
                <input type="date" className={periodSelect} aria-label={t("list.rangeEnd")} value={period.to ?? ""} onChange={(e) => setRange("to", e.target.value)} />
              </span>
            )}
          </div>
          <div className="flex flex-wrap items-center gap-2" role="group" aria-label={t("form.forecast")}>
            <span className="text-xs font-medium text-muted">{t("form.forecast")}</span>
            {(hasPlan ? (["rest", "steady", "plan", null] as const) : (["rest", "steady", null] as const)).map((m) => (
              <PeriodButton key={m ?? "off"} on={forecast === m} onClick={() => write(period, m)}>
                {t(`form.forecast.${m ?? "off"}` as TextKey)}
              </PeriodButton>
            ))}
          </div>
          {d && forecast && d.forecast.days.length > 0 && (
            <p className="text-xs text-muted">
              {forecast === "rest"
                ? t("form.forecastHint.rest", { days: d.forecast.days.length })
                : forecast === "plan"
                  ? t("form.forecastHint.plan", { to: d.plan?.last_date ? germanDate(d.plan.last_date) : "–" })
                  : t("form.forecastHint.steady", { days: d.forecast.days.length, load: num(d.forecast.daily_load) })}{" "}
              {forecast === "plan" && (
                <Link to="/plan" className="text-accent hover:underline">
                  {t("form.editPlan")}
                </Link>
              )}
            </p>
          )}
        </div>
      </Card>

      <div className="grid min-w-0 gap-4 xl:grid-cols-2">
        <Card title={t("form.chart.load")} className="grid min-w-0 content-start gap-3">
          {c && cmp && (
            <Tiles
              testId="tiles-load"
              rows={[
                { label: "form.fitness", className: "text-fitness", now: num(c.fitness), ...deltas(c.fitness, cmp, "fitness") },
                { label: "form.fatigue", className: "text-fatigue", now: num(c.fatigue), ...deltas(c.fatigue, cmp, "fatigue") },
              ]}
            />
          )}
          <div className="flex flex-wrap items-center justify-between gap-2">
            <Legend items={[["form.fitness", "var(--color-fitness)", true], ["form.fatigue", "var(--color-fatigue)", true]]} />
            {twoCurve && (
              <label className="inline-flex items-center gap-1.5 text-xs text-muted">
                <input type="checkbox" checked={parts} onChange={(e) => setParts(e.target.checked)} />
                {t("form.showParts")}
              </label>
            )}
          </div>
          {twoCurve && parts && c?.fatigue_aerobic != null && c.fatigue_anaerobic != null && (
            <Legend
              items={[
                ["form.fatigueAerobic", "var(--color-aerobic)"],
                ["form.fatigueAnaerobic", "var(--color-anaerobic)"],
              ]}
            />
          )}
          {empty ? <p className="py-10 text-center text-sm text-muted">{t("form.empty")}</p> : <LoadChart points={points} parts={twoCurve && parts} goal={goal} />}
        </Card>

        <Card title={t("form.chart.form")} className="grid min-w-0 content-start gap-3">
          {c && cmp && (
            <Tiles
              testId="tiles-form"
              rows={[{ label: "form.form", now: signed(c.form), ...deltas(c.form, cmp, "form") }]}
            />
          )}
          {state && (
            <div className="flex flex-wrap items-center gap-2 text-xs">
              <span className={`rounded-full px-2 py-0.5 font-semibold ${STATE_CLASS[state]}`} title={t("form.stateHint")}>
                {t(`form.state.${state}` as TextKey)}
              </span>
            </div>
          )}
          {empty ? <p className="py-10 text-center text-sm text-muted">{t("form.empty")}</p> : <FormChart points={points} goal={goal} />}
        </Card>

        <Card title={t("form.chart.foster")} className="grid min-w-0 content-start gap-3">
          {c && cmp && (
            <Tiles
              testId="tiles-foster"
              rows={[
                {
                  label: "form.monotonyShort",
                  className: monotonous ? "text-danger" : undefined,
                  now: c.monotony != null ? num(c.monotony, 2) : "–",
                  digits: 2,
                  ...deltas(c.monotony, cmp, "monotony"),
                },
                { label: "form.strainShort", now: c.strain != null ? num(c.strain) : "–", ...deltas(c.strain, cmp, "strain") },
              ]}
            />
          )}
          <div className="flex flex-wrap items-center gap-2 text-xs" title={d ? t("form.monotonyHint", { warn: num(d.monotony_warn, 1) }) : undefined}>
            {c?.monotony != null && (
              <span className={`rounded-full px-2 py-0.5 font-semibold ${monotonous ? STATE_CLASS.overload : STATE_CLASS.fresh}`}>
                {t(monotonous ? "form.monotonyHigh" : "form.monotonyOk")}
              </span>
            )}
            {d?.settings.foster_surcharge && monotonous && <span className="text-muted">{t("form.surchargeActive", { pct: d.settings.foster_surcharge_pct })}</span>}
            <span className="text-muted">{t("form.fosterHint", { warn: num(d?.monotony_warn ?? 2, 1) })}</span>
          </div>
          {empty ? <p className="py-10 text-center text-sm text-muted">{t("form.empty")}</p> : <FosterChart points={foster} warn={d?.monotony_warn ?? 2} goal={goal} />}
        </Card>

        <Card title={t("form.weeks")} className="grid min-w-0 content-start gap-3">
          {c && (
            <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1 text-sm">
              <span className="text-xs font-medium text-muted">{t("form.load7")}</span>
              <span className="num text-lg font-semibold">{num(c.load_7d)}</span>
              <span className="text-xs text-muted">{t("form.vsLastWeek", { delta: signed(c.load_7d - c.load_prev_7d) })}</span>
            </div>
          )}
          <Legend
            items={[
              ["form.aerobic", "var(--color-aerobic)", true],
              ["form.anaerobic", "var(--color-anaerobic)", true],
              ["form.estimated", "repeating-linear-gradient(45deg, var(--tc-muted) 0 2px, transparent 2px 4px)", true],
            ]}
          />
          {bars.length > 0 && <Weeks bars={bars} />}
        </Card>
      </div>

      {settingsOpen && d && <FormSettingsDialog settings={d.settings} onClose={() => setSettingsOpen(false)} />}
    </div>
  );
}
