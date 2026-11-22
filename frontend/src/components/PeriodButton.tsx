// Zeitraum-Knöpfe (Bestenliste, Aktivitätenliste): schließen sich gegenseitig aus; Jahr und Saison als geteilte Knöpfe.
import type { ReactNode } from "react";
import type { Season } from "../api/client";
import { t } from "../i18n";
import { SeasonDot, seasonStateText } from "./SeasonDot";
import { SplitPill } from "./SplitPill";

/** Auswahlfeld neben einem aktiven Zeitraum-Knopf; mit eigenem Hintergrund, damit die Liste im dunklen Thema lesbar bleibt. */
export const periodSelect = "rounded-full border border-line bg-card-2 px-3 py-1 text-sm text-text";

export function PeriodButton({
  on,
  onClick,
  title,
  children,
}: {
  on: boolean;
  onClick: () => void;
  title?: string;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      aria-pressed={on}
      onClick={onClick}
      title={title}
      className={`inline-flex items-center gap-1.5 rounded-full border px-3 py-1 text-sm ${on ? "border-transparent bg-accent-strong text-white" : "border-line bg-card-2 text-muted hover:text-text"}`}
    >
      {children}
    </button>
  );
}

/** Geteilter Knopf „Jahr | 2026 ▾“: links wählt das Jahr als Zeitraum, das Menü ein anderes Jahr. */
export function YearPill({ on, year, years, onPick, onSelect }: { on: boolean; year: number; years: number[]; onPick: () => void; onSelect: (year: number) => void }) {
  return (
    <SplitPill
      label={t("segment.board.yearPlain")}
      on={on}
      onPick={onPick}
      value={<span className="num">{year}</span>}
      options={years.map((y) => ({ key: String(y), label: String(y) }))}
      selected={String(year)}
      onSelect={(k) => k && onSelect(Number(k))}
      menuLabel={t("segment.board.yearSelect")}
    />
  );
}

/** Geteilter Knopf „Saison | ● Name ▾“; der Punkt zeigt den Zustand. */
export function SeasonPill({
  on,
  season,
  seasons,
  onPick,
  onSelect,
}: {
  on: boolean;
  season: Season;
  seasons: Season[];
  onPick: () => void;
  onSelect: (id: string) => void;
}) {
  return (
    <SplitPill
      label={t("segment.board.season")}
      on={on}
      onPick={onPick}
      title={`${season.name} · ${seasonStateText(season.state)}`}
      value={
        <>
          <SeasonDot state={season.state} onAccent={on} />
          {season.name}
        </>
      }
      options={seasons.map((s) => ({ key: s.id, label: s.name, icon: <SeasonDot state={s.state} /> }))}
      selected={season.id}
      onSelect={(k) => k && onSelect(k)}
      menuLabel={t("segment.board.seasonSelect")}
    />
  );
}
