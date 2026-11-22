import { useMemo, useState, type ReactNode } from "react";
import { useQuery } from "@tanstack/react-query";
import { Link, useSearchParams } from "react-router-dom";
import { ChevronLeft, ChevronRight, Pencil, Plus } from "lucide-react";
import {
  apiGet,
  type PeriodShortSummary,
  type RankedSportClass,
  type RecordKind,
  type Season,
  type SportClass,
  type StatisticsPeriodsResponse,
  type StatisticsResponse,
} from "../../api/client";
import { Card, KpiCard } from "../../components/Card";
import { Problem } from "../../components/Problem";
import { SeasonDot } from "../../components/SeasonDot";
import { SportFilterPill } from "../../components/SportFilterPill";
import { SportIcon } from "../../components/SportIcon";
import { useConfig } from "../../hooks/useConfig";
import { t, type TextKey } from "../../i18n";
import { dateOnly, hoursMinutes, localDate, num } from "../../utils/format";
import { SeasonDialog } from "../seasons/SeasonDialog";
import { useSeasons } from "../seasons/useSeasons";
import { ShareRing, StackedBars } from "./StatsCharts";
import {
  classesIn,
  monthBars,
  readSelection,
  recordValue,
  resolve,
  seasonRow,
  shares,
  statsQuery,
  totalOf,
  weekBars,
  withRange,
  withSport,
  writeSelection,
  type Selection,
} from "./statsModel";
import { RANKED_CLASSES, SPORT_CLASSES, subSportEntryLabel } from "../../utils/sports";

const SPORT_PILLS: SportClass[] = SPORT_CLASSES;
const RANKED: RankedSportClass[] = RANKED_CLASSES;
const KINDS: RecordKind[] = ["distance", "elevation", "time", "speed"];
const input = "rounded-lg border border-line bg-card-2 px-2 py-1.5 text-sm text-text";

function Pill({ on, onClick, children }: { on: boolean; onClick: () => void; children: ReactNode }) {
  return (
    <button
      type="button"
      aria-pressed={on}
      onClick={onClick}
      className={`inline-flex items-center gap-1.5 rounded-full border px-3 py-1 text-sm ${on ? "border-transparent bg-accent-strong text-white" : "border-line bg-card-2 text-muted hover:text-text"}`}
    >
      {children}
    </button>
  );
}

function Short({ s }: { s: PeriodShortSummary | undefined }) {
  return (
    // Jede Kennzahl ist ein eigener Block (Titel über Wert mit Einheit). Reicht die Breite nicht,
    // bricht der letzte Block in eine neue Zeile um, statt über den Kachelrand zu ragen.
    <dl className="mt-2 flex flex-wrap justify-between gap-x-3 gap-y-2 text-xs">
      <div className="grid min-w-0">
        <dt className="text-muted">{t("dashboard.count")}</dt>
        <dd className="num text-base font-semibold">{s ? num(s.count) : "–"}</dd>
      </div>
      <div className="grid min-w-0">
        <dt className="text-muted">{t("dashboard.distance")}</dt>
        <dd className="num whitespace-nowrap text-base font-semibold">
          {s ? num(s.distance_m / 1000) : "–"}
          <span className="ml-1 text-xs font-normal text-muted">km</span>
        </dd>
      </div>
      <div className="grid min-w-0">
        <dt className="text-muted">{t("dashboard.time")}</dt>
        <dd className="num whitespace-nowrap text-base font-semibold">
          {s ? hoursMinutes(s.total_time_ms) : "–"}
          <span className="ml-1 text-xs font-normal text-muted">h</span>
        </dd>
      </div>
    </dl>
  );
}

const tile = "relative rounded-card border bg-card p-3 text-left shadow-sm";
const tileOn = "border-accent ring-1 ring-inset ring-accent";

/** Saison-Kachel: Name, Symbol läuft/nicht aktiv, Kurzsummen, Stift zum Ändern. */
function SeasonTile({
  season,
  first,
  on,
  summary,
  onPick,
  onEdit,
}: {
  season: Season;
  first: boolean;
  on: boolean;
  summary: PeriodShortSummary | undefined;
  onPick: () => void;
  onEdit: () => void;
}) {
  const live = season.state === "current";
  return (
    <div className={`${tile} ${on ? tileOn : "border-line hover:border-accent-strong"}`} data-testid="season-tile">
      <button type="button" className="absolute inset-0 rounded-card" aria-pressed={on} aria-label={t("stats.pickSeason", { name: season.name })} onClick={onPick} />
      <div className="pointer-events-none relative">
        <div className="flex items-center gap-2 pr-7">
          <SeasonDot state={season.state} labelled />
          <span className="truncate font-semibold">{season.name}</span>
          {first && <span className="ml-auto shrink-0 text-[10px] uppercase tracking-wide text-muted">{t(live ? "stats.current" : "stats.last")}</span>}
        </div>
        <Short s={summary} />
      </div>
      <button
        type="button"
        className="absolute top-2 right-2 rounded-md p-1 text-muted hover:bg-card-2 hover:text-text"
        aria-label={t("season.editNamed", { name: season.name })}
        title={t("season.edit")}
        onClick={onEdit}
      >
        <Pencil size={13} />
      </button>
    </div>
  );
}

/** Statistikseite nach RFC-0006 „Statistik“: Saison- und Zeitraum-Reihe, Kennzahlen, Balken, Rekorde. */
export function StatisticsPage() {
  const [params, setParams] = useSearchParams();
  const sel = readSelection(params);
  const apply = (s: Selection) => setParams(writeSelection(s), { replace: true });
  const config = useConfig();
  const cfg = config.data?.data;
  const tz = cfg?.timezone;
  const seasons = useSeasons();
  const [showAll, setShowAll] = useState(false);
  const [dialog, setDialog] = useState<Season | "new" | null>(null);

  const list = useMemo(() => seasons.data?.data ?? [], [seasons.data]);
  const defaultId = seasons.data?.meta.default_season_id ?? null;
  const today = seasons.data?.meta.today ?? localDate(new Date(), tz);
  const currentYear = Number(today.slice(0, 4));
  const resolved = resolve(sel.period, list, defaultId, currentYear);
  const ready = seasons.isSuccess || seasons.isError;

  const periods = useQuery({
    queryKey: ["stats", "periods", sel.sport, sel.sub],
    queryFn: () => apiGet<StatisticsPeriodsResponse>(`/stats/periods?${statsQuery(null, sel)}`),
  });
  const query = statsQuery(resolved, sel);
  const stats = useQuery({
    queryKey: ["stats", "overview", query],
    enabled: ready,
    queryFn: () => apiGet<StatisticsResponse>(`/stats?${query}`),
    placeholderData: (prev) => prev,
  });

  const row = seasonRow(list, defaultId, showAll);
  const hidden = list.length - seasonRow(list, defaultId, false).length;
  const years = periods.data?.data.years.map((y) => y.year) ?? [currentYear];
  const year = sel.period.kind === "year" ? sel.period.year : currentYear;
  const yearIdx = years.indexOf(year);
  const pickYear = (y: number) => apply({ ...sel, period: { kind: "year", year: y } });
  // Von–Bis zeigt den Zeitraum von Jahr oder Saison und bleibt änderbar
  const subEntry = sel.sport && sel.sub ? cfg?.sub_sport_catalog[sel.sport]?.find((x) => x.key === sel.sub) : undefined;
  const range = sel.period.kind === "range" ? sel.period : { from: resolved.from ?? "", to: resolved.to ?? "" };

  const d = stats.data?.data;
  const total = d ? totalOf(d.summaries) : null;
  // Ohne festen Zeitraum vom ersten bis zum letzten Tag mit Aktivität
  const span = { from: resolved.from ?? d?.first_day ?? null, to: resolved.to ?? d?.last_day ?? null };
  const distBars = d ? monthBars(d.by_month, "distance", span.from, span.to) : [];
  const timeBars = d ? monthBars(d.by_month, "time", span.from, span.to) : [];
  const weeks = d ? weekBars(d.by_week, span.from, span.to) : [];
  const recordClasses = RANKED.filter((c) => d?.records.some((r) => r.sport_class === c));
  const seasonSum = (id: string) => periods.data?.data.seasons.find((s) => s.season_id === id);
  const yearSum = periods.data?.data.years.find((y) => y.year === year);

  return (
    <div className="grid gap-4">
      <h1 className="text-2xl font-bold">{t("nav.statistics")}</h1>

      <section className="grid gap-2" aria-label={t("stats.seasons")}>
        <h2 className="text-xs font-semibold uppercase tracking-wide text-muted">{t("stats.seasons")}</h2>
        {seasons.error && <Problem error={seasons.error} />}
        <div className="grid grid-cols-[repeat(auto-fill,minmax(240px,1fr))] gap-3">
          {row.map((s, i) => (
            <SeasonTile
              key={s.id}
              season={s}
              first={i === 0 && s.id === defaultId}
              on={resolved.season?.id === s.id}
              summary={seasonSum(s.id)}
              onPick={() => apply({ ...sel, period: { kind: "season", id: s.id === defaultId ? null : s.id } })}
              onEdit={() => setDialog(s)}
            />
          ))}
          <button
            type="button"
            className={`${tile} grid min-h-24 place-items-center border-dashed border-line bg-transparent text-muted hover:border-accent-strong hover:text-text`}
            onClick={() => setDialog("new")}
          >
            <span className="flex items-center gap-1.5">
              <Plus size={16} />
              {t("season.new")}
            </span>
          </button>
        </div>
        {hidden > 0 && (
          <button type="button" className="justify-self-start text-sm text-accent hover:underline" onClick={() => setShowAll((x) => !x)}>
            {showAll ? t("stats.fewerSeasons") : t("stats.allSeasons", { n: list.length })}
          </button>
        )}
      </section>

      <section className="grid gap-2" aria-label={t("stats.period")}>
        <h2 className="text-xs font-semibold uppercase tracking-wide text-muted">{t("stats.period")}</h2>
        <div className="grid gap-3 md:grid-cols-[270px_1fr]">
          <div className={`${tile} ${sel.period.kind === "year" ? tileOn : "border-line hover:border-accent-strong"}`}>
            <button type="button" className="absolute inset-0 rounded-card" aria-pressed={sel.period.kind === "year"} aria-label={t("stats.pickYear", { year })} onClick={() => pickYear(year)} />
            <div className="pointer-events-none relative grid gap-1">
              <span className="text-xs font-semibold uppercase tracking-wide text-muted">{t("stats.year")}</span>
              <div className="flex items-center gap-2">
                <button
                  type="button"
                  className="pointer-events-auto rounded-lg border border-line bg-card-2 p-1 disabled:opacity-40"
                  aria-label={t("stats.prevYear")}
                  disabled={yearIdx < 0 || yearIdx >= years.length - 1}
                  onClick={() => pickYear(years[yearIdx + 1]!)}
                >
                  <ChevronLeft size={16} />
                </button>
                <span className="num min-w-16 text-center text-2xl font-semibold">{year}</span>
                <button
                  type="button"
                  className="pointer-events-auto rounded-lg border border-line bg-card-2 p-1 disabled:opacity-40"
                  aria-label={t("stats.nextYear")}
                  disabled={yearIdx <= 0}
                  onClick={() => pickYear(years[yearIdx - 1]!)}
                >
                  <ChevronRight size={16} />
                </button>
              </div>
              <Short s={yearSum} />
            </div>
          </div>
          <div className="flex flex-wrap items-end gap-x-5 gap-y-3 rounded-card border border-line bg-card p-3 shadow-sm">
            <label className="grid gap-1">
              <span className="text-xs font-medium text-muted">{t("stats.from")}</span>
              <input type="date" className={input} value={range.from} onChange={(e) => apply(withRange(sel, e.target.value, range.to))} />
            </label>
            <label className="grid gap-1">
              <span className="text-xs font-medium text-muted">{t("stats.to")}</span>
              <input type="date" className={input} value={range.to} onChange={(e) => apply(withRange(sel, range.from, e.target.value))} />
            </label>
            <div className="grid gap-1" role="group" aria-label={t("stats.sport")}>
              <span className="text-xs font-medium text-muted">{t("stats.sport")}</span>
              <div className="flex flex-wrap gap-1.5">
                <Pill on={sel.sport === null} onClick={() => apply(withSport(sel, null))}>
                  {t("list.allData")}
                </Pill>
                {/* Untertyp im Pfeilmenü des geteilten Knopfs */}
                {SPORT_PILLS.map((c) => (
                  <SportFilterPill
                    key={c}
                    sport={c}
                    label={t(`sport.${c}` as TextKey)}
                    on={sel.sport === c}
                    sub={sel.sport === c ? sel.sub : null}
                    subs={cfg?.sub_sport_catalog[c] ?? []}
                    onPick={() => apply(withSport(sel, c))}
                    onPickSub={(sub) => apply({ ...sel, sport: c, sub })}
                  />
                ))}
              </div>
            </div>
            <button type="button" className="py-1.5 text-sm text-accent hover:underline" onClick={() => apply({ period: { kind: "season", id: null }, sport: null, sub: null })}>
              {t("stats.reset")}
            </button>
          </div>
        </div>
      </section>

      <h2 className="text-xs font-semibold uppercase tracking-wide text-muted" data-testid="selection-title">
        {resolved.title}
        {sel.sport && ` · ${t(`sport.${sel.sport}` as TextKey)}`}
        {subEntry ? ` · ${subSportEntryLabel(subEntry)}` : sel.sub && ` · ${sel.sub}`}
      </h2>

      {stats.error ? (
        <Card>
          <Problem error={stats.error} />
        </Card>
      ) : !d || !total ? (
        <Card>
          <p className="text-muted">{t("state.loading")}</p>
        </Card>
      ) : (
        <>
          <div className="grid grid-cols-2 gap-4 lg:grid-cols-5">
            <KpiCard label={t("list.kpi.count")} value={num(total.count)} />
            <KpiCard
              label={t("list.kpi.distance")}
              value={num(total.distance_m / 1000)}
              unit="km"
              sub={total.activities_without_distance > 0 ? t("stats.without", { n: total.activities_without_distance }) : undefined}
            />
            <KpiCard label={t("list.kpi.time")} value={hoursMinutes(total.total_time_ms)} unit="h" />
            <KpiCard
              label={t("list.kpi.gain")}
              value={num(total.elevation_gain_m)}
              unit="m"
              sub={total.activities_without_elevation > 0 ? t("stats.without", { n: total.activities_without_elevation }) : undefined}
            />
            <KpiCard label={t("stats.prs")} value={num(total.segment_success_count)} sub={t("stats.prsHint")} />
          </div>

          {total.count === 0 ? (
            <Card>
              <p className="text-muted">{t("stats.empty")}</p>
            </Card>
          ) : (
            <>
              <div className="grid gap-4 xl:grid-cols-2">
                <Card title={t("stats.distanceByMonth")}>
                  <div className="grid gap-4 sm:grid-cols-[1fr_170px]">
                    <StackedBars bars={distBars} classes={classesIn(distBars)} unit="km" />
                    <ShareRing parts={shares(distBars)} unit="km" />
                  </div>
                </Card>
                <Card title={t("stats.timeByMonth")}>
                  <div className="grid gap-4 sm:grid-cols-[1fr_170px]">
                    <StackedBars bars={timeBars} classes={classesIn(timeBars)} unit="h" />
                    <ShareRing parts={shares(timeBars)} unit="h" />
                  </div>
                </Card>
              </div>
              <Card title={t("stats.timeByWeek")}>
                <StackedBars bars={weeks} classes={classesIn(weeks)} unit="h" height={170} />
              </Card>
            </>
          )}

          <Card title={t("stats.records")}>
            {recordClasses.length === 0 ? (
              <p className="text-sm text-muted">{t("stats.noRecords")}</p>
            ) : (
              <div className="grid gap-4 lg:grid-cols-3">
                {recordClasses.map((c) => (
                  <div key={c} className="grid content-start gap-1" data-testid={`records-${c}`}>
                    <h3 className="flex items-center gap-2 font-semibold">
                      <SportIcon sport={c} />
                      {t(`sport.${c}` as TextKey)}
                    </h3>
                    <ul className="divide-y divide-line">
                      {KINDS.map((k) => {
                        const r = d.records.find((x) => x.sport_class === c && x.kind === k);
                        if (!r) return null;
                        const v = recordValue(r);
                        return (
                          <li key={k} className="grid grid-cols-[1fr_auto] gap-x-3 py-1.5 text-sm">
                            <span className="text-muted">{t(`stats.record.${k}${k === "speed" && c === "running" ? "Pace" : ""}` as TextKey)}</span>
                            <span className="num text-right font-semibold">
                              {v.value} <span className="text-xs font-normal text-muted">{v.unit}</span>
                            </span>
                            <Link to={`/aktivitaeten/${r.activity.id}`} className="col-span-2 truncate text-xs text-muted hover:text-text hover:underline">
                              {dateOnly(r.activity.start_time, tz)} · {r.activity.title}
                              {r.equaled_count > 0 && ` · ${t("stats.equaled", { n: r.equaled_count })}`}
                            </Link>
                          </li>
                        );
                      })}
                    </ul>
                  </div>
                ))}
              </div>
            )}
          </Card>
        </>
      )}

      {dialog && (
        <SeasonDialog
          season={dialog === "new" ? undefined : dialog}
          onClose={() => setDialog(null)}
          onDeleted={(id) => {
            if (sel.period.kind === "season" && sel.period.id === id) apply({ ...sel, period: { kind: "season", id: null } });
          }}
        />
      )}
    </div>
  );
}
