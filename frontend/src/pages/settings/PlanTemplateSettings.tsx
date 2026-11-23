import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Pencil, Plus } from "lucide-react";
import { apiGet, type FormResponse, type HrZonesResponse, type PlanTemplate } from "../../api/client";
import { Card } from "../../components/Card";
import { primaryButton } from "../../components/Dialog";
import { Problem } from "../../components/Problem";
import { SportIcon } from "../../components/SportIcon";
import { useConfig } from "../../hooks/useConfig";
import { t } from "../../i18n";
import { localDate, num } from "../../utils/format";
import { hm } from "../plan/planModel";
import { UnitDialog, useTemplates } from "../plan/UnitDialog";

/**
 * Einstellungen → Plan-Vorlagen (#38): Einheiten-Vorlagen anlegen, bearbeiten (auch umbenennen) und löschen.
 * Im Einheiten-Dialog der Seite „Plan“ wird nur ausgewählt.
 */
export function PlanTemplateSettings() {
  const config = useConfig().data?.data;
  const today = localDate(new Date(), config?.timezone);
  const templates = useTemplates();
  const zones = useQuery({ queryKey: ["hr-zones"], queryFn: () => apiGet<HrZonesResponse>("/hr-zones") });
  const form = useQuery({ queryKey: ["form", "settings"], queryFn: () => apiGet<FormResponse>("/form?forecast_days=0") });
  const [open, setOpen] = useState<{ template: PlanTemplate | null } | null>(null);
  const list = templates.data?.data ?? [];

  return (
    <Card>
      <div className="grid gap-3 text-sm">
        <div className="flex flex-wrap items-start justify-between gap-2">
          <p className="min-w-0 flex-1 text-muted">{t("plan.template.intro")}</p>
          <button type="button" className={`${primaryButton} inline-flex items-center gap-1`} onClick={() => setOpen({ template: null })}>
            <Plus size={15} aria-hidden />
            {t("plan.template.add")}
          </button>
        </div>
        {templates.error && <Problem error={templates.error} />}
        {templates.data && list.length === 0 && <p className="text-muted">{t("plan.template.empty")}</p>}
        {list.length > 0 && (
          <ul className="grid gap-1" data-testid="plan-templates">
            {list.map((x) => (
              <li key={x.id}>
                <button
                  type="button"
                  className="flex w-full items-center gap-2 rounded-lg border border-line bg-card-2 px-3 py-2 text-left hover:border-accent"
                  onClick={() => setOpen({ template: x })}
                  aria-label={t("plan.template.editNamed", { name: x.name })}
                >
                  <SportIcon sport={x.sport_class} size={15} decorative />
                  <span className="grid min-w-0 flex-1">
                    <b className="truncate">{x.name}</b>
                    {x.title !== x.name && <span className="truncate text-xs text-muted">{x.title}</span>}
                  </span>
                  <span className="num text-xs text-muted">{hm(x.load.minutes)}</span>
                  <span className="num w-10 text-right text-xs text-muted">{num(x.load.total)}</span>
                  <Pencil size={14} className="text-muted" aria-hidden />
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>
      {open && (
        <UnitDialog
          unit={null}
          date={today}
          templateOnly
          template={open.template}
          zoneSets={zones.data?.data ?? []}
          settings={form.data?.data.settings}
          catalog={config?.sub_sport_catalog}
          onClose={() => setOpen(null)}
        />
      )}
    </Card>
  );
}
