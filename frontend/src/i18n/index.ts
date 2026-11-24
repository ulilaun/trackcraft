// Alle Texte zentral über Schlüssel (RFC-0006 „Sprache und Formate“). Deutsch ist die Referenz, weitere Sprachen
// fallen je Schlüssel auf Deutsch zurück. Sprache und Format wählt man je Browser (wie das Design).
import de from "./de.json";
import en from "./en.json";

export type TextKey = keyof typeof de;

/** Basisschlüssel mit Mehrzahlformen `…_one` / `…_other` für `tn()`. */
export type PluralKey = { [K in TextKey]: K extends `${infer B}_other` ? B : never }[TextKey];

export type Language = "de" | "en";
export type LanguageChoice = "auto" | Language;
/** Gebietsschema für Zahlen und Datum; „auto“ folgt der Sprache. */
export type FormatChoice = "auto" | "de-DE" | "en-GB" | "en-US";

export const LANGUAGES: Language[] = ["de", "en"];
export const FORMATS: Exclude<FormatChoice, "auto">[] = ["de-DE", "en-GB", "en-US"];
/** Sprachen, die „Automatisch“ wählen darf. Englisch kommt dazu, sobald die Übersetzung vollständig ist (#59). */
export const COMPLETE_LANGUAGES: Language[] = ["de"];

export const LANGUAGE_KEY = "trackcraft.language";
export const FORMAT_KEY = "trackcraft.format";

const dictionaries: Record<Language, Partial<Record<string, string>>> = { de, en };

interface LocaleState {
  languageChoice: LanguageChoice;
  formatChoice: FormatChoice;
  language: Language;
  format: Exclude<FormatChoice, "auto">;
}

function storage(): Storage | undefined {
  try {
    return typeof window === "undefined" ? undefined : window.localStorage;
  } catch {
    return undefined;
  }
}

function read(key: string): string | null {
  try {
    return storage()?.getItem(key) ?? null;
  } catch {
    return null;
  }
}

function write(key: string, value: string | null) {
  try {
    if (value == null) storage()?.removeItem(key);
    else storage()?.setItem(key, value);
  } catch {
    /* ohne Speicher gilt die Wahl nur bis zum Neuladen */
  }
}

function browserLanguages(): readonly string[] {
  if (typeof navigator === "undefined") return [];
  return navigator.languages?.length ? navigator.languages : [navigator.language];
}

/** Sprache aus der Wahl; „auto“ nimmt die erste vollständige Sprache des Browsers, sonst Deutsch. */
export function resolveLanguage(choice: LanguageChoice, browser: readonly string[] = browserLanguages()): Language {
  if (choice !== "auto") return choice;
  for (const tag of browser) {
    const base = tag.toLowerCase().split("-")[0] as Language;
    if (COMPLETE_LANGUAGES.includes(base)) return base;
  }
  return "de";
}

/** Gebietsschema aus der Wahl; „auto“ folgt der Sprache (Englisch: britisch oder amerikanisch nach Browser). */
export function resolveFormat(choice: FormatChoice, language: Language, browser: readonly string[] = browserLanguages()): Exclude<FormatChoice, "auto"> {
  if (choice !== "auto") return choice;
  if (language === "de") return "de-DE";
  return browser.some((b) => b.toLowerCase() === "en-us") ? "en-US" : "en-GB";
}

function load(): LocaleState {
  const l = read(LANGUAGE_KEY);
  const f = read(FORMAT_KEY);
  const languageChoice: LanguageChoice = l === "de" || l === "en" ? l : "auto";
  const formatChoice: FormatChoice = (FORMATS as string[]).includes(f ?? "") ? (f as FormatChoice) : "auto";
  const language = resolveLanguage(languageChoice);
  return { languageChoice, formatChoice, language, format: resolveFormat(formatChoice, language) };
}

let state: LocaleState | null = null;
const listeners = new Set<() => void>();

function current(): LocaleState {
  state ??= load();
  return state;
}

/** Aktive Sprache und Formatwahl (für `useSyncExternalStore`). */
export function getLocale(): LocaleState {
  return current();
}

export function subscribeLocale(listener: () => void) {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/** Setzt Sprache und/oder Format, speichert die Wahl im Browser und benachrichtigt die Oberfläche. */
export function setLocale(patch: { language?: LanguageChoice; format?: FormatChoice }) {
  const prev = current();
  const languageChoice = patch.language ?? prev.languageChoice;
  const formatChoice = patch.format ?? prev.formatChoice;
  write(LANGUAGE_KEY, languageChoice === "auto" ? null : languageChoice);
  write(FORMAT_KEY, formatChoice === "auto" ? null : formatChoice);
  const language = resolveLanguage(languageChoice);
  state = { languageChoice, formatChoice, language, format: resolveFormat(formatChoice, language) };
  if (typeof document !== "undefined") document.documentElement.lang = language;
  listeners.forEach((l) => l());
}

/** Nur für Tests: gespeicherte Wahl neu einlesen. */
export function resetLocaleForTests() {
  state = null;
}

/** Aktive Sprache der Texte. */
export function language(): Language {
  return current().language;
}

/** Gebietsschema für Zahlen und Datum (`Intl`). */
export function formatLocale(): string {
  return current().format;
}

function lookup(key: string): string | undefined {
  return dictionaries[current().language][key] ?? (de as Record<string, string>)[key];
}

function fill(text: string, params: Record<string, string | number>): string {
  return text.replace(/\{(\w+)\}/g, (_, name: string) => (name in params ? String(params[name]) : `{${name}}`));
}

export function t(key: TextKey, params: Record<string, string | number> = {}): string {
  return fill(lookup(key) ?? key, params);
}

/** Mehrzahl nach den Regeln der Sprache: `…_one`, `…_other` usw.; `{count}` steht als Platzhalter bereit. */
export function tn(key: PluralKey, count: number, params: Record<string, string | number> = {}): string {
  const form = new Intl.PluralRules(current().language).select(count);
  const text = lookup(`${key}_${form}`) ?? lookup(`${key}_other`) ?? key;
  return fill(text, { count, ...params });
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
