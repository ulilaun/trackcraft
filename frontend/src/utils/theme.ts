// Theme nach RFC-0006: Wahl je Browser, sonst System, ohne Präferenz Dark.
export type ThemeChoice = "system" | "light" | "dark";
export const THEME_KEY = "trackcraft.theme";
export function resolveDark(choice: ThemeChoice, systemPrefersLight: boolean): boolean {
  if (choice === "dark") return true;
  if (choice === "light") return false;
  return !systemPrefersLight;
}

export function readChoice(storage: Pick<Storage, "getItem"> | undefined): ThemeChoice {
  try {
    const v = storage?.getItem(THEME_KEY);
    return v === "light" || v === "dark" ? v : "system";
  } catch {
    return "system";
  }
}

export function storeChoice(storage: Pick<Storage, "setItem" | "removeItem"> | undefined, choice: ThemeChoice): void {
  try {
    if (choice === "system") storage?.removeItem(THEME_KEY);
    else storage?.setItem(THEME_KEY, choice);
  } catch {
    /* ohne Speicher gilt die Wahl nur bis zum Neuladen */
  }
}
