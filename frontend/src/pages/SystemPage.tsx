import type { ReactElement, ReactNode } from "react";
import { CircleAlert, CircleCheck, TriangleAlert } from "lucide-react";
import type { CheckStatus, SystemInfo } from "../api/client";
import { Card, KpiCard } from "../components/Card";
import { Problem } from "../components/Problem";
import { useConfig } from "../hooks/useConfig";
import { useHealth, useSystemInfo, useVersion } from "../hooks/useSystem";
import { brand } from "../brand";
import { t, type TextKey } from "../i18n";
import { dateTime, fileSize, num } from "../utils/format";

const icon: Record<CheckStatus, ReactElement> = {
  ok: <CircleCheck size={18} className="text-good" />,
  warn: <TriangleAlert size={18} className="text-warn" />,
  fail: <CircleAlert size={18} className="text-danger" />,
};

/** Bekannte Warnungscodes aus `GET /system`; unbekannte zeigen nur die Meldung des Backends. */
const WARNINGS = ["low_disk_space", "backup_pending", "backup_failed", "job_failed", "certificate_expiring"];

/** Laufzeit grob: ab einem Tag Tage und Stunden, sonst Stunden und Minuten. */
export function uptime(seconds: number): string {
  const days = Math.floor(seconds / 86_400);
  const hours = Math.floor((seconds % 86_400) / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  return days > 0 ? t("system.uptimeDays", { days, hours }) : t("system.uptimeHours", { hours, minutes });
}

function Row({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex flex-wrap items-baseline justify-between gap-x-4 py-2">
      <dt className="text-muted">{label}</dt>
      <dd className="text-right">{children}</dd>
    </div>
  );
}

function Warnings({ info, tz }: { info: SystemInfo; tz?: string }) {
  if (info.warnings.length === 0) return null;
  return (
    <Card title={t("system.warnings")}>
      <ul className="divide-y divide-line">
        {info.warnings.map((w) => (
          <li key={`${w.code}-${w.message}`} className="flex items-start gap-3 py-2">
            <TriangleAlert size={18} className="mt-0.5 shrink-0 text-warn" />
            <div className="min-w-0">
              <div className="font-medium">
                {WARNINGS.includes(w.code) ? t(`system.warning.${w.code}` as TextKey) : w.message}
              </div>
              <div className="text-sm text-muted">
                {WARNINGS.includes(w.code) && `${w.message} · `}
                {t("system.since", { time: dateTime(w.since, tz, true) })}
              </div>
            </div>
          </li>
        ))}
      </ul>
    </Card>
  );
}

function Operation({ info, tz }: { info: SystemInfo; tz?: string }) {
  const { storage, jobs, calculation } = info;
  return (
    <Card title={t("system.operation")}>
      <dl className="divide-y divide-line text-sm">
        <Row label={t("system.platform")}>{t(`system.platform.${info.platform}` as TextKey)}</Row>
        <Row label={t("system.started")}>
          {dateTime(info.started_at, tz, true)}
          <span className="ml-2 text-muted">({uptime(info.uptime_s)})</span>
        </Row>
        <Row label={t("system.tzdata")}>{info.tzdata_version}</Row>
        <Row label={t("system.certificate")}>
          {info.tls_certificate_expires_at
            ? t("system.certificateUntil", { time: dateTime(info.tls_certificate_expires_at, tz, true) })
            : t("system.certificateNone")}
        </Row>
        <Row label={t("system.freeData")}>{fileSize(storage.data_free_bytes)}</Row>
        <Row label={t("system.freeTarget")}>
          {storage.backup_target_free_bytes == null
            ? t("system.targetUnavailable")
            : fileSize(storage.backup_target_free_bytes)}
        </Row>
        <Row label={t("system.calculation")}>
          {t(`system.calculation.${calculation.status}` as TextKey)}
          {calculation.published_at && (
            <span className="ml-2 text-muted">{dateTime(calculation.published_at, tz, true)}</span>
          )}
        </Row>
        <Row label={t("system.jobs")}>
          {t("system.jobsValue", { queued: num(jobs.queued), running: num(jobs.running), failed: num(jobs.failed) })}
        </Row>
      </dl>
    </Card>
  );
}

/** Systemseite (RFC-0006 Abschnitt 2, RFC-0013): Warnungen, Betrieb, Bestand, Versionen und Gesundheit. */
export function SystemPage() {
  const version = useVersion();
  const health = useHealth();
  const system = useSystemInfo();
  const tz = useConfig().data?.data.timezone;
  const v = version.data?.data;
  const info = system.data?.data;

  return (
    <div className="grid grid-cols-1 gap-4">
      {info && <Warnings info={info} tz={tz} />}
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
      {system.error ? (
        <Card title={t("system.operation")}>
          <Problem error={system.error} />
        </Card>
      ) : !info ? (
        <Card title={t("system.operation")}>
          <p className="text-muted">{t("state.loading")}</p>
        </Card>
      ) : (
        <>
          <div className="grid grid-cols-1 gap-4 sm:grid-cols-3">
            <KpiCard label={t("system.count.activities")} value={num(info.counts.activities)} />
            <KpiCard label={t("system.count.segments")} value={num(info.counts.segments)} />
            <KpiCard label={t("system.count.efforts")} value={num(info.counts.efforts)} />
          </div>
          <Operation info={info} tz={tz} />
        </>
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
