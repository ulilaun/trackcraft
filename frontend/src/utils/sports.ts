// Sportklassen an einer Stelle (RFC-0008 Abschnitt 4): Reihenfolge für Filter, Stapel und Legenden.
import type { RankedSportClass, SportClass, SubSportCatalog, SubSportEntry } from "../api/client";
import { SPORT_GLYPH_IDS, type SportGlyphId } from "../components/glyphs";
import { hasText, t } from "../i18n";

/** Alle Sportklassen, `unknown` zuletzt. */
export const SPORT_CLASSES: SportClass[] = ["cycling", "ebike", "running", "skiing", "swimming", "studio", "unknown"];

/** Sportklassen mit Segmenten, Strecken, Bestenlisten und Rekorden (Uli 14.11.2026: Skifahren ja, Schwimmen und Studio nicht). */
export const RANKED_CLASSES: RankedSportClass[] = ["cycling", "ebike", "running", "skiing"];

/** Ein Wert 0 je Sportklasse, z. B. für Stapelbalken. */
export function zeroBySport(): Record<SportClass, number> {
  return { cycling: 0, ebike: 0, running: 0, skiing: 0, swimming: 0, studio: 0, unknown: 0 };
}

/** Anzeige eines Untertyps: eigener Name, sonst Übersetzung des Schlüssels, sonst der Schlüssel. */
export function subSportEntryLabel(e: SubSportEntry): string {
  if (e.name) return e.name;
  const k = `subSport.${e.key}`;
  return hasText(k) ? t(k) : e.key;
}

/** Eintrag eines Untertyp-Schlüssels im Katalog; unbekannte Schlüssel als eigener Eintrag ohne Namen. */
export function subSportEntry(catalog: SubSportCatalog | undefined, sportClass: string, key: string): SubSportEntry {
  return catalog?.[sportClass]?.find((x) => x.key === key) ?? { key, name: null, icon: null, builtin: false };
}

/** Anzeige des Untertyp-Schlüssels einer Aktivität in ihrer Sportklasse; leer ohne Untertyp. */
export function subSportLabel(catalog: SubSportCatalog | undefined, sportClass: string, key: string | null | undefined): string {
  if (!key) return "";
  return subSportEntryLabel(subSportEntry(catalog, sportClass, key));
}

/** Symbol je Sportklasse. */
export const SPORT_CLASS_GLYPH: Record<SportClass, SportGlyphId> = {
  cycling: "bike",
  ebike: "ebike",
  running: "run",
  skiing: "ski",
  swimming: "swim",
  studio: "studio",
  unknown: "unknown",
};

/** Vorgabesymbol der eingebauten Untertypen; eigene ohne Wahl bekommen das Etikett. */
const DEFAULT_SUB_GLYPH: Record<string, SportGlyphId> = {
  "cycling/road": "road_bike",
  "cycling/mtb": "mtb",
  "cycling/indoor": "indoor_bike",
  "running/street": "street_run",
  "running/trail": "trail_run",
  "running/walking": "walk",
  "skiing/nordic": "nordic",
  "skiing/skating": "skating",
  "swimming/indoor": "pool",
  "studio/strength": "strength",
};

/** Vorgabesymbol eines Untertyps ohne eigene Wahl. */
export function defaultSubSportGlyph(sportClass: string, key: string): SportGlyphId {
  return DEFAULT_SUB_GLYPH[`${sportClass}/${key}`] ?? "tag";
}

/** Symbol eines Untertyps: eigene Wahl, sonst Vorgabe. */
export function subSportGlyph(sportClass: string, e: SubSportEntry): SportGlyphId {
  return e.icon && SPORT_GLYPH_IDS.includes(e.icon) ? e.icon : defaultSubSportGlyph(sportClass, e.key);
}
