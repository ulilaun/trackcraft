import { Bar, BarChart, CartesianGrid, Cell, Pie, PieChart, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import type { SportClass } from "../../api/client";
import { t, type TextKey } from "../../i18n";
import { num } from "../../utils/format";
import type { StackBar } from "./statsModel";

const SWATCH: Record<SportClass, string> = {
  cycling: "bg-cycling",
  ebike: "bg-ebike",
  running: "bg-running",
  skiing: "bg-skiing",
  swimming: "bg-swimming",
  studio: "bg-studio",
  unknown: "bg-unknown",
};

/** Wert mit Einheit: km mit einer Nachkommastelle, Stunden als h:mm. */
export function chartValue(v: number, unit: "km" | "h"): string {
  if (unit === "km") return `${num(v, 1)} km`;
  const min = Math.round(v * 60);
  return `${Math.floor(min / 60)}:${String(min % 60).padStart(2, "0")} h`;
}

function StackTooltip({
  active,
  payload,
  classes,
  unit,
}: {
  active?: boolean;
  payload?: { payload: StackBar }[];
  classes: SportClass[];
  unit: "km" | "h";
}) {
  const d = payload?.[0]?.payload;
  if (!active || !d) return null;
  const used = classes.filter((c) => d[c] > 0);
  if (used.length === 0) return null;
  const total = used.reduce((s, c) => s + d[c], 0);
  return (
    <div className="rounded-lg border border-line bg-card px-3 py-2 text-xs shadow">
      <div className="mb-1 font-semibold">{d.title}</div>
      {used.map((c) => (
        <div key={c} className="flex items-center gap-2">
          <span className={`inline-block h-2 w-2 rounded-sm ${SWATCH[c]}`} />
          <span className="flex-1">{t(`sport.${c}` as TextKey)}</span>
          <span className="num">{chartValue(d[c], unit)}</span>
        </div>
      ))}
      {used.length > 1 && (
        <div className="mt-1 flex justify-between gap-4 border-t border-line pt-1 font-semibold">
          <span>{t("dashboard.total")}</span>
          <span className="num">{chartValue(total, unit)}</span>
        </div>
      )}
    </div>
  );
}

/** Gestapelte Balken nach Sportklasse in deren Farbe (RFC-0006 „Statistik“). */
export function StackedBars({ bars, classes, unit, height = 200 }: { bars: StackBar[]; classes: SportClass[]; unit: "km" | "h"; height?: number }) {
  return (
    <div style={{ height }}>
      <ResponsiveContainer width="100%" height="100%">
        <BarChart data={bars} margin={{ top: 4, right: 4, bottom: 0, left: -12 }}>
          <CartesianGrid stroke="var(--tc-line)" vertical={false} />
          <XAxis
            dataKey="label"
            interval={bars.length > 14 ? "preserveStartEnd" : 0}
            tick={{ fill: "var(--tc-muted)", fontSize: 11 }}
            stroke="var(--tc-line)"
            tickLine={false}
          />
          <YAxis
            allowDecimals={false}
            tickFormatter={(v: number) => num(v)}
            tick={{ fill: "var(--tc-muted)", fontSize: 11 }}
            stroke="var(--tc-line)"
            tickLine={false}
            width={48}
          />
          <Tooltip cursor={{ fill: "var(--tc-card-2)" }} content={<StackTooltip classes={classes} unit={unit} />} isAnimationActive={false} />
          {classes.map((c, i) => (
            <Bar
              key={c}
              dataKey={c}
              stackId="s"
              fill={`var(--color-${c})`}
              radius={i === classes.length - 1 ? [3, 3, 0, 0] : 0}
              isAnimationActive={false}
            />
          ))}
        </BarChart>
      </ResponsiveContainer>
    </div>
  );
}

/** Ring mit den Anteilen der Sportklassen, Legende darunter. */
export function ShareRing({ parts, unit }: { parts: { sport: SportClass; value: number; share: number }[]; unit: "km" | "h" }) {
  return (
    <div className="grid content-start gap-2">
      <div className="h-36">
        <ResponsiveContainer width="100%" height="100%">
          <PieChart>
            <Pie data={parts} dataKey="value" nameKey="sport" innerRadius="58%" outerRadius="92%" stroke="none" isAnimationActive={false}>
              {parts.map((p) => (
                <Cell key={p.sport} fill={`var(--color-${p.sport})`} />
              ))}
            </Pie>
          </PieChart>
        </ResponsiveContainer>
      </div>
      {/* Zwei Zeilen je Klasse: Name und Anteil oben, Wert darunter. So bleibt alles in der schmalen Spalte. */}
      <ul className="grid gap-1.5 text-xs">
        {parts.map((p) => (
          <li key={p.sport} className="grid grid-cols-[0.5rem_minmax(0,1fr)_auto] items-center gap-x-2">
            <span className={`inline-block h-2 w-2 rounded-sm ${SWATCH[p.sport]}`} />
            <span className="truncate">{t(`sport.${p.sport}` as TextKey)}</span>
            <span className="num whitespace-nowrap text-right">{num(p.share * 100)} %</span>
            <span className="num col-start-2 col-span-2 whitespace-nowrap text-muted">{chartValue(p.value, unit)}</span>
          </li>
        ))}
      </ul>
    </div>
  );
}
