import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link } from "react-router-dom";
import { apiDelete, apiGet, apiPost, apiPut, type ActivityPlanResponse, type PlannedUnit } from "../../api/client";
import { Card } from "../../components/Card";
import { primaryButton, secondaryButton } from "../../components/Dialog";
import { Problem } from "../../components/Problem";
import { t } from "../../i18n";
import { num } from "../../utils/format";
import { hm } from "../plan/planModel";

const ZONE_COLORS = ["#94a3b8", "#22c55e", "#eab308", "#f97316", "#ef4444"];

function ZoneBar({ label, values }: { label: string; values: number[] | null | undefined }) {
  const sum = values?.reduce((a, b) => a + b, 0) ?? 0;
  return (
    <div className="grid gap-1">
      <span className="text-xs text-muted">{label}</span>
      {sum > 0 ? (
        <span className="flex h-3 overflow-hidden rounded bg-card-2">
          {values!.map((v, i) => (
            <i key={i} style={{ width: `${(v / sum) * 100}%`, background: ZONE_COLORS[i] }} title={`Z${i + 1}`} />
          ))}
        </span>
      ) : (
        <span className="text-xs text-muted">–</span>
      )}
    </div>
  );
}

const signedNum = (v: number) => (Math.round(v) === 0 ? "±0" : `${v > 0 ? "+" : "−"}${num(Math.abs(v))}`);
const signedTime = (min: number) => (Math.round(min) === 0 ? "±0:00" : `${min > 0 ? "+" : "−"}${hm(Math.abs(min))}`);

/** Vergleich Plan ↔ Ist einer verknüpften Einheit (E8). */
function Comparison({ u }: { u: PlannedUnit }) {
  const a = u.actual;
  if (!a) return null;
  const actMin = a.total_time_ms / 60000;
  const rows: [string, string, string, string][] = [
    [t("plan.duration"), hm(u.load.minutes), hm(actMin), signedTime(actMin - u.load.minutes)],
    [t("plan.load"), num(u.load.total), a.load != null ? num(a.load) : "–", a.load != null ? signedNum(a.load - u.load.total) : "–"],
    [t("plan.cmp.anaerobic"), num(u.load.anaerobic), a.anaerobic != null ? num(a.anaerobic) : "–", a.anaerobic != null ? signedNum(a.anaerobic - u.load.anaerobic) : "–"],
  ];
  return (
    <div className="grid gap-3">
      <table className="w-full text-sm">
        <thead>
          <tr className="text-xs text-muted">
            <th className="py-1 text-left font-medium" />
            <th className="py-1 text-right font-medium">{t("plan.cmp.planned")}</th>
            <th className="py-1 text-right font-medium">{t("plan.cmp.actual")}</th>
            <th className="py-1 text-right font-medium">{t("plan.cmp.diff")}</th>
          </tr>
        </thead>
        <tbody>
          {rows.map(([k, p, i, d]) => (
            <tr key={k} className="border-t border-line">
              <td className="py-1.5">{k}</td>
              <td className="num py-1.5 text-right">{p}</td>
              <td className="num py-1.5 text-right">{i}</td>
              <td className="num py-1.5 text-right">{d}</td>
            </tr>
          ))}
        </tbody>
      </table>
      <div className="grid gap-3 sm:grid-cols-2">
        <ZoneBar label={t("plan.cmp.zonesPlanned")} values={u.zone_minutes} />
        <ZoneBar label={t("plan.cmp.zonesActual")} values={a.zone_seconds} />
      </div>
    </div>
  );
}

/**
 * Aktivitätsseite (E8): Vorschlag „Geplant war …“ mit Verknüpfen / Nicht verknüpfen, nach dem Verknüpfen der
 * Vergleich Plan ↔ Ist. Ohne Plan an diesem Tag erscheint nichts.
 */
export function PlannedUnitCard({ activityId }: { activityId: string }) {
  const queryClient = useQueryClient();
  const key = ["activities", "plan-match", activityId];
  const q = useQuery({ queryKey: key, queryFn: () => apiGet<ActivityPlanResponse>(`/activities/${activityId}/plan`) });
  const refresh = () => {
    void queryClient.invalidateQueries({ queryKey: key });
    void queryClient.invalidateQueries({ queryKey: ["plan"] });
    void queryClient.invalidateQueries({ queryKey: ["form"] });
  };
  const link = useMutation({ mutationFn: (unit: string) => apiPut(`/plan/units/${unit}/activity`, { activity_id: activityId }), onSuccess: refresh });
  const unlink = useMutation({ mutationFn: (unit: string) => apiDelete(`/plan/units/${unit}/activity`), onSuccess: refresh });
  const decline = useMutation({ mutationFn: (unit: string) => apiPost(`/plan/units/${unit}/decline`, { activity_id: activityId }), onSuccess: refresh });
  const d = q.data?.data;
  const busy = link.isPending || unlink.isPending || decline.isPending;
  const error = link.error ?? unlink.error ?? decline.error;

  if (d?.linked) {
    const u = d.linked;
    return (
      <Card title={t("plan.cmp.title")} className="grid gap-3">
        <div className="flex flex-wrap items-center justify-between gap-2 text-sm">
          <span>
            {t("plan.cmp.unit")} <b>{u.title}</b>
          </span>
          <span className="flex gap-2">
            <Link to="/plan" className={secondaryButton}>
              {t("plan.toPlan")}
            </Link>
            <button type="button" className={secondaryButton} disabled={busy} onClick={() => unlink.mutate(u.id)}>
              {t("plan.unlink")}
            </button>
          </span>
        </div>
        <Comparison u={u} />
        {error && <Problem error={error} />}
      </Card>
    );
  }
  if (!d || d.candidates.length === 0) return null;
  return (
    <section className="grid gap-3 rounded-card border border-accent bg-card p-4 shadow-sm" data-testid="plan-match">
      {d.candidates.map((u) => (
        <div key={u.id} className="flex flex-wrap items-center justify-between gap-2">
          <div>
            <div className="font-semibold">{t("plan.match.title", { title: u.title })}</div>
            <div className="num text-xs text-muted">
              {hm(u.load.minutes)} h · {t("plan.load")} {num(u.load.total)}
            </div>
          </div>
          <div className="flex flex-wrap gap-2">
            <button type="button" className={primaryButton} disabled={busy} onClick={() => link.mutate(u.id)}>
              {t("plan.link")}
            </button>
            <button type="button" className={secondaryButton} disabled={busy} onClick={() => decline.mutate(u.id)}>
              {t("plan.decline")}
            </button>
          </div>
        </div>
      ))}
      <p className="text-xs text-muted">{t("plan.match.hint")}</p>
      {error && <Problem error={error} />}
    </section>
  );
}
