import { useEffect, useSyncExternalStore } from "react";
import { getLocale, setLocale, subscribeLocale } from "../i18n";

/** Sprache und Format (Wahl und Ergebnis) mit Setzer; Einstellungen → Darstellung ändert sie. */
export function useLocale() {
  const locale = useSyncExternalStore(subscribeLocale, getLocale);
  return { ...locale, setLocale };
}

/** Setzt `<html lang>` passend zur Sprache. Einmal in der App. */
export function useApplyLocale() {
  const { language } = useLocale();
  useEffect(() => {
    document.documentElement.lang = language;
  }, [language]);
}
