/**
 * Symbol für „Darstellung wie System“: ein „A“ in einem Pfeilkreis (automatisch), im Stil der Lucide-Icons
 * (24er Raster, Strich 2, runde Enden), damit es neben Sonne und Mond passt.
 */
export function AutoThemeIcon({ size = 18 }: { size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      data-testid="auto-theme-icon"
    >
      {/* Pfeilkreis (wie Lucide „rotate-cw“) */}
      <path d="M21 12a9 9 0 1 1-9-9c2.52 0 4.93 1 6.74 2.74L21 8" />
      <path d="M21 3v5h-5" />
      {/* A */}
      <path d="M9 16l3-8 3 8" />
      <path d="M10.1 13.2h3.8" />
    </svg>
  );
}
