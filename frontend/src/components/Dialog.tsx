import { useEffect, useId, useRef, type ReactNode } from "react";
import { X } from "lucide-react";
import { t } from "../i18n";

/**
 * Modales Fenster im Kachel-Design über der Seite. Escape und Antippen bzw. Klick auf den Hintergrund schließen,
 * solange `busy` nicht gesetzt ist. Höhe nach dem sichtbaren Bereich (`dvh`), Fuß mit den Knöpfen bleibt sichtbar. Eigenes Element statt `<dialog>`, weil jsdom `showModal` nicht kennt.
 */
export function Dialog({
  title,
  onClose,
  busy = false,
  wide = false,
  footer,
  children,
}: {
  title: string;
  onClose: () => void;
  busy?: boolean;
  /** Breites Fenster für Karte und Profil (Segment erstellen). */
  wide?: boolean;
  footer: ReactNode;
  children: ReactNode;
}) {
  const titleId = useId();
  const box = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !busy) onClose();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [busy, onClose]);

  // Seite hinter dem Dialog sperren, sonst scrollt sie auf dem Tablet mit
  useEffect(() => {
    const body = document.body;
    const before = body.style.overflow;
    body.style.overflow = "hidden";
    return () => {
      body.style.overflow = before;
    };
  }, []);

  // Hintergrund schließt nur bei Druck und Loslassen dort ohne nennenswerte Bewegung: ein Wisch, der auf dem
  // Hintergrund beginnt oder endet, schließt nicht.
  const press = useRef<{ x: number; y: number } | null>(null);

  useEffect(() => {
    const first = box.current?.querySelector<HTMLElement>("input, textarea, select, button[data-autofocus]");
    first?.focus();
  }, []);

  return (
    <div
      className="fixed inset-0 z-[2000] grid place-items-center overscroll-contain bg-black/40 p-2 sm:p-4"
      onPointerDown={(e) => {
        press.current = e.target === e.currentTarget ? { x: e.clientX, y: e.clientY } : null;
      }}
      onPointerUp={(e) => {
        const p = press.current;
        press.current = null;
        if (p && e.target === e.currentTarget && Math.hypot(e.clientX - p.x, e.clientY - p.y) < 10 && !busy) onClose();
      }}
    >
      <div
        ref={box}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        className={`grid max-h-[calc(100dvh-1rem)] w-full sm:max-h-[94dvh] ${wide ? "max-w-6xl" : "max-w-xl"} grid-rows-[auto_1fr_auto] rounded-card border border-line bg-card shadow-xl`}
      >
        <div className="flex items-center gap-3 border-b border-line px-4 py-3">
          <h2 id={titleId} className="flex-1 text-lg font-semibold">
            {title}
          </h2>
          <button type="button" className="-m-1 rounded-lg p-2 text-muted hover:text-text" onClick={onClose} disabled={busy} aria-label={t("dialog.close")}>
            <X size={18} />
          </button>
        </div>
        <div className="min-h-0 overflow-y-auto overscroll-contain px-4 py-3">{children}</div>
        <div className="flex flex-wrap justify-end gap-2 border-t border-line px-4 py-3">{footer}</div>
      </div>
    </div>
  );
}

export const primaryButton =
  "rounded-lg bg-accent px-4 py-1.5 text-sm font-medium text-white hover:bg-accent-strong disabled:opacity-50 disabled:hover:bg-accent";
export const dangerButton = "rounded-lg bg-danger px-4 py-1.5 text-sm font-medium text-white hover:opacity-90 disabled:opacity-50";
export const secondaryButton = "rounded-lg border border-line bg-card-2 px-4 py-1.5 text-sm hover:border-accent disabled:opacity-50";
