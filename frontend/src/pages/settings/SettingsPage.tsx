import type { ReactNode } from "react";
import { Link, Navigate, useParams } from "react-router-dom";
import { Activity, ArrowLeft, CalendarRange, ChevronRight, Download, MapPin, Palette, RefreshCw, type LucideIcon } from "lucide-react";
import { t, type TextKey } from "../../i18n";
import { AppearanceSettings } from "./AppearanceSettings";
import { ExportSettings } from "./ExportSettings";
import { GarminSettings } from "./GarminSettings";
import { PlaceSettings } from "./PlaceSettings";
import { PlanTemplateSettings } from "./PlanTemplateSettings";
import { SportsSettings } from "./SportsSettings";

type Section = { slug: string; title: TextKey; hint: TextKey; icon: LucideIcon; render: () => ReactNode };

/** Abschnitte in der Reihenfolge der Liste; der erste ist die Vorgabe auf großen Bildschirmen. */
const SECTIONS: Section[] = [
  { slug: "synchronisierung", title: "settings.sync", hint: "settings.syncHint", icon: RefreshCw, render: () => <GarminSettings /> },
  { slug: "sportarten", title: "settings.sports", hint: "settings.sportsHint", icon: Activity, render: () => <SportsSettings /> },
  { slug: "plan-vorlagen", title: "settings.planTemplates", hint: "settings.planTemplatesHint", icon: CalendarRange, render: () => <PlanTemplateSettings /> },
  { slug: "orte", title: "settings.places", hint: "settings.placesHint", icon: MapPin, render: () => <PlaceSettings /> },
  { slug: "export", title: "settings.export", hint: "settings.exportHint", icon: Download, render: () => <ExportSettings /> },
  { slug: "darstellung", title: "settings.appearance", hint: "settings.appearanceHint", icon: Palette, render: () => <AppearanceSettings /> },
];

/**
 * Einstellungen hinter dem Zahnrad der Kopfzeile als Master-Detail (Uli 16.11.2026):
 * links die Abschnitte (ca. 28 %), rechts der gewählte; Abschnitt in der URL `/einstellungen/:section`.
 * Unter `lg` zuerst nur die Liste, ein Abschnitt dann allein mit Zurück-Link.
 */
export function SettingsPage() {
  const { section } = useParams();
  const current = section ? SECTIONS.find((s) => s.slug === section) : undefined;
  if (section && !current) return <Navigate to="/einstellungen" replace />;
  const shown = current ?? SECTIONS[0]!;

  return (
    <div className="grid grid-cols-1 min-w-0 gap-4">
      <h1 className="text-2xl font-bold">{t("settings.title")}</h1>
      <div className="grid min-w-0 gap-4 lg:grid-cols-[minmax(220px,28%)_minmax(0,1fr)] lg:items-start">
        <nav
          aria-label={t("settings.sections")}
          className={`${current ? "hidden lg:block" : ""} rounded-card border border-line bg-card p-2 shadow-sm lg:sticky lg:top-20`}
        >
          <ul className="grid gap-1">
            {SECTIONS.map((s) => {
              const active = s === shown;
              const Icon = s.icon;
              return (
                <li key={s.slug}>
                  <Link
                    to={`/einstellungen/${s.slug}`}
                    aria-current={active ? "page" : undefined}
                    className={`flex items-center gap-3 rounded-lg px-3 py-2.5 hover:bg-card-2 ${active ? "lg:bg-card-2 lg:text-accent" : ""}`}
                  >
                    <Icon size={18} className="shrink-0" aria-hidden="true" />
                    <span className="grid min-w-0">
                      <span className="font-medium">{t(s.title)}</span>
                      <span className="truncate text-xs text-muted">{t(s.hint)}</span>
                    </span>
                    <ChevronRight size={16} className="ml-auto shrink-0 text-muted lg:hidden" aria-hidden="true" />
                  </Link>
                </li>
              );
            })}
          </ul>
        </nav>
        <section aria-labelledby="settings-section" className={`${current ? "grid" : "hidden lg:grid"} min-w-0 gap-3`}>
          {current && (
            <Link to="/einstellungen" className="flex items-center gap-1.5 text-sm text-muted hover:text-text lg:hidden">
              <ArrowLeft size={16} />
              {t("settings.back")}
            </Link>
          )}
          <h2 id="settings-section" className="text-lg font-semibold">
            {t(shown.title)}
          </h2>
          {shown.render()}
        </section>
      </div>
    </div>
  );
}
