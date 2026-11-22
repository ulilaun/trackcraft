import type { ReactNode } from "react";

/** Karte im Kachel-Design: 16 px Radius, feine Linie, leichter Schatten (RFC-0006). */
export function Card({ title, children, className = "" }: { title?: string; children: ReactNode; className?: string }) {
  return (
    <section className={`rounded-card border border-line bg-card p-4 shadow-sm ${className}`}>
      {title && <h2 className="mb-3 text-xs font-semibold uppercase tracking-wide text-muted">{title}</h2>}
      {children}
    </section>
  );
}

/** Kennzahl als eigene Karte: kleines Label oben, großer Wert mit Einheit. */
export function KpiCard({ label, value, unit, sub }: { label: string; value: ReactNode; unit?: string; sub?: ReactNode }) {
  return (
    <section className="rounded-card border border-line bg-card p-4 shadow-sm">
      <div className="text-xs font-medium text-muted">{label}</div>
      <div className="num mt-1 text-2xl font-semibold">
        {value}
        {unit && <span className="ml-1 text-sm font-normal text-muted">{unit}</span>}
      </div>
      {sub && <div className="mt-1 text-xs text-muted">{sub}</div>}
    </section>
  );
}
