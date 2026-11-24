import type { ReactNode } from "react";
import { Moon, Sun } from "lucide-react";
import { AutoThemeIcon } from "../../components/AutoThemeIcon";
import { Card } from "../../components/Card";
import { useLocale } from "../../hooks/useLocale";
import { useTheme } from "../../hooks/useTheme";
import {
  COMPLETE_LANGUAGES,
  FORMATS,
  LANGUAGES,
  resolveFormat,
  resolveLanguage,
  t,
  type FormatChoice,
  type LanguageChoice,
  type TextKey,
} from "../../i18n";
import type { ThemeChoice } from "../../utils/theme";

const OPTIONS: { value: ThemeChoice; label: TextKey; Icon: (p: { size?: number }) => ReactNode }[] = [
  { value: "light", label: "theme.light", Icon: Sun },
  { value: "dark", label: "theme.dark", Icon: Moon },
  { value: "system", label: "theme.system", Icon: AutoThemeIcon },
];

const select = "w-full rounded-lg border border-line bg-card-2 px-2 py-1.5 text-sm text-text";

/** Beispiel eines Formats: Datum und Zahl, z. B. „24.11.2026 · 1.234,5“. */
function example(locale: string): string {
  const date = new Intl.DateTimeFormat(locale, { day: "2-digit", month: "2-digit", year: "numeric", timeZone: "UTC" }).format(
    new Date(Date.UTC(2026, 10, 24, 12)),
  );
  const number = new Intl.NumberFormat(locale, { minimumFractionDigits: 1, maximumFractionDigits: 1 }).format(1234.5);
  return `${date} · ${number}`;
}

/** Einstellungen → Darstellung: Design (Hell, Dunkel, Wie System), Sprache und Format von Datum und Zahlen, je Browser. */
export function AppearanceSettings() {
  const { choice, setChoice } = useTheme();
  const { languageChoice, formatChoice, language, setLocale } = useLocale();
  const autoLanguage = resolveLanguage("auto");
  return (
    <div className="grid grid-cols-1 gap-4">
      <Card title={t("settings.theme")}>
        <p className="mb-3 text-sm text-muted">{t("settings.themeIntro")}</p>
        <div role="radiogroup" aria-label={t("settings.theme")} className="grid grid-cols-3 gap-3">
          {OPTIONS.map(({ value, label, Icon }) => {
            const on = choice === value;
            return (
              <button
                key={value}
                type="button"
                role="radio"
                aria-checked={on}
                onClick={() => setChoice(value)}
                className={`flex flex-col items-center gap-2 rounded-card border-2 px-2 py-4 text-sm font-medium transition-colors ${
                  on ? "border-accent bg-card-2 text-accent" : "border-line text-muted hover:border-accent/60 hover:text-text"
                }`}
              >
                <Icon size={28} />
                {t(label)}
              </button>
            );
          })}
        </div>
      </Card>

      <Card title={t("settings.language")}>
        <p className="mb-3 text-sm text-muted">{t("settings.languageIntro")}</p>
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
          <div className="grid content-start gap-1">
            <label className="grid gap-1">
              <span className="text-xs font-medium text-muted">{t("settings.language")}</span>
              <select className={select} value={languageChoice} onChange={(e) => setLocale({ language: e.target.value as LanguageChoice })}>
                <option value="auto">{t("settings.language.auto", { language: t(`language.${autoLanguage}`) })}</option>
                {LANGUAGES.map((l) => (
                  <option key={l} value={l}>
                    {t(`language.${l}`)}
                  </option>
                ))}
              </select>
            </label>
            {!COMPLETE_LANGUAGES.includes(language) && <span className="text-xs text-warn">{t("settings.languageIncomplete")}</span>}
          </div>
          <label className="grid gap-1">
            <span className="text-xs font-medium text-muted">{t("settings.format")}</span>
            <select className={select} value={formatChoice} onChange={(e) => setLocale({ format: e.target.value as FormatChoice })}>
              <option value="auto">{t("settings.format.auto", { example: example(resolveFormat("auto", language)) })}</option>
              {FORMATS.map((f) => (
                <option key={f} value={f}>
                  {example(f)}
                </option>
              ))}
            </select>
          </label>
        </div>
      </Card>
    </div>
  );
}
