// Zustand einer Saison als Punkt: gefüllt mit Hof = läuft, Ring = beendet oder künftig.
import type { Season } from "../api/client";
import { t, type TextKey } from "../i18n";

/** Zustandstext zur Saison, z. B. „beendet“. */
export function seasonStateText(state: Season["state"]): string {
  return t(`season.state.${state}` as TextKey);
}

/**
 * `labelled`: Punkt trägt den Zustand als zugänglichen Namen (frei stehend).
 * Ohne `labelled` ist er nur Schmuck; das umgebende Element nennt den Zustand dann selbst.
 * `onAccent`: auf gefülltem Akzentgrund (aktiver Knopf), Ring dort hell.
 */
export function SeasonDot({ state, labelled = false, onAccent = false }: { state: Season["state"]; labelled?: boolean; onAccent?: boolean }) {
  const live = state === "current";
  const look = live ? "bg-good ring-4 ring-good/20" : `border-2 ${onAccent ? "border-white/80" : "border-muted"}`;
  const text = seasonStateText(state);
  return labelled ? (
    <span role="img" className={`inline-block h-2.5 w-2.5 shrink-0 rounded-full ${look}`} title={text} aria-label={text} data-state={state} />
  ) : (
    <span aria-hidden="true" className={`inline-block h-2.5 w-2.5 shrink-0 rounded-full ${look}`} data-state={state} />
  );
}
