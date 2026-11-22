import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Link, useSearchParams } from "react-router-dom";
import { Settings2 } from "lucide-react";
import { Area, Bar, BarChart, CartesianGrid, ComposedChart, Line, ReferenceLine, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import { apiGet, type ForecastMode, type FormResponse, type StatisticsPeriodsResponse } from "../../api/client";
import { Card, KpiCard } from "../../components/Card";
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
  STATE_CLASS,
  chartPoints,
  formState,
  signed,
  weekBars,
  zeroOffset,
  type ChartPoint,
  type WeekBar,
} from "./formModel";

const MONTHS = ["Jan", "Feb", "Mär", "Apr", "Mai", "Jun", "Jul", "Aug", "Sep", "Okt", "Nov", "Dez"];

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
  return v === "wie-bisher" ? "steady" : "rest";
}

function CurveTooltip({ active, payload }: { active?: boolean; payload?: { payload: ChartPoint }[] }) {
  const p = payload?.[0]?.payload;
  if (!active || !p) return null;
  const forecast = p.fitness == null;
  const row = (key: TextKey, v: number | undefined, color: string) =>
    v == null ? null : (
      <div className="flex justify-between gap-4">
        <span style={{ color }}>{t(key)}</span>
        <span className="num">{key === "form.form" ? signed(v) : num(v)}</span>
      </div>
    );
  return (
    <div className="rounded-lg border border-line bg-card px-3 py-2 text-xs shadow">
      <div className="mb-1 font-semibold">
        {germanDate(p.date)}
        {forecast && <span className="ml-1 font-normal text-muted">· {t("form.forecastLabel")}</span>}
      </div>
      {row("form.fitness", p.fitness ?? p.fitnessForecast, "var(--color-fitness)")}
      {row("form.fatigue", p.fatigue ?? p.fatigueForecast, "var(--color-fatigue)")}
      {row("form.form", p.form ?? p.formForecast, "var(--tc-text)")}
      {!forecast && row("form.load", p.load, "var(--tc-muted)")}
    </div>
  );
}

function Curve({ points }: { points: ChartPoint[] }) {
  const span = points.length;
  const offset = zeroOffset(points);
  return (
    <div className="h-72" data-testid="form-curve">
      <ResponsiveContainer width="100%" height="100%">
        <ComposedChart data={points} margin={{ top: 6, right: 6, bottom: 0, left: -14 }}>
          <defs>
            <linearGradient id="formFill" x1="0" y1="0" x2="0" y2="1">
              <stop offset={offset} stopColor="var(--color-good)" stopOpacity={0.25} />
              <stop offset={offset} stopColor="var(--color-warn)" stopOpacity={0.28} />
            </linearGradient>
          </defs>
          <CartesianGrid stroke="var(--tc-line)" vertical={false} />
          <XAxis
            dataKey="date"
            tickFormatter={tickOf(span)}
            minTickGap={24}
            tick={{ fill: "var(--tc-muted)", fontSize: 11 }}
            stroke="var(--tc-line)"
            tickLine={false}
          />
          <YAxis tick={{ fill: "var(--tc-muted)", fontSize: 11 }} stroke="var(--tc-line)" tickLine={false} width={44} allowDecimals={false} />
          <Tooltip content={<CurveTooltip />} isAnimationActive={false} />
          <ReferenceLine y={0} stroke="var(--tc-muted)" strokeOpacity={0.5} />
          <Area dataKey="form" stroke="var(--color-good)" strokeWidth={1} fill="url(#formFill)" isAnimationActive={false} connectNulls={false} />
          <Area
            dataKey="formForecast"
            stroke="var(--color-good)"
            strokeWidth={1}
            strokeDasharray="4 3"
            fill="url(#formFill)"
            fillOpacity={0.5}
            isAnimationActive={false}
          />
          <Line dataKey="fatigue" stroke="var(--color-fatigue)" strokeWidth={1.6} dot={false} isAnimationActive={false} />
          <Line dataKey="fatigueForecast" stroke="var(--color-fatigue)" strokeWidth={1.6} strokeDasharray="4 3" dot={false} isAnimationActive={false} />
          <Line dataKey="fitness" stroke="var(--color-fitness)" strokeWidth={2.4} dot={false} isAnimationActive={false} />
          <Line dataKey="fitnessForecast" stroke="var(--color-fitness)" strokeWidth={2.4} strokeDasharray="4 3" dot={false} isAnimationActive={false} />
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

/** Seite „Form“ (E7): aktueller Stand, Formkurve mit Vorschau und Belastung je Woche. */
export function FormPage() {
  const [params, setParams] = useSearchParams();
  const period = readPeriod(params);
  const forecast = readForecast(params);
  const [settingsOpen, setSettingsOpen] = useState(false);
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
    if (f !== "rest") p.set("vorschau", f === "steady" ? "wie-bisher" : "aus");
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
  const bars = useMemo(() => weekBars(days), [days]);
  const c = d?.current;
  const state = c ? formState(c.form) : null;

  return (
    <div className="grid gap-4">
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

      <div className="grid grid-cols-2 gap-4 lg:grid-cols-4">
        <KpiCard
          label={t("form.fitness")}
          value={c ? <span className="text-fitness">{num(c.fitness)}</span> : "–"}
          sub={c && t("form.vsLastWeek", { delta: signed(c.fitness - c.fitness_week_ago) })}
        />
        <KpiCard
          label={t("form.fatigue")}
          value={c ? <span className="text-fatigue">{num(c.fatigue)}</span> : "–"}
          sub={c && t("form.vsLastWeek", { delta: signed(c.fatigue - c.fatigue_week_ago) })}
        />
        <KpiCard
          label={t("form.form")}
          value={c ? signed(c.form) : "–"}
          sub={
            state && (
              <span className={`rounded-full px-2 py-0.5 font-semibold ${STATE_CLASS[state]}`} title={t("form.stateHint")}>
                {t(`form.state.${state}` as TextKey)}
              </span>
            )
          }
        />
        <KpiCard
          label={t("form.load7")}
          value={c ? num(c.load_7d) : "–"}
          sub={c && t("form.vsLastWeek", { delta: signed(c.load_7d - c.load_prev_7d) })}
        />
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
          <div className="flex flex-wrap items-center justify-between gap-2">
            <div className="flex flex-wrap items-center gap-2" role="group" aria-label={t("form.forecast")}>
              <span className="text-xs font-medium text-muted">{t("form.forecast")}</span>
              {(["rest", "steady", null] as const).map((m) => (
                <PeriodButton key={m ?? "off"} on={forecast === m} onClick={() => write(period, m)}>
                  {t(`form.forecast.${m ?? "off"}` as TextKey)}
                </PeriodButton>
              ))}
            </div>
            <Legend
              items={[
                ["form.fitness", "var(--color-fitness)"],
                ["form.fatigue", "var(--color-fatigue)"],
                ["form.form", "var(--color-good)"],
              ]}
            />
          </div>
          {d && forecast && d.forecast.days.length > 0 && (
            <p className="text-xs text-muted">
              {forecast === "rest"
                ? t("form.forecastHint.rest", { days: d.forecast.days.length })
                : t("form.forecastHint.steady", { days: d.forecast.days.length, load: num(d.forecast.daily_load) })}
            </p>
          )}
          {d && days.length === 0 ? <p className="py-10 text-center text-sm text-muted">{t("form.empty")}</p> : <Curve points={points} />}
          <div className="flex flex-wrap items-center justify-between gap-2">
            <h2 className="text-xs font-semibold uppercase tracking-wide text-muted">{t("form.weeks")}</h2>
            <Legend
              items={[
                ["form.aerobic", "var(--color-aerobic)", true],
                ["form.anaerobic", "var(--color-anaerobic)", true],
                ["form.estimated", "repeating-linear-gradient(45deg, var(--tc-muted) 0 2px, transparent 2px 4px)", true],
              ]}
            />
          </div>
          {bars.length > 0 && <Weeks bars={bars} />}
        </div>
      </Card>

      {settingsOpen && d && <FormSettingsDialog settings={d.settings} onClose={() => setSettingsOpen(false)} />}
    </div>
  );
}
