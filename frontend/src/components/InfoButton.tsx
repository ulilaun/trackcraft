import { useEffect, useId, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { Info, X } from "lucide-react";
import { t } from "../i18n";

/**
 * Kleines ⓘ, das ein Erklärfenster über allem öffnet (auch über einem {@link Dialog}). Escape, Klick auf den
 * Hintergrund oder „Schließen“ schließen nur das Erklärfenster; danach steht der Fokus wieder auf dem ⓘ.
 */
export function InfoButton({ title, children }: { title: string; children: ReactNode }) {
  const [open, setOpen] = useState(false);
  const button = useRef<HTMLButtonElement>(null);
  const close = () => {
    setOpen(false);
    button.current?.focus();
  };
  return (
    <>
      <button
        ref={button}
        type="button"
        className="inline-grid size-5 shrink-0 place-items-center rounded-full text-muted hover:text-accent focus-visible:text-accent"
        aria-label={t("info.open", { title })}
        aria-haspopup="dialog"
        onClick={() => setOpen(true)}
      >
        <Info size={15} aria-hidden />
      </button>
      {open && createPortal(<InfoPanel title={title} onClose={close}>{children}</InfoPanel>, document.body)}
    </>
  );
}

function InfoPanel({ title, onClose, children }: { title: string; onClose: () => void; children: ReactNode }) {
  const titleId = useId();
  const closeButton = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    closeButton.current?.focus();
    // Escape vor dem umgebenden Dialog abfangen, sonst schließt er mit
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.stopImmediatePropagation();
      e.preventDefault();
      onClose();
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [onClose]);

  return (
    <div
      className="fixed inset-0 z-[2100] grid place-items-center bg-black/30 p-3"
      // Ereignisse aus dem Portal laufen im React-Baum zum umgebenden Dialog weiter; dort nichts auslösen
      onPointerDown={(e) => e.stopPropagation()}
      onPointerUp={(e) => {
        e.stopPropagation();
        if (e.target === e.currentTarget) onClose();
      }}
      onClick={(e) => e.stopPropagation()}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        className="grid max-h-[calc(100dvh-1.5rem)] w-full max-w-lg grid-rows-[auto_1fr] rounded-card border border-line bg-card shadow-xl"
      >
        <div className="flex items-center gap-3 border-b border-line px-4 py-3">
          <Info size={18} className="text-accent" aria-hidden />
          <h2 id={titleId} className="flex-1 text-base font-semibold">
            {title}
          </h2>
          <button
            ref={closeButton}
            type="button"
            className="-m-1 rounded-lg p-2 text-muted hover:text-text"
            onClick={onClose}
            aria-label={t("dialog.close")}
          >
            <X size={18} />
          </button>
        </div>
        <div className="grid min-h-0 gap-3 overflow-y-auto overscroll-contain px-4 py-3 text-sm leading-relaxed">{children}</div>
      </div>
    </div>
  );
}

/** Ein Eintrag im Erklärfenster: Begriff fett, darunter die Erklärung. */
export function InfoItem({ term, children }: { term: string; children: ReactNode }) {
  return (
    <div>
      <div className="font-semibold">{term}</div>
      <div className="text-muted">{children}</div>
    </div>
  );
}
