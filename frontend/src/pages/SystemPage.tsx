import type { ReactElement } from "react";
import { CircleAlert, CircleCheck, TriangleAlert } from "lucide-react";
import type { CheckStatus } from "../api/client";
import { Card, KpiCard } from "../components/Card";
import { Problem } from "../components/Problem";
import { useHealth, useVersion } from "../hooks/useSystem";
import { brand } from "../brand";
import { t, type TextKey } from "../i18n";

const icon: Record<CheckStatus, ReactElement> = {
  ok: <CircleCheck size={18} className="text-good" />,
  warn: <TriangleAlert size={18} className="text-warn" />,
  fail: <CircleAlert size={18} className="text-danger" />,
};

/** Systemseite, Stand M0: Versionen und Gesundheit (vollständig in M5, RFC-0006 Abschnitt 2). */
export function SystemPage() {
  const version = useVersion();
  const health = useHealth();
  const v = version.data?.data;

  return (
    <div className="grid grid-cols-1 gap-4">
      {version.error ? (
        <Card title={t("system.version")}>
          <Problem error={version.error} />
        </Card>
      ) : (
        <div className="grid grid-cols-2 gap-4 lg:grid-cols-4">
          <KpiCard label={t("system.app")} value={v?.app_version ?? "–"} />
          <KpiCard label={t("system.schema")} value={v?.schema_version ?? "–"} />
          <KpiCard label={t("system.algorithm")} value={v?.algorithm_version ?? "–"} />
          <KpiCard label={t("system.api")} value={v?.api_version ?? "–"} />
        </div>
      )}
      <Card title={t("system.health")}>
        {health.error ? (
          <Problem error={health.error} />
        ) : !health.data ? (
          <p className="text-muted">{t("state.loading")}</p>
        ) : (
          <ul className="divide-y divide-line">
            {health.data.data.checks.map((c) => (
              <li key={c.name} className="flex items-center gap-3 py-2">
                {icon[c.status]}
                <span className="font-medium">{t(`health.${c.name}` as TextKey)}</span>
                <span className="ml-auto text-sm text-muted">{c.message ?? t(`health.${c.status}` as TextKey)}</span>
              </li>
            ))}
          </ul>
        )}
      </Card>
      <footer className="text-center text-xs text-muted">
        {t("system.footer", { name: brand.name, version: v?.app_version ?? "–", license: brand.license })}
        {" · "}
        <a className="text-accent hover:underline" href={brand.sourceUrl}>
          {t("system.source")}
        </a>
      </footer>
    </div>
  );
}
