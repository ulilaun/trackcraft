import { memo, useEffect, useRef, useState } from "react";
import { Area, AreaChart, CartesianGrid, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import { t } from "../../i18n";
import { dateOnly, num, segmentTime } from "../../utils/format";
import type { TrackPoint } from "../activities/trackModel";
import type { ProgressPoint } from "./segmentModel";

const tooltipStyle = { background: "var(--tc-card)", border: "1px solid var(--tc-line)", borderRadius: 10, fontSize: 12 };
const tick = { fill: "var(--tc-muted)", fontSize: 11 };
const GOLD = "var(--color-gold)";
const YEAR = "var(--tc-accent)";
const OWN = "#f59e0b";

/** Höhenprofil der Referenz über die Distanz; Hover meldet den Punkt an die Karte. */
export const ElevationProfile = memo(function ElevationProfile({ points, onHover }: { points: TrackPoint[]; onHover: (index: number | null) => void }) {
  const data = points.flatMap((p, i) => (p.ele != null && p.distM != null ? [{ x: p.distM / 1000, ele: p.ele, i }] : []));
  if (data.length < 2) return <p className="text-sm text-muted">{t("segment.profile.none")}</p>;
  const hoverAt = (index: unknown) => {
    const i = index == null ? null : Number(index);
    onHover(i == null ? null : (data[i]?.i ?? null));
  };
  return (
    <div className="h-full min-h-[240px] touch-pan-y select-none" data-testid="segment-profile">
      <ResponsiveContainer width="100%" height="100%">
        <AreaChart
          data={data}
          margin={{ top: 4, right: 4, bottom: 0, left: 4 }}
          onMouseMove={(s) => hoverAt(s.activeTooltipIndex)}
          onMouseLeave={() => onHover(null)}
          // Finger: Wischen über das Profil zeigt die Stelle auf der Karte
          onTouchStart={(s) => hoverAt(s.activeTooltipIndex)}
          onTouchMove={(s) => hoverAt(s.activeTooltipIndex)}
          onTouchEnd={() => onHover(null)}
        >
          <defs>
            <linearGradient id="tc-segment-profile" x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stopColor="var(--tc-accent)" stopOpacity={0.35} />
              <stop offset="100%" stopColor="var(--tc-accent)" stopOpacity={0} />
            </linearGradient>
          </defs>
          <CartesianGrid stroke="var(--tc-line)" vertical={false} />
          <XAxis dataKey="x" type="number" domain={["dataMin", "dataMax"]} tickFormatter={(v: number) => `${num(v, 1)} km`} tick={tick} stroke="var(--tc-line)" />
          <YAxis domain={["auto", "auto"]} tickFormatter={(v: number) => num(v)} tick={{ ...tick, fill: "var(--tc-accent)" }} stroke="var(--tc-line)" width={48} />
          <Tooltip
            isAnimationActive={false}
            contentStyle={tooltipStyle}
            labelFormatter={(v) => `${num(Number(v), 2)} km`}
            formatter={(value) => [typeof value === "number" ? `${num(value)} m` : "–", t("detail.chart.ele")]}
          />
          <Area dataKey="ele" type="monotone" stroke="var(--tc-accent)" strokeWidth={1.6} fill="url(#tc-segment-profile)" baseValue="dataMin" isAnimationActive={false} dot={false} activeDot={{ r: 3 }} />
        </AreaChart>
      </ResponsiveContainer>
    </div>
  );
});

/** Balkenfarbe: Gold für die Bestzeit, Akzent für die Jahresbestzeit, sonst gedämpft (Uli 12.11.2026). */
export function rankColor(rank: number | null, yearBest = false): string {
  return rank === 1 ? GOLD : yearBest ? YEAR : "var(--tc-muted)";
}

const H = 260;
const PAD = { left: 56, right: 16, top: 44, bottom: 24 };
/** Breite eines Balkens samt Abstand: höchstens so viel, bei vielen Versuchen entsprechend schmaler. */
const MAX_SLOT = 34;
/** Platz, den eine Datumsangabe unter den Balken braucht. */
const LABEL_WIDTH = 54;

/** Lage der Balken: in zeitlicher Folge direkt nebeneinander, von links, Breite an die Anzahl angepasst. */
export function barLayout(count: number, plotWidth: number): { slot: number; bar: number; labelEvery: number } {
  const slot = Math.min(MAX_SLOT, plotWidth / Math.max(count, 1));
  const gap = slot >= 12 ? 4 : slot >= 5 ? 2 : 0.5;
  return { slot, bar: Math.max(slot - gap, 0.5), labelEvery: Math.max(1, Math.ceil(LABEL_WIDTH / slot)) };
}

/** Ab dem Wievielfachen des Medians gilt ein Versuch als Ausreißer (z. B. Defekt, Pause im Segment). */
const OUTLIER_FACTOR = 2;
/** Höchstens dieser Anteil der Versuche darf gekappt werden; sind mehr so langsam, ist das kein Ausreißer mehr. */
const OUTLIER_MAX_SHARE = 0.25;

/**
 * Obergrenze der Zeitachse, wenn einzelne Versuche brutal aus der Reihe fallen: dann der langsamste normale Versuch,
 * und alles darüber wird gekappt mit Bruchzeichen gezeichnet. `null`, wenn es keine solchen Ausreißer gibt.
 */
export function outlierCap(durations: number[]): number | null {
  if (durations.length < 3) return null;
  const sorted = [...durations].sort((a, b) => a - b);
  const mid = sorted.length / 2;
  const median = sorted.length % 2 ? sorted[Math.floor(mid)]! : (sorted[mid - 1]! + sorted[mid]!) / 2;
  const limit = median * OUTLIER_FACTOR;
  const normal = sorted.filter((d) => d <= limit);
  const outliers = sorted.length - normal.length;
  if (outliers === 0 || outliers > sorted.length * OUTLIER_MAX_SHARE || normal.length < 2) return null;
  return normal[normal.length - 1]!;
}

/** So weit ragt ein gekappter Balken über die oberste Achsenlinie hinaus. */
const CAP_OVERHANG = 20;

/** Abstand des Bruchzeichens unter der obersten Achsenlinie. */
const BREAK_BELOW = 18;
/** Sinuswelle des Bruchzeichens: genau eine Periode über die Balkenbreite; Ausschlag in px, Lückenhöhe. */
const WAVE_AMPLITUDE = 3;
const BREAK_GAP = 6;

/** Punkte einer Sinuswelle von `xa` nach `xb` auf Höhe `y`, Phase am linken Rand `x0` verankert. */
function sinePoints(x0: number, period: number, xa: number, xb: number, y: number): string {
  const n = Math.max(2, Math.ceil(Math.abs(xb - xa)));
  return Array.from({ length: n + 1 }, (_, i) => {
    const x = xa + ((xb - xa) * i) / n;
    return `${x.toFixed(2)},${(y + WAVE_AMPLITUDE * Math.sin(((x - x0) / period) * 2 * Math.PI)).toFixed(2)}`;
  }).join(" L ");
}

/** Bruchzeichen: Lücke quer durch den gekappten Balken, Ränder je eine Sinusperiode über die Balkenbreite. */
function BreakMark({ cx, y, width }: { cx: number; y: number; width: number }) {
  const x0 = cx - width / 2 - 2;
  const x1 = cx + width / 2 + 2;
  const top = y - BREAK_GAP / 2;
  const bottom = y + BREAK_GAP / 2;
  return (
    <path
      data-testid="progress-break"
      d={`M ${sinePoints(x0, x1 - x0, x0, x1, top)} L ${sinePoints(x0, x1 - x0, x1, x0, bottom)} Z`}
      fill="var(--tc-card)"
    />
  );
}

/**
 * Bestzeiten-Entwicklung (RFC-0006 „Segmentseite“): jeder gültige Versuch als Balken, in zeitlicher Folge direkt
 * nebeneinander (keine lineare Zeitachse, das Datum steht darunter). Gold mit PR-Zeichen ist die Bestzeit, Akzent
 * die Bestzeit des laufenden Jahres (keine Markierung damaliger Bestzeiten); der Versuch der
 * Herkunftsaktivität hat einen orangen Rahmen. Hover ist mit der Tabelle verbunden (`hoverId`, `onHover`).
 */
export const ProgressionChart = memo(function ProgressionChart({
  data,
  timeZone,
  yearBestId,
  year,
  hoverId,
  onHover,
}: {
  data: ProgressPoint[];
  timeZone?: string;
  yearBestId?: string | null;
  year?: number;
  hoverId?: string | null;
  onHover?: (id: string | null) => void;
}) {
  const box = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(800);
  const [ownHover, setOwnHover] = useState<string | null>(null);
  const hoverKey = onHover ? (hoverId ?? null) : ownHover;
  const setHoverKey = onHover ?? setOwnHover;

  useEffect(() => {
    const el = box.current;
    if (!el || typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver((entries) => {
      const w = entries[0]?.contentRect.width;
      if (w) setWidth(w);
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  if (data.length === 0) return <p className="text-sm text-muted">{t("segment.progress.empty")}</p>;

  const { slot, bar, labelEvery } = barLayout(data.length, width - PAD.left - PAD.right);
  const durations = data.map((p) => p.d);
  const dMin = Math.min(...durations);
  // Fällt ein Versuch brutal aus der Reihe, reicht die Achse nur bis zum langsamsten normalen Versuch.
  const cap = outlierCap(durations);
  const dMax = cap ?? Math.max(...durations);
  const span = dMax - dMin || dMax * 0.1 || 1;
  // Die Achse beginnt knapp unter der Bestzeit, damit die Unterschiede sichtbar sind.
  const y0 = Math.max(0, dMin - span * 0.3);
  const y1 = dMax;
  const base = H - PAD.bottom;
  const sx = (i: number) => PAD.left + slot * (i + 0.5);
  const sy = (v: number) => PAD.top + (1 - (v - y0) / (y1 - y0)) * (base - PAD.top);
  const yTicks = [0, 1, 2, 3].map((i) => y0 + ((y1 - y0) * i) / 3);
  const hasOwn = data.some((p) => p.own != null);
  const hasYear = data.some((p) => p.effort.id === yearBestId && p.effort.rank !== 1);
  const hover = hoverKey == null ? -1 : data.findIndex((p) => p.effort.id === hoverKey);
  const hovered = hover < 0 ? undefined : data[hover];
  const small = slot < 12;

  return (
    <div>
      <div ref={box} className="relative select-none" data-testid="segment-progress">
        <svg width={width} height={H} role="img" aria-label={t("segment.progress.title")}>
          {yTicks.map((v) => (
            <g key={v}>
              <line x1={PAD.left} x2={width - PAD.right} y1={sy(v)} y2={sy(v)} stroke="var(--tc-line)" />
              <text x={PAD.left - 8} y={sy(v) + 4} textAnchor="end" fontSize={11} fill="var(--tc-muted)">
                {segmentTime(v * 1000).replace(/,\d$/, "")}
              </text>
            </g>
          ))}
          {data.map((p, i) =>
            i % labelEvery === 0 ? (
              <text key={p.effort.id} x={sx(i)} y={H - 6} textAnchor={slot < LABEL_WIDTH && i === 0 ? "start" : "middle"} fontSize={11} fill="var(--tc-muted)">
                {dateOnly(p.effort.start_time, timeZone)}
              </text>
            ) : null,
          )}
          {data.map((p, i) => {
            const cx = sx(i);
            const capped = cap != null && p.d > cap;
            const top = capped ? PAD.top - CAP_OVERHANG : sy(p.d);
            const pr = p.effort.rank === 1;
            const yearBest = p.effort.id === yearBestId;
            return (
              <g key={p.effort.id} onMouseEnter={() => setHoverKey(p.effort.id)} onMouseLeave={() => setHoverKey(null)}>
                <rect x={cx - slot / 2} y={PAD.top - 40} width={slot} height={base - PAD.top + 40} fill="transparent" />
                <rect
                  data-testid="progress-bar"
                  data-own={p.own != null ? "true" : undefined}
                  data-hover={hover === i ? "true" : undefined}
                  data-capped={capped ? "true" : undefined}
                  x={cx - bar / 2}
                  y={top}
                  width={bar}
                  height={Math.max(base - top, 2)}
                  rx={Math.min(3, bar / 2)}
                  fill={rankColor(p.effort.rank, yearBest)}
                  stroke={p.own != null ? OWN : undefined}
                  strokeWidth={p.own != null ? (small ? 1.5 : 2.5) : undefined}
                  opacity={hover < 0 || hover === i ? 1 : 0.55}
                />
                {capped && (
                  <>
                    <BreakMark cx={cx} y={PAD.top + BREAK_BELOW} width={bar} />
                    {!small && (
                      <text x={cx} y={top - 5} textAnchor="middle" fontSize={10} fill="var(--tc-muted)" data-testid="progress-capped-value">
                        {segmentTime(p.effort.duration_ms).replace(/,\d$/, "")}
                      </text>
                    )}
                  </>
                )}
                {pr && (
                  <g data-testid="progress-pr">
                    <rect x={cx - 11} y={top - 19} width={22} height={14} rx={7} fill={GOLD} />
                    <text x={cx} y={top - 9} textAnchor="middle" fontSize={9} fontWeight={700} fill="#000">
                      PR
                    </text>
                  </g>
                )}
              </g>
            );
          })}
          <line x1={PAD.left} x2={width - PAD.right} y1={base} y2={base} stroke="var(--tc-line)" />
        </svg>
        {hovered && (
          <div
            className="pointer-events-none absolute top-0 z-10 -translate-x-1/2 whitespace-nowrap rounded-lg border border-line bg-card px-2.5 py-1.5 text-xs shadow-lg"
            style={{ left: Math.min(Math.max(sx(hover), 110), width - 110) }}
          >
            <div className="font-medium">
              {dateOnly(hovered.effort.start_time, timeZone, true)} · {hovered.effort.activity_title}
            </div>
            <div className="text-muted">
              <span className="num text-text">{segmentTime(hovered.effort.duration_ms)}</span>
              {hovered.effort.rank != null && <> · {t("segment.kpi.rankOnly", { rank: hovered.effort.rank, of: data.length })}</>}
            </div>
          </div>
        )}
      </div>
      <p className="mt-1.5 flex flex-wrap items-center gap-x-4 gap-y-1 text-xs text-muted">
        <span className="flex items-center gap-1.5">
          <span className="inline-block h-3 w-1.5 rounded-sm bg-gold" /> {t("segment.progress.pr")}
        </span>
        {hasYear && (
          <span className="flex items-center gap-1.5">
            <span className="inline-block h-3 w-1.5 rounded-sm bg-accent" /> {t("segment.board.yearBest", { year: year ?? "" })}
          </span>
        )}
        <span className="flex items-center gap-1.5">
          <span className="inline-block h-3 w-1.5 rounded-sm bg-muted" /> {t("segment.progress.other")}
        </span>
        {hasOwn && (
          <span className="flex items-center gap-1.5">
            <span className="inline-block h-3 w-2 rounded-sm border-2 border-warn" /> {t("segment.progress.own")}
          </span>
        )}
      </p>
    </div>
  );
});
