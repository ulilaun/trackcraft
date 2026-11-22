import { useEffect, useLayoutEffect, useMemo, useRef, useState, type KeyboardEvent, type PointerEvent, type ReactNode } from "react";
import { ChevronLeft, ChevronRight } from "lucide-react";
import type { SegmentKind } from "../../api/client";
import { t } from "../../i18n";
import { duration, num } from "../../utils/format";
import { GAP_A, GAP_W, detailWindow, idxAt, moveNearer, shiftBand, type Axis, type Range, type SelPoint } from "./createModel";

interface Props {
  points: SelPoint[];
  axis: Axis;
  kind: SegmentKind;
  range: Range;
  onChange: (r: Range) => void;
  onHover: (i: number | null) => void;
}

const W = 1000;
const H = 90;
const START = "bg-[#16a34a]";
const END = "bg-[#dc2626]";

export function distanceText(m: number): string {
  return m < 1000 ? `${num(m)} m` : `${num(m / 1000, 2)} km`;
}

/** Dauerdruck: erst nach 400 ms wiederholen, dann alle 70 ms, nach 20 Schritten in Fünferschritten. */
const REPEAT_DELAY_MS = 400;
const REPEAT_EVERY_MS = 70;
const REPEAT_FAST_AFTER = 20;

/** Knopf, der bei gedrückter Maus bzw. gedrücktem Finger weiter auslöst (Uli 12.11.2026). */
function RepeatButton({ onStep, label, className, children }: { onStep: (times: number) => void; label: string; className: string; children: ReactNode }) {
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const step = useRef(onStep);
  useLayoutEffect(() => {
    step.current = onStep;
  });
  const stop = () => {
    if (timer.current) clearTimeout(timer.current);
    timer.current = null;
  };
  useEffect(() => stop, []);
  const start = (ev: PointerEvent<HTMLButtonElement>) => {
    if (ev.button !== 0) return;
    ev.preventDefault();
    ev.currentTarget.setPointerCapture?.(ev.pointerId);
    stop();
    step.current(1);
    let n = 0;
    const tick = () => {
      n++;
      step.current(n > REPEAT_FAST_AFTER ? 5 : 1);
      timer.current = setTimeout(tick, REPEAT_EVERY_MS);
    };
    timer.current = setTimeout(tick, REPEAT_DELAY_MS);
  };
  return (
    <button
      type="button"
      // Finger: kein Kontextmenü und kein Markieren bei langem Druck, sonst bricht die Wiederholung ab
      className={`${className} touch-none select-none [-webkit-touch-callout:none]`}
      aria-label={label}
      title={label}
      onContextMenu={(ev) => ev.preventDefault()}
      onPointerDown={start}
      onPointerUp={stop}
      onPointerCancel={stop}
      onLostPointerCapture={stop}
      // Tastatur (Leertaste/Enter) löst einen Schritt aus; die Wiederholung übernimmt dort das System.
      onKeyDown={(ev) => {
        if (ev.key === "Enter" || ev.key === " ") {
          ev.preventDefault();
          step.current(1);
        }
      }}
    >
      {children}
    </button>
  );
}

function pointText(p: SelPoint | undefined, d0: number): string {
  if (!p) return "–";
  return p.t == null ? distanceText(p.d - d0) : `${distanceText(p.d - d0)} · ${duration(p.t * 1000)}`;
}

/**
 * Höhenprofil mit Auswahl und zwei gekoppelten Reglern (RFC-0016). Die Strecke zeigt eine gebrochene Achse:
 * erster und letzter Kilometer breit, die Mitte als schmale Lücke.
 */
export function SelectionBar({ points, axis, kind, range, onChange, onHover }: Props) {
  const [s, e] = range;
  const d0 = points[0]?.d ?? 0;
  const at = (i: number) => (points[i]?.d ?? d0) - d0;
  const rail = useRef<HTMLDivElement>(null);
  const svg = useRef<SVGSVGElement>(null);
  const drag = useRef<{ which: "s" | "e" | "band"; m0: number; cs: number; ce: number } | null>(null);
  const [cursor, setCursor] = useState<number | null>(null);

  const profile = useMemo(() => {
    const shown = (m: number) => !axis.broken || m <= axis.edge || m >= axis.totalM - axis.edge;
    const heights = points.flatMap((p) => (p.ele != null && shown(p.d - d0) ? [p.ele] : []));
    if (heights.length < 2) return null;
    const lo = Math.min(...heights);
    let hi = Math.max(...heights);
    if (hi - lo < 10) hi = lo + 10;
    const step = axis.broken ? 1 : Math.max(1, Math.floor(points.length / W));
    const parts: [number, number][][] = [];
    let cur: [number, number][] | null = null;
    for (let i = 0; i < points.length; i += step) {
      const p = points[i]!;
      if (!shown(p.d - d0)) {
        cur = null;
        continue;
      }
      if (p.ele == null) continue;
      if (!cur) parts.push((cur = []));
      cur.push([axis.fx(p.d - d0) * W, H - 4 - ((p.ele - lo) / (hi - lo)) * (H - 10)]);
    }
    const path = (pp: [number, number][]) => pp.map(([x, y], k) => `${k ? "L" : "M"}${x.toFixed(1)},${y.toFixed(1)}`).join("");
    return {
      line: parts.map(path).join(""),
      area: parts
        .filter((pp) => pp.length > 1)
        .map((pp) => `M${pp[0]![0].toFixed(1)},${H}` + path(pp).replace(/^M/, "L") + `L${pp[pp.length - 1]![0].toFixed(1)},${H}Z`)
        .join(""),
    };
  }, [points, axis, d0]);

  const fraction = (clientX: number, box: DOMRect) => Math.min(1, Math.max(0, (clientX - box.left) / box.width));
  const indexAtX = (clientX: number, box: DOMRect) => idxAt(points, axis.fm(fraction(clientX, box)));

  const down = (which: "s" | "e" | "band", ev: PointerEvent<HTMLElement>) => {
    if (which === "band" && kind === "route") return;
    ev.preventDefault();
    ev.stopPropagation();
    ev.currentTarget.setPointerCapture?.(ev.pointerId);
    const box = rail.current!.getBoundingClientRect();
    drag.current = { which, m0: axis.fm(fraction(ev.clientX, box)), cs: at(s), ce: at(e) };
  };
  const move = (ev: PointerEvent<HTMLElement>) => {
    const d = drag.current;
    if (!d || !rail.current) return;
    const box = rail.current.getBoundingClientRect();
    const m = axis.fm(fraction(ev.clientX, box));
    if (d.which === "s") onChange([Math.min(idxAt(points, m), e - 1), e]);
    else if (d.which === "e") onChange([s, Math.max(idxAt(points, m), s + 1)]);
    else onChange(shiftBand(points, d.cs, d.ce, m - d.m0));
  };
  const up = () => (drag.current = null);
  // Beim Dauerdruck folgen Schritte schneller als neu gezeichnet wird: vom zuletzt gesetzten Stand weiterrechnen.
  const latest = useRef(range);
  useLayoutEffect(() => {
    latest.current = range;
  }, [range]);
  const nudge = (which: "s" | "e", by: number) => {
    const [cs, ce] = latest.current;
    const next: Range = which === "s" ? [Math.min(Math.max(0, cs + by), ce - 1), ce] : [cs, Math.max(Math.min(points.length - 1, ce + by), cs + 1)];
    latest.current = next;
    onChange(next);
  };
  const key = (which: "s" | "e") => (ev: KeyboardEvent) => {
    const k = ({ ArrowLeft: -1, ArrowDown: -1, ArrowRight: 1, ArrowUp: 1, PageDown: -100, PageUp: 100 } as Record<string, number>)[ev.key];
    if (!k) return;
    ev.preventDefault();
    nudge(which, ev.shiftKey ? k * 10 : k);
  };

  const ls = axis.fx(at(s)) * 100;
  const le = axis.fx(at(e)) * 100;
  const ticks = axis.broken
    ? [0, axis.edge / 2, axis.edge, axis.totalM - axis.edge, axis.totalM - axis.edge / 2, axis.totalM]
    : [0, 1, 2, 3, 4, 5, 6].map((k) => (axis.totalM * k) / 6);
  const thumb = "absolute top-1.5 h-[22px] w-[18px] -ml-[9px] cursor-ew-resize rounded-md border-2 border-white shadow focus-visible:outline-3 focus-visible:outline-accent";
  const fine = "grid min-h-9 min-w-9 place-items-center rounded-md border border-line bg-card-2 p-1 hover:border-accent";
  const hovered = cursor == null ? undefined : points[cursor];

  return (
    <div className="grid gap-1 rounded-xl border border-line bg-card-2 px-3 pt-2 pb-3">
      <div className="relative">
        <svg
          ref={svg}
          viewBox={`0 0 ${W} ${H}`}
          preserveAspectRatio="none"
          className="block h-[90px] w-full cursor-crosshair"
          onPointerMove={(ev) => {
            const i = indexAtX(ev.clientX, ev.currentTarget.getBoundingClientRect());
            setCursor(i);
            onHover(i);
          }}
          onPointerLeave={() => {
            setCursor(null);
            onHover(null);
          }}
          onClick={(ev) => onChange(moveNearer(range, indexAtX(ev.clientX, ev.currentTarget.getBoundingClientRect())))}
          aria-hidden="true"
        >
          <defs>
            <clipPath id="tc-sel-clip">
              <rect x={(ls / 100) * W} y={0} width={Math.max(1, ((le - ls) / 100) * W)} height={H} />
            </clipPath>
          </defs>
          {profile && (
            <>
              <path d={profile.area} className="fill-muted opacity-30" />
              <path d={profile.area} className="fill-accent opacity-60" clipPath="url(#tc-sel-clip)" />
              <path d={profile.line} className="fill-none stroke-muted" strokeWidth={1.2} vectorEffect="non-scaling-stroke" />
            </>
          )}
          {axis.broken && (
            <g className="stroke-muted" strokeWidth={1.5} vectorEffect="non-scaling-stroke">
              <line x1={GAP_A * W + 8} y1={H - 8} x2={GAP_A * W + 18} y2={H - 30} />
              <line x1={(GAP_A + GAP_W) * W - 18} y1={H - 8} x2={(GAP_A + GAP_W) * W - 8} y2={H - 30} />
            </g>
          )}
        </svg>
        {axis.broken && (
          <div
            className="pointer-events-none absolute top-6 text-center text-[10px] leading-tight text-muted"
            style={{ left: `${GAP_A * 100}%`, width: `${GAP_W * 100}%` }}
          >
            {t("create.gap", { km: num((axis.totalM - 2 * axis.edge) / 1000, 1) })}
          </div>
        )}
        {cursor != null && hovered && (
          <>
            <div className="pointer-events-none absolute top-0 h-full border-l border-dashed border-text" style={{ left: `${axis.fx(at(cursor)) * 100}%` }} />
            <div className="pointer-events-none absolute top-0 right-0 rounded bg-card px-1.5 text-[11px]">
              {distanceText(at(cursor))} · {hovered.ele == null ? "–" : `${num(hovered.ele)} m`}
            </div>
          </>
        )}
      </div>

      <div
        ref={rail}
        className="relative mx-[9px] h-[34px] touch-none select-none"
        onPointerDown={(ev) => {
          if (ev.target !== ev.currentTarget) return;
          onChange(moveNearer(range, indexAtX(ev.clientX, ev.currentTarget.getBoundingClientRect())));
        }}
        onPointerMove={move}
        onPointerUp={up}
        onPointerCancel={up}
      >
        <div className="pointer-events-none absolute inset-x-0 top-[15px] h-1 rounded bg-line" />
        <div
          className={`absolute top-3 h-2.5 rounded-full bg-accent ${kind === "route" ? "" : "cursor-grab active:cursor-grabbing"}`}
          style={{ left: `${ls}%`, width: `${le - ls}%` }}
          title={kind === "route" ? undefined : t("create.bandHint")}
          onPointerDown={(ev) => down("band", ev)}
        />
        {axis.broken && (
          <div
            className="pointer-events-none absolute top-1 h-[26px] bg-card-2 text-center leading-[26px] text-muted"
            style={{ left: `${GAP_A * 100}%`, width: `${GAP_W * 100}%` }}
          >
            ≈
          </div>
        )}
        {(["s", "e"] as const).map((which) => {
          const i = which === "s" ? s : e;
          return (
            <div
              key={which}
              role="slider"
              tabIndex={0}
              aria-label={t(which === "s" ? "create.start" : "create.end")}
              aria-valuemin={0}
              aria-valuemax={points.length - 1}
              aria-valuenow={i}
              aria-valuetext={pointText(points[i], d0)}
              className={`${thumb} ${which === "s" ? START : END}`}
              style={{ left: `${which === "s" ? ls : le}%` }}
              onPointerDown={(ev) => down(which, ev)}
              onKeyDown={key(which)}
            />
          );
        })}
      </div>

      <div className="relative mx-[9px] h-4 text-[11px] text-muted">
        {ticks.map((m, k) => {
          const left = k === 0 || (axis.broken && k === 3);
          const right = k === ticks.length - 1 || (axis.broken && k === 2);
          return (
            <span
              key={k}
              className={`absolute top-0 whitespace-nowrap ${left ? "" : right ? "-translate-x-full" : "-translate-x-1/2"}`}
              style={{ left: `${axis.fx(m) * 100}%` }}
            >
              {num(m / 1000, 1)} km
            </span>
          );
        })}
      </div>

      <div className="mt-1 grid grid-cols-1 gap-2 sm:grid-cols-2">
        {(["s", "e"] as const).map((which) => {
          const i = which === "s" ? s : e;
          return (
            <div key={which} className="flex items-center gap-2 rounded-lg border border-line bg-card px-2 py-1">
              <span className={`h-2.5 w-2.5 rounded-full ${which === "s" ? START : END}`} />
              <span className="text-sm font-semibold">{t(which === "s" ? "create.start" : "create.end")}</span>
              <RepeatButton className={fine} label={t("create.back")} onStep={(n) => nudge(which, -n)}>
                <ChevronLeft size={14} />
              </RepeatButton>
              <span className="num flex-1 text-center text-sm" data-testid={`point-${which}`}>
                {pointText(points[i], d0)}
              </span>
              <RepeatButton className={fine} label={t("create.forward")} onStep={(n) => nudge(which, n)}>
                <ChevronRight size={14} />
              </RepeatButton>
            </div>
          );
        })}
      </div>
    </div>
  );
}

const DH = 110;

/**
 * Vergrößertes Höhenprofil um die Auswahl (Uli 12.11.2026): Bei kurzen Segmenten auf langen Fahrten ist die
 * Auswahl im Gesamtprofil nur ein schmaler Streifen. Der Ausschnitt zeigt die Auswahl plus Rand; Klick setzt den
 * näheren Punkt, Hover zeigt die Stelle auf der Karte. Er folgt der Auswahl, gezogen wird im Gesamtprofil.
 */
export function DetailProfile({ points, range, onChange, onHover }: { points: SelPoint[]; range: Range; onChange: (r: Range) => void; onHover: (i: number | null) => void }) {
  const [s, e] = range;
  const { from, to } = detailWindow(points, range);
  const [cursor, setCursor] = useState<number | null>(null);
  const span = Math.max(to - from, 1);
  const fx = (d: number) => ((d - from) / span) * W;
  const view = useMemo(() => points.filter((p) => p.d >= from && p.d <= to), [points, from, to]);
  const heights = view.flatMap((p) => (p.ele == null ? [] : [p.ele]));
  if (heights.length < 2) return null;
  const lo = Math.min(...heights);
  let hi = Math.max(...heights);
  if (hi - lo < 5) hi = lo + 5;
  const fy = (h: number) => DH - 4 - ((h - lo) / (hi - lo)) * (DH - 12);
  const line = view.flatMap((p) => (p.ele == null ? [] : [`${fx(p.d).toFixed(1)},${fy(p.ele).toFixed(1)}`]));
  const path = `M${line.join("L")}`;
  const firstX = fx(view.find((p) => p.ele != null)!.d);
  const lastX = fx([...view].reverse().find((p) => p.ele != null)!.d);
  const area = `M${firstX.toFixed(1)},${DH}L${line.join("L")}L${lastX.toFixed(1)},${DH}Z`;
  const xs = fx(points[s]!.d);
  const xe = fx(points[e]!.d);
  const indexAt = (clientX: number, box: DOMRect) => idxAt(points, from + Math.min(1, Math.max(0, (clientX - box.left) / box.width)) * span - (points[0]?.d ?? 0));
  const hovered = cursor == null ? undefined : points[cursor];
  const d0 = points[0]?.d ?? 0;
  return (
    <div className="grid gap-1 rounded-xl border border-line bg-card-2 px-3 pt-2 pb-2" data-testid="detail-profile">
      <div className="flex items-center justify-between text-[11px] text-muted">
        <span>{t("create.detail")}</span>
        <span className="num">
          {hovered ? `${distanceText(hovered.d - d0)} · ${hovered.ele == null ? "–" : `${num(hovered.ele)} m`}` : `${distanceText(from - d0)} – ${distanceText(to - d0)}`}
        </span>
      </div>
      <svg
        viewBox={`0 0 ${W} ${DH}`}
        preserveAspectRatio="none"
        className="block h-[110px] w-full cursor-crosshair"
        aria-hidden="true"
        onPointerMove={(ev) => {
          const i = indexAt(ev.clientX, ev.currentTarget.getBoundingClientRect());
          setCursor(i);
          onHover(i);
        }}
        onPointerLeave={() => {
          setCursor(null);
          onHover(null);
        }}
        onClick={(ev) => onChange(moveNearer(range, indexAt(ev.clientX, ev.currentTarget.getBoundingClientRect())))}
      >
        <defs>
          <clipPath id="tc-detail-clip">
            <rect x={xs} y={0} width={Math.max(1, xe - xs)} height={DH} />
          </clipPath>
        </defs>
        <path d={area} className="fill-muted opacity-30" />
        <path d={area} className="fill-accent opacity-60" clipPath="url(#tc-detail-clip)" />
        <path d={path} className="fill-none stroke-muted" strokeWidth={1.2} vectorEffect="non-scaling-stroke" />
        <line x1={xs} x2={xs} y1={0} y2={DH} stroke="#16a34a" strokeWidth={2} vectorEffect="non-scaling-stroke" />
        <line x1={xe} x2={xe} y1={0} y2={DH} stroke="#dc2626" strokeWidth={2} vectorEffect="non-scaling-stroke" />
        {hovered && <line x1={fx(hovered.d)} x2={fx(hovered.d)} y1={0} y2={DH} className="stroke-text" strokeDasharray="4 3" vectorEffect="non-scaling-stroke" />}
      </svg>
    </div>
  );
}
