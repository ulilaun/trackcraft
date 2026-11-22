import type { ReactNode } from "react";
import { Moon, Sun } from "lucide-react";
import { AutoThemeIcon } from "../../components/AutoThemeIcon";
import { Card } from "../../components/Card";
import { useTheme } from "../../hooks/useTheme";
import { t, type TextKey } from "../../i18n";
import type { ThemeChoice } from "../../utils/theme";

const OPTIONS: { value: ThemeChoice; label: TextKey; Icon: (p: { size?: number }) => ReactNode }[] = [
  { value: "light", label: "theme.light", Icon: Sun },
  { value: "dark", label: "theme.dark", Icon: Moon },
  { value: "system", label: "theme.system", Icon: AutoThemeIcon },
];

/** Einstellungen → Darstellung: Hell, Dunkel oder Wie System als drei Karten; ersetzt den Knopf der Kopfzeile. */
export function AppearanceSettings() {
  const { choice, setChoice } = useTheme();
  return (
    <Card>
      <p className="mb-3 text-sm text-muted">{t("settings.themeIntro")}</p>
      <div role="radiogroup" aria-label={t("settings.appearance")} className="grid grid-cols-3 gap-3">
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
  );
}
