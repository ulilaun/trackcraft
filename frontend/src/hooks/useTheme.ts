import { useEffect, useSyncExternalStore } from "react";
import { readChoice, resolveDark, storeChoice, type ThemeChoice } from "../utils/theme";

const lightQuery = "(prefers-color-scheme: light)";

// Eine gemeinsame Wahl für die ganze Seite: Einstellungen → Darstellung ändert sie, die App wendet sie an.
let current: ThemeChoice | null = null;
const listeners = new Set<() => void>();

function get(): ThemeChoice {
  current ??= readChoice(window.localStorage);
  return current;
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/** Setzt die Wahl, speichert sie im Browser und benachrichtigt alle Nutzer. */
export function setThemeChoice(choice: ThemeChoice) {
  storeChoice(window.localStorage, choice);
  current = choice;
  listeners.forEach((l) => l());
}

/** Nur für Tests: gespeicherte Wahl neu einlesen. */
export function resetThemeForTests() {
  current = null;
}

/** Aktuelle Wahl und Setzer. */
export function useTheme() {
  const choice = useSyncExternalStore(subscribe, get);
  return { choice, setChoice: setThemeChoice };
}

/** Wendet die Wahl auf `<html class="dark">` an und folgt bei „Wie System“ dem Betriebssystem. Einmal in der App. */
export function useApplyTheme() {
  const { choice } = useTheme();
  useEffect(() => {
    const mq = window.matchMedia(lightQuery);
    const apply = () => document.documentElement.classList.toggle("dark", resolveDark(choice, mq.matches));
    apply();
    mq.addEventListener("change", apply);
    return () => mq.removeEventListener("change", apply);
  }, [choice]);
}
