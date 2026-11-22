// Seite „Sportarten“: reine Logik für Zuordnungstabelle und Untertyp-Katalog, ohne React.
import type { SportClass, SportMapping, SubSport } from "../../api/client";
import { SPORT_CLASSES } from "../../utils/sports";

/** Sportklassen mit Untertyp-Katalog (alle außer `unknown`). */
export const CATALOG_CLASSES: SportClass[] = SPORT_CLASSES.filter((c) => c !== "unknown");

/** Ein Typ fällt auf, wenn vorhandene Aktivitäten ihn haben und ein neuer Import ihn nicht zuordnen würde. */
export function needsAttention(m: SportMapping): boolean {
  return m.activity_count > 0 && m.effective_sport_class === "unknown";
}

/** Auffällige Typen zuerst, sonst alphabetisch wie vom Server. */
export function sortMappings(list: SportMapping[]): SportMapping[] {
  return [...list.filter(needsAttention), ...list.filter((m) => !needsAttention(m))];
}

/** Herkunft der wirksamen Zuordnung eines Typs ohne eigenen Eintrag: der Sport vor dem ersten `-`. */
export function inheritedFrom(m: SportMapping): string | null {
  if (m.mapped || m.effective_sport_class === "unknown") return null;
  const i = m.source_sport.indexOf("-");
  return i > 0 ? m.source_sport.slice(0, i) : null;
}

/** Untertyp-Einträge einer Klasse in Anzeigereihenfolge (die Liste kommt sortiert). */
export function subsOf(list: SubSport[], sportClass: SportClass): SubSport[] {
  return list.filter((s) => s.sport_class === sportClass);
}

/** PUT-Inhalt nach Wahl einer Sportklasse: der Untertyp bleibt nur, wenn die neue Klasse ihn kennt. */
export function classChange(m: SportMapping, sportClass: SportClass, list: SubSport[]): { sport_class: SportClass; sub_sport: string | null } {
  const keep = m.sub_sport !== null && subsOf(list, sportClass).some((s) => s.key === m.sub_sport);
  return { sport_class: sportClass, sub_sport: keep ? m.sub_sport : null };
}

/** Normalisierung eines neuen Typs wie beim GPX-Import: Kleinbuchstaben, andere Zeichen außer `-` werden `_`. */
export function normalizeSource(input: string): string {
  return input
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9-]+/g, "_")
    .replace(/^_+|_+$/g, "");
}
