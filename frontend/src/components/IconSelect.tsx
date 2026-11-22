// Auswahlliste mit Symbolen: Browser-Auswahlfelder können keine Symbole zeigen.
// Bedienung wie ein Auswahlfeld: Klick oder ↓/Enter öffnet, Pfeiltasten/Pos1/Ende wählen vor, Enter/Leertaste übernimmt,
// Esc oder Klick daneben schließt.
import { useEffect, useId, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import { ChevronDown } from "lucide-react";

export interface IconOption<V extends string> {
  value: V;
  label: string;
  icon?: ReactNode;
}

export function IconSelect<V extends string>({
  value,
  options,
  onChange,
  label,
  disabled = false,
  className = "",
}: {
  value: V;
  options: IconOption<V>[];
  onChange: (v: V) => void;
  /** zugänglicher Name des Feldes */
  label: string;
  disabled?: boolean;
  className?: string;
}) {
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(0);
  const box = useRef<HTMLDivElement>(null);
  const list = useRef<HTMLUListElement>(null);
  const id = useId();
  const current = options.find((o) => o.value === value) ?? options[0];

  useEffect(() => {
    if (!open) return;
    const away = (e: PointerEvent) => {
      if (!box.current?.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("pointerdown", away);
    return () => document.removeEventListener("pointerdown", away);
  }, [open]);

  useEffect(() => {
    if (open) list.current?.querySelector<HTMLElement>(`[data-index="${active}"]`)?.scrollIntoView?.({ block: "nearest" });
  }, [open, active]);

  const show = () => {
    if (disabled) return;
    setActive(Math.max(0, options.findIndex((o) => o.value === value)));
    setOpen(true);
  };
  const pick = (v: V) => {
    setOpen(false);
    if (v !== value) onChange(v);
  };
  const key = (e: KeyboardEvent) => {
    if (!open) {
      if (["ArrowDown", "ArrowUp", "Enter", " "].includes(e.key)) {
        e.preventDefault();
        show();
      }
      return;
    }
    const last = options.length - 1;
    const moves: Record<string, number> = { ArrowDown: Math.min(last, active + 1), ArrowUp: Math.max(0, active - 1), Home: 0, End: last };
    if (e.key in moves) {
      e.preventDefault();
      setActive(moves[e.key]!);
    } else if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      const o = options[active];
      if (o) pick(o.value);
    } else if (e.key === "Escape" || e.key === "Tab") {
      if (e.key === "Escape") e.preventDefault();
      setOpen(false);
    }
  };

  return (
    <div ref={box} className={`relative ${className}`}>
      <button
        type="button"
        role="combobox"
        aria-label={label}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-controls={open ? `${id}-list` : undefined}
        aria-activedescendant={open ? `${id}-${active}` : undefined}
        disabled={disabled}
        onClick={() => (open ? setOpen(false) : show())}
        onKeyDown={key}
        className="flex w-full items-center gap-2 rounded-lg border border-line bg-card-2 px-2.5 py-1.5 text-left text-sm disabled:opacity-60"
      >
        {current?.icon}
        <span className="min-w-0 flex-1 truncate">{current?.label}</span>
        <ChevronDown size={14} className="shrink-0 text-muted" aria-hidden />
      </button>
      {open && (
        <ul
          ref={list}
          id={`${id}-list`}
          role="listbox"
          aria-label={label}
          className="absolute left-0 z-40 mt-1 max-h-72 min-w-full overflow-auto rounded-lg border border-line bg-card py-1 shadow-lg"
        >
          {options.map((o, i) => (
            <li
              key={o.value}
              id={`${id}-${i}`}
              data-index={i}
              role="option"
              aria-selected={o.value === value}
              onPointerEnter={() => setActive(i)}
              onClick={() => pick(o.value)}
              className={`flex cursor-pointer items-center gap-2 px-2.5 py-1.5 text-sm whitespace-nowrap ${i === active ? "bg-card-2" : ""} ${
                o.value === value ? "font-semibold" : ""
              }`}
            >
              {o.icon ?? <span className="inline-block w-4" />}
              {o.label}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
