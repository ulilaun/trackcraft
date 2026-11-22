// Geteilter Filterknopf: links der Name (wählt die Gruppe), nach einem Trenner der gewählte
// Unterpunkt mit ▾; der Pfeil öffnet ein Menü, die Wahl schließt es und steht danach im Knopf.
import { useEffect, useRef, useState, type ReactNode } from "react";
import { Check, ChevronDown } from "lucide-react";

export interface SplitOption {
  /** `null` = Eintrag „alle“ (z. B. alle Untertypen). */
  key: string | null;
  label: string;
  icon?: ReactNode;
}

const base = "py-1 text-sm";
const onLook = "bg-accent-strong text-white";
const offLook = "bg-card-2 text-muted hover:text-text";

export function SplitPill({
  label,
  icon,
  on,
  onPick,
  value,
  options,
  selected,
  onSelect,
  menuLabel,
  title,
}: {
  label: string;
  icon?: ReactNode;
  on: boolean;
  onPick: () => void;
  /** Gewählter Unterpunkt rechts vom Trenner; ohne Wert nur der Pfeil. */
  value?: ReactNode;
  options: SplitOption[];
  /** Haken im Menü; `undefined` = keiner. */
  selected?: string | null;
  onSelect: (key: string | null) => void;
  /** Zugänglicher Name des Pfeils, z. B. „Jahr wählen“. */
  menuLabel: string;
  title?: string;
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

  // Ohne Unterpunkte ein einfacher Knopf
  if (options.length === 0 && value == null) {
    return (
      <button
        type="button"
        aria-pressed={on}
        onClick={onPick}
        title={title}
        className={`inline-flex items-center gap-1.5 rounded-full border px-3 ${base} ${on ? `border-transparent ${onLook}` : `border-line ${offLook}`}`}
      >
        {icon}
        {label}
      </button>
    );
  }

  const pick = (key: string | null) => {
    setOpen(false);
    onSelect(key);
  };
  const item = "flex w-full items-center gap-2 px-3 py-1.5 text-left text-sm whitespace-nowrap hover:bg-card-2";
  const look = on ? onLook : offLook;
  const divider = on ? "border-white/30" : "border-line";
  const menu = options.length > 1 || (options.length === 1 && value == null);

  return (
    <div ref={box} className="relative">
      <div className={`flex overflow-hidden rounded-full border ${on ? "border-transparent" : "border-line"}`}>
        <button type="button" aria-pressed={on} onClick={onPick} title={title} className={`inline-flex items-center gap-1.5 pr-2.5 pl-3 ${base} ${look}`}>
          {icon}
          {label}
        </button>
        {menu ? (
          <button
            type="button"
            aria-haspopup="menu"
            aria-expanded={open}
            aria-label={menuLabel}
            title={menuLabel}
            onClick={() => setOpen((o) => !o)}
            className={`inline-flex items-center gap-1.5 border-l pr-2 ${value != null ? "pl-2.5" : "pl-1.5"} ${base} ${divider} ${look}`}
          >
            {value}
            <ChevronDown size={14} />
          </button>
        ) : (
          <span className={`inline-flex items-center gap-1.5 border-l pr-3 pl-2.5 ${base} ${divider} ${look}`}>{value}</span>
        )}
      </div>
      {open && (
        <div role="menu" aria-label={label} className="absolute left-0 z-30 mt-1 max-h-80 min-w-[180px] overflow-y-auto rounded-lg border border-line bg-card py-1 shadow-lg">
          {options.map((o) => (
            <button key={o.key ?? ""} type="button" role="menuitemradio" aria-checked={selected === o.key} className={item} onClick={() => pick(o.key)}>
              <Check size={14} className={selected === o.key ? "" : "invisible"} />
              {o.icon}
              {o.label}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
