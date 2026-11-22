// Symbolwahl eines Untertyps in den Einstellungen: Raster aller Sport-Symbole plus „Vorgabe“.
import { useEffect, useRef, useState } from "react";
import { Glyph } from "../../components/Glyph";
import { SPORT_GLYPH_IDS, type SportGlyphId } from "../../components/glyphs";
import { sportColor } from "../../components/SportIcon";
import type { SportClass } from "../../api/client";
import { t, type TextKey } from "../../i18n";

export function glyphName(id: SportGlyphId): string {
  return t(`glyph.${id}` as TextKey);
}

export function GlyphPicker({
  sport,
  name,
  current,
  chosen,
  fallback,
  disabled,
  onPick,
}: {
  sport: SportClass;
  /** Name des Untertyps */
  name: string;
  /** angezeigtes Symbol (Wahl oder Vorgabe) */
  current: SportGlyphId;
  /** eigene Wahl; `null` = Vorgabe */
  chosen: SportGlyphId | null;
  fallback: SportGlyphId;
  disabled: boolean;
  onPick: (icon: SportGlyphId | null) => void;
}) {
  const [open, setOpen] = useState(false);
  const box = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const away = (e: PointerEvent) => {
      if (!box.current?.contains(e.target as Node)) setOpen(false);
    };
    const esc = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    document.addEventListener("pointerdown", away);
    document.addEventListener("keydown", esc);
    return () => {
      document.removeEventListener("pointerdown", away);
      document.removeEventListener("keydown", esc);
    };
  }, [open]);
  const pick = (icon: SportGlyphId | null) => {
    setOpen(false);
    if (icon !== chosen) onPick(icon);
  };
  const cell = "grid h-9 w-9 place-items-center rounded-lg border hover:border-accent";

  return (
    <div ref={box} className="relative">
      <button
        type="button"
        className={`grid h-7 w-7 place-items-center rounded-md hover:bg-card disabled:opacity-60 ${sportColor(sport)}`}
        disabled={disabled}
        aria-haspopup="dialog"
        aria-expanded={open}
        aria-label={t("sports.iconFor", { name })}
        title={t("sports.iconFor", { name })}
        onClick={() => setOpen((o) => !o)}
      >
        <Glyph id={current} size={18} />
      </button>
      {open && (
        <div
          role="dialog"
          aria-label={t("sports.iconFor", { name })}
          className="absolute left-0 z-40 mt-1 grid w-[248px] gap-2 rounded-lg border border-line bg-card p-2 shadow-lg"
        >
          <div className="grid grid-cols-6 gap-1.5">
            {SPORT_GLYPH_IDS.map((id) => (
              <button
                key={id}
                type="button"
                aria-pressed={chosen === id}
                aria-label={glyphName(id)}
                title={glyphName(id)}
                onClick={() => pick(id)}
                className={`${cell} ${chosen === id ? "border-accent bg-card-2" : "border-line"} ${sportColor(sport)}`}
              >
                <Glyph id={id} size={18} />
              </button>
            ))}
          </div>
          <button
            type="button"
            aria-pressed={chosen === null}
            onClick={() => pick(null)}
            className={`flex items-center gap-2 rounded-lg border px-2 py-1 text-sm ${chosen === null ? "border-accent bg-card-2" : "border-line"}`}
          >
            <span className={sportColor(sport)}>
              <Glyph id={fallback} size={16} />
            </span>
            {t("sports.iconDefault")}
          </button>
        </div>
      )}
    </div>
  );
}
