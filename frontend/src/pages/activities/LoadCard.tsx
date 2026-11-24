import type { ActivityLoad, HrZoneSet } from "../../api/client";
import { Card } from "../../components/Card";
import { t, type TextKey } from "../../i18n";
import { dayDate, duration, num } from "../../utils/format";

function zoneSetName(s: HrZoneSet): string {
  return s.sport_class ? t(`sport.${s.sport_class}` as TextKey) : t("zones.general");
}

/** Block „Belastung“ im Aktivitätsdetail: TRIMP, aerob/anaerob und Zeit in den HF-Zonen (E7). */
export function LoadCard({ load }: { load: ActivityLoad | null | undefined }) {
  if (load === undefined) return null;
  if (load === null) {
    return (
      <Card title={t("load.title")}>
        <p className="text-sm text-muted">{t("load.pending")}</p>
      </Card>
    );
  }
  const z = load.zone_set;
  const secs = load.zone_seconds;
  const total = secs?.reduce((a, b) => a + b, 0) ?? 0;
  const longest = secs ? Math.max(1, ...secs) : 1;
  const estimated = load.method === "estimated";
  return (
    <Card>
      <div className="mb-3 flex flex-wrap items-baseline justify-between gap-2">
        <h2 className="text-xs font-semibold uppercase tracking-wide text-muted">{t("load.title")}</h2>
        {z && <span className="text-xs text-muted">{t("load.zoneSet", { name: zoneSetName(z), date: dayDate(z.valid_from) })}</span>}
      </div>
      <div className="grid grid-cols-3 gap-3">
        <div>
          <div className="text-xs font-medium text-muted">{t("load.title")}</div>
          <div className="num text-2xl font-semibold" data-testid="load-trimp">
            {num(load.trimp)}
          </div>
          <div className="text-xs text-muted">{estimated ? t("form.estimated") : t("load.trimp")}</div>
        </div>
        {!estimated && (
          <>
            <div>
              <div className="text-xs font-medium text-muted">{t("load.aerobic")}</div>
              <div className="num text-2xl font-semibold text-aerobic">{num(load.aerobic)}</div>
              {z && <div className="text-xs text-muted">{t("load.below", { bpm: z.anaerobic_from_bpm })}</div>}
            </div>
            <div>
              <div className="text-xs font-medium text-muted">{t("load.anaerobic")}</div>
              <div className="num text-2xl font-semibold text-anaerobic">{num(load.anaerobic)}</div>
              {z && <div className="text-xs text-muted">{t("load.above", { bpm: z.anaerobic_from_bpm })}</div>}
            </div>
          </>
        )}
      </div>
      {load.method !== "samples" && <p className="mt-3 text-xs text-muted">{t(`load.method.${load.method}` as TextKey)}</p>}
      {secs && z && total > 0 && (
        <div className="mt-4 grid gap-1.5" data-testid="load-zones">
          <h3 className="text-xs font-semibold uppercase tracking-wide text-muted">{t("load.zones")}</h3>
          {secs.map((s, i) => (
            <div key={i} className="grid grid-cols-[minmax(0,9.5rem)_minmax(0,1fr)_auto] items-center gap-3 text-sm">
              <span className="flex items-center gap-2 truncate">
                <i className="inline-block h-2.5 w-2.5 shrink-0 rounded-sm" style={{ background: `var(--color-zone-${i + 1})` }} />
                Z{i + 1} {t(`zones.name.${i + 1}` as TextKey)}
                <span className="num text-xs text-muted">{z.zones_bpm[i]?.[0]}+</span>
              </span>
              <span className="h-3 rounded bg-card-2">
                <span className="block h-full rounded" style={{ width: `${(s / longest) * 100}%`, background: `var(--color-zone-${i + 1})` }} />
              </span>
              <span className="num whitespace-nowrap text-right">
                {duration(s * 1000)} <span className="text-xs text-muted">{num((s / total) * 100)} %</span>
              </span>
            </div>
          ))}
        </div>
      )}
    </Card>
  );
}
