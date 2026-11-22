import type { SegmentCategory, SegmentKind } from "../api/client";
import { t, type TextKey } from "../i18n";
import { Glyph } from "./Glyph";

const colors: Record<SegmentCategory, string> = {
  climb: "text-danger",
  descent: "text-good",
  rolling: "text-accent",
  loop: "text-accent",
  out_and_back: "text-accent",
  point_to_point: "text-accent",
};

export const SEGMENT_CATEGORIES: SegmentCategory[] = ["climb", "descent", "rolling"];
export const ROUTE_CATEGORIES: SegmentCategory[] = ["loop", "out_and_back", "point_to_point"];

export function categoriesFor(kind: SegmentKind): SegmentCategory[] {
  return kind === "route" ? ROUTE_CATEGORIES : SEGMENT_CATEGORIES;
}

export function categoryText(c: SegmentCategory): string {
  return t(`category.${c}` as TextKey);
}

/** Symbol der Kategorie aus dem eigenen Satz (RFC-0016 „Kategorien“); ohne Kategorie nichts. */
export function CategoryIcon({
  category,
  size = 16,
  label = false,
  decorative = false,
}: {
  category: SegmentCategory | null | undefined;
  size?: number;
  label?: boolean;
  /** nur Schmuck neben eigenem Text (z. B. in Auswahllisten) */
  decorative?: boolean;
}) {
  if (!category) return null;
  const text = categoryText(category);
  if (decorative) return <Glyph id={category} size={size} className={colors[category]} />;
  return (
    <span className="inline-flex items-center gap-1.5" title={text}>
      <Glyph id={category} size={size} className={colors[category]} label={label ? undefined : text} />
      {label && <span>{text}</span>}
    </span>
  );
}

/** Symbol der Art Segment oder Strecke. */
export function SegmentKindIcon({ kind, size = 16, className = "" }: { kind: SegmentKind; size?: number; className?: string }) {
  return <Glyph id={kind} size={size} className={className} />;
}
