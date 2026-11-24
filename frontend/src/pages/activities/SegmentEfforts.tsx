import { Link } from "react-router-dom";
import { Flag, Route as RouteIcon } from "lucide-react";
import type { Effort } from "../../api/client";
import { Card } from "../../components/Card";
import { CategoryIcon } from "../../components/CategoryIcon";
import { t, type TextKey } from "../../i18n";
import { num, segmentTime, speed } from "../../utils/format";
import { segmentPath } from "../segments/segmentModel";

/** Zeichen für die persönliche Bestzeit; Silber und Bronze gibt es nicht (Uli 12.11.2026). */
export function Medal({ rank }: { rank: number | null }) {
  if (rank !== 1) return <span className="inline-block w-7" />;
  return (
    <span className="inline-grid h-6 w-7 place-items-center rounded-full bg-gold text-[10px] font-bold text-black" title={t("efforts.medal.1")}>
      PR
    </span>
  );
}

/** Ergebnis in Worten: Erste Bestzeit, Verbesserung mit Zeitersparnis, Einstellung oder Platz. */
export function resultText(e: Effort): string | null {
  if (e.record_kind === "first_best") return t("efforts.firstBest");
  if (e.record_kind === "improvement") return t("efforts.improvement", { saved: segmentTime(e.improvement_ms) });
  if (e.record_kind === "equaled") return t("efforts.equaled");
  if (e.rank === 2 || e.rank === 3) return t("efforts.place", { rank: e.rank });
  return null;
}

/** Strecken zeigt die Aktivitätsseite getrennt von den Segmenten (Uli 23.11.2026). */
export const isRoute = (e: Effort) => e.segment?.kind === "route";

/** Höchstens so viele Top-Ergebnisse der Segmente (Uli: „2–3“). */
export const TOP_LIMIT = 3;

/** Top-Ergebnisse: Segmentversuche mit Bestzeit oder Platz 2 und 3, Bestzeiten zuerst, dann nach Platz. */
export function topEfforts(efforts: Effort[]): Effort[] {
  return efforts
    .filter((e) => e.status === "valid" && (e.record_kind != null || (e.rank != null && e.rank <= 3)))
    .sort((a, b) => Number(b.record_kind != null) - Number(a.record_kind != null) || (a.rank ?? 99) - (b.rank ?? 99))
    .slice(0, TOP_LIMIT);
}

const tile = "flex items-center gap-3 rounded-xl border border-line bg-card-2 px-3 py-2 hover:border-accent";
const createButton =
  "flex items-center gap-1.5 self-start rounded-lg border border-line bg-card-2 px-3 py-1.5 text-sm hover:border-accent disabled:opacity-40 disabled:hover:border-line";

/** Strecke der Aktivität: Rang, Zahl der gewerteten Versuche und heutige Bestzeit. */
function RouteTile({ e }: { e: Effort }) {
  const ranked = e.status === "valid" && e.rank != null;
  const details = [
    ranked ? t("efforts.routeRank", { rank: e.rank ?? "–", of: e.segment?.ranked_count ?? "–" }) : t(`efforts.notRanked.${e.not_ranked_reason ?? "time_invalid"}` as TextKey),
    // „Platz n“ stünde schon im Rang, nur Bestzeit-Meldungen ergänzen
    ranked && e.record_kind != null ? resultText(e) : null,
    e.segment?.best_ms != null && e.rank !== 1 ? t("efforts.routeBest", { time: segmentTime(e.segment.best_ms) }) : null,
  ].filter(Boolean);
  return (
    <Link to={segmentPath("route", e.segment_id)} state={{ from: "activity", activityId: e.activity_id }} className={tile}>
      <Medal rank={ranked ? e.rank : null} />
      <span className="min-w-0 flex-1">
        <span className="block truncate font-medium">{e.segment_name}</span>
        <span className="block text-xs text-muted">{details.join(" · ")}</span>
      </span>
      <span className="num">{segmentTime(e.duration_ms)}</span>
    </Link>
  );
}

/**
 * Kachel oben auf der Aktivitätsseite, zweigeteilt (Uli 23.11.2026): links die erkannte Strecke mit Rang und Bestzeit
 * oder „Strecke erstellen“, rechts bis zu drei Top-Ergebnisse der Segmente und „Segment erstellen“.
 */
export function TopResults({
  efforts,
  loaded,
  canCreate,
  onCreate,
}: {
  efforts: Effort[];
  loaded: boolean;
  canCreate: boolean;
  onCreate: (kind: "segment" | "route") => void;
}) {
  const routes = efforts.filter(isRoute);
  const top = topEfforts(efforts.filter((e) => !isRoute(e)));
  const heading = "mb-2 text-xs font-medium uppercase tracking-wide text-muted";
  return (
    <Card title={t("efforts.top")}>
      <div className="grid grid-cols-1 gap-4 lg:grid-cols-[minmax(0,2fr)_minmax(0,3fr)]">
        <section className="flex flex-col gap-2" aria-label={t("efforts.routeHeading")}>
          <h3 className={heading}>{t("efforts.routeHeading")}</h3>
          {routes.length > 0 ? (
            routes.map((e) => <RouteTile key={e.id} e={e} />)
          ) : (
            <>
              <p className="text-sm text-muted">{loaded ? t("efforts.noRoute") : t("state.loading")}</p>
              <button type="button" className={createButton} disabled={!canCreate || !loaded} onClick={() => onCreate("route")}>
                <RouteIcon size={14} />
                {t("detail.createRoute")}
              </button>
            </>
          )}
        </section>
        <section className="flex flex-col gap-2" aria-label={t("efforts.title")}>
          <h3 className={heading}>{t("efforts.title")}</h3>
          {top.length > 0 ? (
            <ul className="grid gap-2 sm:grid-cols-2 xl:grid-cols-3">
              {top.map((e) => (
                <li key={e.id}>
                  <Link to={segmentPath(e.segment?.kind, e.segment_id)} state={{ from: "activity", activityId: e.activity_id }} className={tile}>
                    <Medal rank={e.rank} />
                    <span className="min-w-0 flex-1">
                      <span className="block truncate font-medium">{e.segment_name}</span>
                      <span className="block text-xs text-muted">{resultText(e)}</span>
                    </span>
                    <span className="num">{segmentTime(e.duration_ms)}</span>
                  </Link>
                </li>
              ))}
            </ul>
          ) : (
            loaded && <p className="text-sm text-muted">{t("efforts.noTop")}</p>
          )}
          <button type="button" className={createButton} disabled={!canCreate} onClick={() => onCreate("segment")}>
            <Flag size={14} />
            {t("detail.createSegment")}
          </button>
        </section>
      </div>
    </Card>
  );
}

function SegmentInfo({ e }: { e: Effort }) {
  const s = e.segment;
  if (!s) return null;
  const parts = [
    `${num(s.length_m / 1000, 2)} km`,
    s.elevation_gain_m == null ? null : `${num(s.elevation_gain_m)} m ↑`,
    s.kind === "route" || s.average_grade_pct == null ? null : `${num(s.average_grade_pct, 1)} %`,
  ].filter(Boolean);
  return <span className="block text-xs text-muted">{parts.join(" · ")}</span>;
}

/**
 * Segmentversuche der Aktivität (RFC-0006): Medaille, Name mit Länge, Aufstieg und Steigung, Zeit in Zehnteln,
 * Schnitt bzw. Pace, HF Ø und max, Rang „n von m“, Zeitersparnis. Hover hebt den Versuch auf Karte und Diagramm hervor.
 */
export function EffortTable({ efforts, running, calculating, onHover }: { efforts: Effort[]; running: boolean; calculating: boolean; onHover: (e: Effort | null) => void }) {
  const th = "px-2 py-1.5 text-left text-xs font-medium text-muted";
  const td = "px-2 py-1.5";
  return (
    <Card title={t("efforts.title")}>
      {calculating && <p className="mb-2 text-sm text-muted">{t("efforts.calculating")}</p>}
      {efforts.length === 0 ? (
        <p className="text-sm text-muted">{t("efforts.none")}</p>
      ) : (
        <div className="overflow-x-auto">
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b border-line">
                <th className={th} />
                <th className={th}>{t("efforts.col.segment")}</th>
                <th className={`${th} text-right`}>{t("efforts.col.time")}</th>
                <th className={`${th} text-right`}>{t(running ? "efforts.col.pace" : "efforts.col.speed")}</th>
                <th className={`${th} text-right`}>{t("efforts.col.hr")}</th>
                <th className={`${th} text-right`}>{t("efforts.col.rank")}</th>
                <th className={`${th} text-right`}>{t("efforts.col.saved")}</th>
              </tr>
            </thead>
            <tbody>
              {efforts.map((e) => {
                const avg =
                  e.duration_ms && e.segment ? speed((e.segment.length_m / (e.duration_ms / 1000)) * 3.6, e.segment.sport_class === "running") : null;
                return (
                  <tr
                    key={e.id}
                    className="border-b border-line last:border-0 hover:bg-card-2"
                    onMouseEnter={() => onHover(e)}
                    onMouseLeave={() => onHover(null)}
                  >
                    <td className={td}>
                      <Medal rank={e.status === "valid" ? e.rank : null} />
                    </td>
                    <td className={td}>
                      <Link to={segmentPath(e.segment?.kind, e.segment_id)} state={{ from: "activity", activityId: e.activity_id }} className="flex items-center gap-1.5 font-medium hover:text-accent">
                        <CategoryIcon category={e.segment?.category} size={14} />
                        {e.segment_name}
                      </Link>
                      <SegmentInfo e={e} />
                    </td>
                    <td className={`${td} num text-right`}>{segmentTime(e.duration_ms)}</td>
                    <td className={`${td} num text-right`}>{avg ? `${avg.value} ${avg.unit}` : "–"}</td>
                    <td className={`${td} num text-right`}>
                      {e.avg_heart_rate_bpm ?? "–"} / {e.max_heart_rate_bpm ?? "–"}
                    </td>
                    <td className={`${td} text-right`}>
                      {e.status === "valid" && e.rank != null ? (
                        <span className="num">{t("efforts.rankOf", { rank: e.rank, of: e.segment?.ranked_count ?? "–" })}</span>
                      ) : (
                        <span className="text-xs text-muted">{t(`efforts.notRanked.${e.not_ranked_reason ?? "time_invalid"}` as TextKey)}</span>
                      )}
                    </td>
                    <td className={`${td} num text-right text-good`}>{e.improvement_ms ? `−${segmentTime(e.improvement_ms)}` : ""}</td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
    </Card>
  );
}
