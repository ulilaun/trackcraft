import type { SportClass, SubSportCatalog, SubSportEntry } from "../api/client";
import { t, type TextKey } from "../i18n";
import { SPORT_CLASS_GLYPH, subSportEntry, subSportEntryLabel, subSportGlyph } from "../utils/sports";
import { Glyph } from "./Glyph";

const colors: Record<SportClass, string> = {
  cycling: "text-cycling",
  ebike: "text-ebike",
  running: "text-running",
  skiing: "text-skiing",
  swimming: "text-swimming",
  studio: "text-studio",
  unknown: "text-unknown",
};

/** Farbe einer Sportklasse als Textklasse (für `currentColor`). */
export function sportColor(sport: SportClass): string {
  return colors[sport] ?? colors.unknown;
}

/**
 * Sportklassen-Symbol aus dem eigenen Satz in der Sportklassenfarbe.
 * `plain`: ohne Klassenfarbe (z. B. auf gefülltem Akzentknopf); `decorative`: ohne zugänglichen Namen, wenn Text danebensteht.
 */
export function SportIcon({ sport, size = 16, plain = false, decorative = false }: { sport: SportClass; size?: number; plain?: boolean; decorative?: boolean }) {
  return (
    <Glyph
      id={SPORT_CLASS_GLYPH[sport] ?? "unknown"}
      size={size}
      className={plain ? "" : sportColor(sport)}
      label={decorative ? undefined : t(`sport.${sport}` as TextKey)}
    />
  );
}

/**
 * Untertyp-Symbol: eigene Wahl, sonst Vorgabe; Name als Tooltip. Entweder `entry` oder `catalog` + `subKey`.
 */
export function SubSportIcon({
  sport,
  entry,
  catalog,
  subKey,
  size = 16,
  plain = false,
  decorative = false,
}: {
  sport: SportClass;
  entry?: SubSportEntry;
  catalog?: SubSportCatalog;
  subKey?: string | null;
  size?: number;
  plain?: boolean;
  decorative?: boolean;
}) {
  const e = entry ?? (subKey ? subSportEntry(catalog, sport, subKey) : undefined);
  if (!e) return null;
  return (
    <Glyph id={subSportGlyph(sport, e)} size={size} className={plain ? "" : sportColor(sport)} label={decorative ? undefined : subSportEntryLabel(e)} />
  );
}

/**
 * Symbol einer Aktivität: Untertyp-Symbol, sobald ein Untertyp zugeordnet ist, sonst das Klassensymbol.
 * Name als Tooltip „Radfahren · Rennrad“.
 */
export function ActivitySportIcon({
  sport,
  subKey,
  catalog,
  size = 16,
  plain = false,
  decorative = false,
}: {
  sport: SportClass;
  subKey?: string | null;
  catalog?: SubSportCatalog;
  size?: number;
  /** ohne Klassenfarbe, etwa weiß auf farbigem Kreis */
  plain?: boolean;
  decorative?: boolean;
}) {
  const e = subKey ? subSportEntry(catalog, sport, subKey) : undefined;
  if (!e) return <SportIcon sport={sport} size={size} plain={plain} decorative={decorative} />;
  const label = `${t(`sport.${sport}` as TextKey)} · ${subSportEntryLabel(e)}`;
  return <Glyph id={subSportGlyph(sport, e)} size={size} className={plain ? "" : sportColor(sport)} label={decorative ? undefined : label} />;
}
