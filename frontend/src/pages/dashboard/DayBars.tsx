import { Bar, BarChart, CartesianGrid, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import type { SportClass } from "../../api/client";
import { t, type TextKey } from "../../i18n";
import { num } from "../../utils/format";
import type { DayBar } from "./dashboardModel";

const SWATCH: Record<SportClass, string> = {
  cycling: "bg-cycling",
  ebike: "bg-ebike",
  running: "bg-running",
  skiing: "bg-skiing",
  swimming: "bg-swimming",
  studio: "bg-studio",
  unknown: "bg-unknown",
};

function DayTooltip({ active, payload, classes }: { active?: boolean; payload?: { payload: DayBar }[]; classes: SportClass[] }) {
  const d = payload?.[0]?.payload;
  if (!active || !d || d.count === 0) return null;
  const total = classes.reduce((s, c) => s + d[c], 0);
  const [y, m, day] = d.date.split("-");
  return (
    <div className="rounded-lg border border-line bg-card px-3 py-2 text-xs shadow">
      <div className="mb-1 font-semibold">{d.title ?? `${day}.${m}.${y}`}</div>
      {classes
        .filter((c) => d[c] > 0)
        .map((c) => (
          <div key={c} className="flex items-center gap-2">
            <span className={`inline-block h-2 w-2 rounded-sm ${SWATCH[c]}`} />
            <span className="flex-1">{t(`sport.${c}` as TextKey)}</span>
            <span className="num">{num(d[c], 1)} km</span>
          </div>
        ))}
      {classes.filter((c) => d[c] > 0).length > 1 && (
        <div className="mt-1 flex justify-between border-t border-line pt-1 font-semibold">
          <span>{t("dashboard.total")}</span>
          <span className="num">{num(total, 1)} km</span>
        </div>
      )}
    </div>
  );
}

/** Distanz je Tag, nach Sportklasse gestapelt in deren Farbe (RFC-0018). */
export function DayBars({ bars, classes }: { bars: DayBar[]; classes: SportClass[] }) {
  return (
    <div className="h-36">
      <ResponsiveContainer width="100%" height="100%">
        <BarChart data={bars} margin={{ top: 4, right: 4, bottom: 0, left: -18 }}>
          <CartesianGrid stroke="var(--tc-line)" vertical={false} />
          <XAxis
            dataKey="label"
            interval={bars.length > 7 ? "preserveStartEnd" : 0}
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
            width={44}
          />
          <Tooltip cursor={{ fill: "var(--tc-card-2)" }} content={<DayTooltip classes={classes} />} isAnimationActive={false} />
          {classes.map((c, i) => (
            <Bar
              key={c}
              dataKey={c}
              stackId="d"
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
