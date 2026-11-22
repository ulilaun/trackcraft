// Darstellung des eigenen Symbolsatzes. Die Zeichnungen sind feste Texte aus glyphs.ts.
import { SEGMENT_GLYPHS, SPORT_GLYPHS, type SegmentGlyphId, type SportGlyphId } from "./glyphs";

/**
 * Ein Symbol in `currentColor`. Mit `label` hat es einen zugänglichen Namen und einen Tooltip,
 * ohne ist es Schmuck neben sichtbarem Text.
 */
export function Glyph({
  id,
  size = 16,
  className = "",
  label,
}: {
  id: SportGlyphId | SegmentGlyphId;
  size?: number;
  className?: string;
  label?: string;
}) {
  const markup = (SPORT_GLYPHS as Record<string, string>)[id] ?? (SEGMENT_GLYPHS as Record<string, string>)[id] ?? SPORT_GLYPHS.unknown;
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.8}
      strokeLinecap="round"
      strokeLinejoin="round"
      className={`shrink-0 ${className}`}
      data-glyph={id}
      role={label ? "img" : undefined}
      aria-label={label}
      aria-hidden={label ? undefined : true}
    >
      {label && <title>{label}</title>}
      {/* feste Zeichnung aus glyphs.ts, keine Benutzerdaten */}
      <g dangerouslySetInnerHTML={{ __html: markup }} />
    </svg>
  );
}
