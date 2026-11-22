// Eigener Symbolsatz (Uli 15.11.2026): Piktogramme im 24er-Raster, Strich 1,8, Köpfe gefüllt.
// Von Grund auf gezeichnet (Entwurf trackcraft-entwurf/symbole); keine bearbeiteten fremden Symbole.
// Die Kennungen der Sport-Symbole stehen auch im Backend (sports::ICONS), weil Untertypen eines wählen können.

const head = (x: number, y: number, r = 1.8) => `<circle fill="currentColor" stroke="none" cx="${x}" cy="${y}" r="${r}"/>`;
const wheels = `<circle cx="5" cy="17.5" r="3.6"/><circle cx="19" cy="17.5" r="3.6"/>`;
const frame = `<path d="M5 17.5H10.5L8 11.2M10.5 17.5L15.6 11.2H8M15.6 11.2L19 17.5"/>`;
const riderUp = head(14.4, 4.3) + `<path d="M8 11L12.6 6.9L15.4 10.2M8 11L12 13.4L10.5 17.5"/>`;
// Rad etwas höher, damit darunter Platz für den Boden ist (Rennrad/MTB)
const bikeHigh = `<circle cx="5" cy="15.5" r="3.5"/><circle cx="19" cy="15.5" r="3.5"/><path d="M5 15.5H10.5L8.4 9.6M10.5 15.5L15.8 9.6H8.4M15.8 9.6L19 15.5"/>`;
const waves = `M2 17Q4.5 15 7 17T12 17T17 17T22 17M2 21Q4.5 19 7 21T12 21T17 21T22 21`;
const baseline = `<path d="M2.5 21.5H21.5" stroke-dasharray="0.1 3"/>`;

/** Sport-Symbole für Klassen und Untertypen; ein Untertyp kann jedes davon wählen. */
export const SPORT_GLYPHS = {
  bike: wheels + frame + riderUp,
  road_bike:
    bikeHigh + head(17, 4.6) + `<path d="M8.4 9L14.6 6.2M14.6 6.2L15.2 9L16.8 10.4M8.4 9L12.6 11L11.2 15.5M8.4 9L10.4 12.8L9.8 16.6M2 22.5H6M10 22.5H14M18 22.5H22"/>`,
  mtb:
    bikeHigh +
    head(15.8, 2.8) +
    `<path d="M9.8 7.6L13.8 4.8M13.8 4.8L14 7.8L15.8 9.4M9.8 7.6L12.6 10.6L11.4 15.2M9.8 7.6L9.2 11.6L9.8 15.8M1.5 22.4L6 20.4L9.5 21.8L15 19.8L18 21.2L22.5 19.6"/>`,
  indoor_bike:
    `<circle cx="5" cy="15.5" r="3.6"/><path d="M5 15.5H10.5L8 9.2M10.5 15.5L15.6 9.2H8M15.6 9.2L18.5 15.5M2 21.5L5 15.5L8 21.5M16.5 15.5H20.5V21.5H16.5Z"/>` +
    head(14.4, 2.3) +
    `<path d="M8 9L12.6 4.9L15.4 8.2M8 9L12 11.4L10.5 15.5"/>`,
  ebike: wheels + frame + riderUp + `<path d="M5.2 2L2.6 6.6H5.4L3.4 10.4"/>`,
  run: head(15, 3.4) + `<path d="M13.8 6.8L11 12.4M13.6 7.2L16.2 9.8L18.6 8.4M13.6 7.2L10.2 8.4L8.6 11M11 12.4L14.6 15.2L13.2 20.2M11 12.4L8.4 15.6L5 15.4"/>`,
  street_run:
    head(15, 2.6) + `<path d="M13.8 6L11 11.6M13.6 6.4L16.2 9L18.6 7.6M13.6 6.4L10.2 7.6L8.6 10.2M11 11.6L14.6 14.4L13.2 19M11 11.6L8.4 14.8L5 14.6M2 22.5H6M10 22.5H14M18 22.5H22"/>`,
  trail_run:
    head(15, 2.6) +
    `<path d="M13.8 6L11 11.6M13.6 6.4L16.2 9L18.6 7.6M13.6 6.4L10.2 7.6L8.6 10.2M11 11.6L14.6 14.4L13.4 18.6M11 11.6L8.4 14.8L5 14.6M1.5 22.5L6 20L9.5 21.6L15 18.8L18 20.4L22.5 18.4"/>`,
  walk: head(12.4, 3.4) + `<path d="M12 6.8L11.4 13M11.9 7.4L9.4 10.6L9 13.4M11.9 7.4L14.4 10.2L15.6 12.4M11.4 13L9.2 17.4L8 21.4M11.4 13L13.6 17L15.6 21.4"/>`,
  ski:
    head(13.6, 3.6) +
    `<path d="M12.8 7L9.6 11.2M12.6 7.6L14.8 10.6M14.8 10.6L17.6 17M9.6 11.2L13 14L11.4 17.9M9.6 11.2L11.4 15.8L9.4 19.7M3 15.2L19.6 20.6Q21.2 21 21.4 19.8M3 17.6L18.4 22.6Q20 23 20.2 21.8"/>`,
  nordic:
    head(12.4, 3.4) +
    `<path d="M12 6.8L11.4 13M11.9 7.4L15 10.4L12.8 20.6M11.9 7.4L8.6 10L3.6 19.6M11.4 13L14.2 16.4L15.8 20.6M11.4 13L9.4 17L7 20.6M1.5 21.6H10M12.5 21.6H22.5"/>`,
  skating:
    head(12.6, 3.4) +
    `<path d="M12.2 6.8L11.2 12.4M12 7.4L15 9.8L12.8 20.2M12 7.4L8.6 9.4L4.8 19.8M11.2 12.4L13.8 15.8L15 19.8M11.2 12.4L8.4 15.2L5.8 17.8M10.8 20.6L22.4 19.4M1.6 15.4L8.6 20.6"/>`,
  swim: head(17.6, 8.6) + `<path d="M4.6 12.6L10.6 10.2L15.4 11.6M10.6 10.2L13 5.4L17 4.4${waves}"/>`,
  pool: head(7.2, 8.6) + `<path d="M2.4 11.6L6.6 7.4L10.4 9.4M16.5 2.5V14M20.5 2.5V14M16.5 5.5H20.5M16.5 9H20.5${waves}"/>`,
  studio: `<path d="M6.5 6.5V17.5M17.5 6.5V17.5M3.5 9.5V14.5M20.5 9.5V14.5M6.5 12H17.5"/>`,
  strength: head(12, 8) + `<path d="M2.5 3.5H21.5M4.5 1.5V5.5M19.5 1.5V5.5M8 3.5L9 7.6L12 10.6L15 7.6L16 3.5M12 10.6V15M12 15L8.8 21.4M12 15L15.2 21.4"/>`,
  unknown: `<circle cx="12" cy="12" r="9.5"/><path d="M9.4 9.4A2.7 2.7 0 1 1 13 11.9C12.3 12.3 12 12.8 12 13.6"/>` + head(12, 16.8, 1.1),
  tag: `<path d="M3 3H11L21 13L13 21L3 11Z"/>` + head(7.5, 7.5, 1.4),
  star: `<path d="M12 2.5L14.9 8.6L21.5 9.4L16.6 13.9L17.9 20.5L12 17.2L6.1 20.5L7.4 13.9L2.5 9.4L9.1 8.6Z"/>`,
  mountain: `<path d="M2 20L9 7L13 14L15.5 10L22 20Z"/>`,
  flag: `<path d="M5 21.5V3M5 3.5H18L15.5 8L18 12.5H5"/>`,
} as const;

export type SportGlyphId = keyof typeof SPORT_GLYPHS;
export const SPORT_GLYPH_IDS = Object.keys(SPORT_GLYPHS) as SportGlyphId[];

/** Segment- und Streckentypen (RFC-0016 „Kategorien“) und die beiden Arten. */
export const SEGMENT_GLYPHS = {
  climb: `<path d="M2.5 20.5L9 14L12.5 16L21.5 6.5M16 6.5H21.5V12"/>` + baseline,
  descent: `<path d="M2.5 6.5L11.5 16L15 14L21.5 20.5M16 20.5H21.5V15"/>` + baseline,
  rolling: `<path d="M2 15Q5 8 8 13T14 13T20 11L22 12"/>` + baseline,
  loop:
    `<path d="M8 19.5C3.5 18 2.5 12.5 5 9C7.5 5 14 3.5 18 6.5C21.5 9.2 21 15 17.5 18C15.8 19.4 13.6 19.9 11.6 19.8M14.6 17.4L11.6 19.8L14.2 22.4"/>` +
    head(8, 19.5),
  out_and_back: `<path d="M3.5 9H18.5M15.5 6L18.5 9L15.5 12M20.5 15H5.5M8.5 12L5.5 15L8.5 18"/>` + head(3.5, 9, 1.6),
  point_to_point: `<path d="M4.5 19C9 19 8.5 10.5 13 10.5C16 10.5 16.5 13 18.5 13M18.5 13V3.5L22 5.4L18.5 7.3"/>` + head(4.5, 19),
  segment: `<path d="M2.5 17C6 17 7.5 12 10.5 11S16 13 21.5 7M2.5 13.5V20.5M21.5 3.5V10.5"/>`,
  route: `<path d="M5 18.5C2.5 15 3.5 9 8 7S16 8 18.5 5.5M5 18.5H13C17 18.5 21 16 20.5 12M18.5 5.5V1.8M18.5 1.8L21.6 3.4L18.5 5"/>` + head(5, 18.5),
} as const;

export type SegmentGlyphId = keyof typeof SEGMENT_GLYPHS;
