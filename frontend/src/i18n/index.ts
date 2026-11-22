// Alle Texte zentral über Schlüssel (RFC-0006, D4). V1 nur Deutsch.
import de from "./de.json";

export type TextKey = keyof typeof de;

export function t(key: TextKey, params: Record<string, string | number> = {}): string {
  const text: string = de[key] ?? key;
  return text.replace(/\{(\w+)\}/g, (_, name: string) => (name in params ? String(params[name]) : `{${name}}`));
}

/** `true`, wenn es zum Schlüssel einen Text gibt (z. B. für Untertyp-Schlüssel aus der Datenbank). */
export function hasText(key: string): key is TextKey {
  return key in de;
}

/** Text zu einem API-Fehlercode; unbekannte Codes zeigen Meldung und Code (RFC-0006). */
export function errorText(code: string, message: string): string {
  const key = `error.${code}`;
  if (key in de) return t(key as TextKey);
  return t("error.unknown", { message, code });
}
