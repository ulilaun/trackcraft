import { useConfig } from "../hooks/useConfig";
import { t } from "../i18n";

/** Hinweisband über der Kopfzeile, wenn die Konfiguration `[demo] enabled = true` hat (Issue #41). */
export function DemoBanner() {
  const demo = useConfig().data?.data.demo;
  if (!demo) return null;
  return (
    <div role="note" className="bg-warn/15 border-b border-warn/40 px-4 py-1.5 text-center text-sm">
      {t("demo.banner")}
    </div>
  );
}
