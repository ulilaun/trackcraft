import { Download } from "lucide-react";
import { API_BASE } from "../../api/client";
import { Card } from "../../components/Card";
import { t } from "../../i18n";

/** Einstellungen → Export (RFC-0019, #58): Gesamtexport als ZIP, vom Server beim Herunterladen erzeugt. */
export function ExportSettings() {
  return (
    <Card>
      <div className="grid gap-3 text-sm">
        <p className="text-muted">{t("export.intro")}</p>
        <ul className="grid list-disc gap-1 pl-5 text-muted">
          <li>{t("export.originals")}</li>
          <li>{t("export.gpx")}</li>
          <li>{t("export.json")}</li>
        </ul>
        <p className="text-xs text-muted">{t("export.noSecrets")}</p>
        <div>
          <a
            href={`${API_BASE}/export`}
            download
            className="inline-flex items-center gap-1.5 rounded-lg bg-accent-strong px-3 py-1.5 text-sm font-medium text-white hover:opacity-90"
          >
            <Download size={14} />
            {t("export.start")}
          </a>
        </div>
        <p className="text-xs text-muted">{t("export.hint")}</p>
      </div>
    </Card>
  );
}
