// Knopf „Herunterladen ▾“ mit einem Menü aus Download-Links (RFC-0019). Die Links laden direkt vom Server.
import { useEffect, useRef, useState } from "react";
import { ChevronDown, Download } from "lucide-react";

export interface DownloadItem {
  href: string;
  label: string;
  hint?: string;
}

export function DownloadMenu({ label, items, className }: { label: string; items: DownloadItem[]; className: string }) {
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

  return (
    <div ref={box} className="relative">
      <button type="button" aria-haspopup="menu" aria-expanded={open} className={className} onClick={() => setOpen((o) => !o)}>
        <Download size={14} />
        {label}
        <ChevronDown size={14} />
      </button>
      {open && (
        <div role="menu" aria-label={label} className="absolute right-0 z-30 mt-1 min-w-[200px] rounded-lg border border-line bg-card py-1 shadow-lg">
          {items.map((i) => (
            <a key={i.href} role="menuitem" href={i.href} download className="grid px-3 py-1.5 text-sm hover:bg-card-2" onClick={() => setOpen(false)}>
              <span>{i.label}</span>
              {i.hint && <span className="text-xs text-muted">{i.hint}</span>}
            </a>
          ))}
        </div>
      )}
    </div>
  );
}
