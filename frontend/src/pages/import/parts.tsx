// Darstellung des Importdialogs (RFC-0006 „Importdialog“); Zustand und Abläufe stehen in ImportPage.
import { useRef, useState, type ReactNode } from "react";
import { CircleAlert, Clock, Copy, Upload } from "lucide-react";
import type { CarryOver } from "../../api/client";
import { Card, KpiCard } from "../../components/Card";
import { SportIcon, SubSportIcon } from "../../components/SportIcon";
import { useConfig, useSubSportLabel } from "../../hooks/useConfig";
import { errorText, t, type TextKey } from "../../i18n";
import { dateTime, duration, km, megabytes } from "../../utils/format";
import { isOpen, type Choice, type DuplicateAction, type FileEntry, type Row, type Summary } from "./model";

export const input = "rounded-lg border border-line bg-card-2 px-2 py-1 text-sm text-text disabled:opacity-60";
export const primaryButton =
  "rounded-lg bg-accent-strong px-4 py-2 text-sm font-medium text-white hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50";
export const secondaryButton = "rounded-lg border border-line px-4 py-2 text-sm font-medium hover:bg-card-2 disabled:opacity-50";

type SetChoice = (key: string, patch: Partial<Choice>) => void;

// ---------------------------------------------------------------------------------------------------------

/** Ablagefläche für FIT/GPX; `compact` für das Dashboard (RFC-0018 „Quick-Upload-Zone“). */
export function DropZone({ maxMb, onFiles, compact = false }: { maxMb: number; onFiles: (files: File[]) => void; compact?: boolean }) {
  const picker = useRef<HTMLInputElement>(null);
  const [over, setOver] = useState(false);
  return (
    <section
      className={`rounded-card border-2 border-dashed text-center transition-colors ${compact ? "p-5" : "p-12"} ${over ? "border-accent bg-card-2" : "border-line"}`}
      onDragOver={(e) => {
        e.preventDefault();
        setOver(true);
      }}
      onDragLeave={() => setOver(false)}
      onDrop={(e) => {
        e.preventDefault();
        setOver(false);
        const files = Array.from(e.dataTransfer.files);
        if (files.length) onFiles(files);
      }}
    >
      <Upload size={compact ? 24 : 40} className="mx-auto text-muted" />
      <h2 className={compact ? "mt-1 font-semibold" : "mt-2 text-xl font-semibold"}>{t("import.drop.title")}</h2>
      <p className="mt-1 text-sm text-muted">{t("import.drop.hint", { max: maxMb })}</p>
      <button type="button" className={`${primaryButton} ${compact ? "mt-3" : "mt-5"}`} onClick={() => picker.current?.click()}>
        {t("import.drop.pick")}
      </button>
      <input
        ref={picker}
        type="file"
        multiple
        accept=".fit,.FIT,.gpx,.GPX"
        className="hidden"
        data-testid="file-input"
        onChange={(e) => {
          const files = Array.from(e.target.files ?? []);
          e.target.value = "";
          if (files.length) onFiles(files);
        }}
      />
    </section>
  );
}

export function UploadList({ entries }: { entries: FileEntry[] }) {
  const done = entries.filter((e) => e.status !== "waiting" && e.status !== "uploading").length;
  return (
    <Card title={`${t("import.upload.title")} · ${done}/${entries.length}`}>
      <p className="mb-2 text-sm text-muted">{t("import.upload.hint")}</p>
      <ul className="divide-y divide-line">
        {entries.map((e) => (
          <li key={e.key} className="flex flex-wrap items-center gap-3 py-2 text-sm">
            <span className="min-w-0 flex-1 truncate">{e.name}</span>
            <span className="num text-muted">{megabytes(e.size)} MB</span>
            <div className="h-1.5 w-44 overflow-hidden rounded-full bg-card-2">
              <div
                className={`h-full ${e.status === "rejected" ? "bg-danger" : "bg-accent"}`}
                style={{ width: `${Math.round(e.progress * 100)}%` }}
              />
            </div>
            <span className={`w-24 text-right ${e.status === "rejected" ? "text-danger" : "text-muted"}`}>
              {t(`import.upload.${e.status}` as TextKey)}
            </span>
          </li>
        ))}
      </ul>
    </Card>
  );
}

export function SummaryCards({ s }: { s: Summary }) {
  return (
    <div className="grid grid-cols-2 gap-4 lg:grid-cols-4">
      <KpiCard label={t("import.kpi.files")} value={s.files} unit={`${megabytes(s.bytes)} MB`} />
      <KpiCard label={t("import.kpi.imported")} value={s.imported} unit={t("import.kpi.importedSub")} />
      <KpiCard
        label={t("import.kpi.decision")}
        value={s.needDecision}
        unit={s.open ? t("import.kpi.open", { n: s.open }) : t("import.kpi.allDecided")}
      />
      <KpiCard label={t("import.kpi.notImportable")} value={s.notImportable} unit={t("import.kpi.notImportableSub")} />
    </div>
  );
}

function Problem({ text }: { text?: string }) {
  return text ? (
    <p className="mt-1 flex items-center gap-1 text-sm text-danger">
      <CircleAlert size={14} /> {text}
    </p>
  ) : null;
}

// ---------------------------------------------------------------------------------------------------------

/** Gespeicherte bzw. mitgespeicherte Sessions, nur Anzeige; Titel und Metadaten ändert die Detailseite. */
export function ActivityTable({ title, hint, rows, tz }: { title: string; hint: string; rows: Row[]; tz?: string }) {
  const subLabel = useSubSportLabel();
  const catalog = useConfig().data?.data.sub_sport_catalog;
  return (
    <Card title={title}>
      <p className="mb-3 text-sm text-muted">{hint}</p>
      <div className="overflow-x-auto">
        <table className="w-full text-sm">
          <thead className="text-xs text-muted">
            <tr className="border-b border-line">
              <th className="p-2 text-left">{t("import.col.title")}</th>
              <th className="p-2 text-left">{t("import.col.file")}</th>
              <th className="p-2 text-right">{t("import.col.start")}</th>
              <th className="p-2 text-left">{t("import.col.subSport")}</th>
              <th className="p-2 text-right">{t("import.col.distance")}</th>
              <th className="p-2 text-right">{t("import.col.time")}</th>
              <th className="p-2 text-right">{t("import.col.hr")}</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((r) => {
              const s = r.session;
              return (
                <tr key={r.key} className="border-b border-line last:border-0">
                  <td className="p-2">
                    <span className="flex items-center gap-2">
                      <SportIcon sport={s.sport_class} />
                      {s.title}
                    </span>
                  </td>
                  <td className="max-w-48 truncate p-2 text-muted" title={r.fileName}>
                    {r.fileName}
                  </td>
                  <td className="num p-2 text-right">{dateTime(s.start_time, tz)}</td>
                  <td className="p-2">
                    {s.sub_sport && (
                      <span className="inline-flex" title={subLabel(s.sport_class, s.sub_sport)}>
                        <SubSportIcon sport={s.sport_class} catalog={catalog} subKey={s.sub_sport} size={18} />
                      </span>
                    )}
                  </td>
                  <td className="num p-2 text-right">{km(s.distance_m)} km</td>
                  <td className="num p-2 text-right">{duration(s.total_time_ms)}</td>
                  <td className="num p-2 text-right">{s.avg_heart_rate_bpm ?? "–"}</td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </Card>
  );
}

const options: { value: DuplicateAction; label: TextKey; text: TextKey }[] = [
  { value: "skip", label: "import.dup.skip", text: "import.dup.skipText" },
  { value: "import", label: "import.dup.import", text: "import.dup.importText" },
  { value: "replace", label: "import.dup.replace", text: "import.dup.replaceText" },
];
const carryKeys: (keyof CarryOver)[] = ["title", "description", "location", "sport_class", "sub_sport"];

export function DuplicateCard({ row, c, set, tz }: { row: Row; c: Choice; set: SetChoice; tz?: string }) {
  const s = row.session;
  const cand = s.duplicate_candidates.find((x) => x.activity_id === c.target) ?? s.duplicate_candidates[0];
  if (!cand) return null;
  const titleCarried = c.action === "replace" && c.carry.title;
  const compare = (label: string, existing: ReactNode, fresh: ReactNode, match?: boolean) => (
    <tr className="border-b border-line last:border-0">
      <td className="p-2 text-muted">{label}</td>
      <td className="num p-2 text-right">{existing}</td>
      <td className="num p-2 text-right">{fresh}</td>
      <td className="p-2">{match && <span className="rounded-md bg-card-2 px-2 py-0.5 text-xs text-gold">{t("import.dup.matches")}</span>}</td>
    </tr>
  );
  return (
    <section
      className={`rounded-card border bg-card p-4 shadow-sm ${c.action ? "border-line" : "border-gold"}`}
      aria-label={t("import.dup.title", { file: row.fileName })}
    >
      <h2 className="mb-3 flex items-center gap-2 text-xs font-semibold uppercase tracking-wide text-muted">
        <Copy size={14} className="text-gold" /> {t("import.dup.title", { file: row.fileName })}
      </h2>
      <div className="grid gap-4 lg:grid-cols-12">
        <div className="lg:col-span-7">
          {s.duplicate_candidates.length > 1 && (
            <label className="mb-2 flex items-center gap-2 text-sm">
              {t("import.dup.candidate")}
              <select className={input} value={cand.activity_id} onChange={(e) => set(row.key, { target: e.target.value })}>
                {s.duplicate_candidates.map((x) => (
                  <option key={x.activity_id} value={x.activity_id}>
                    {x.title} · {dateTime(x.start_time, tz)}
                  </option>
                ))}
              </select>
            </label>
          )}
          <div className="overflow-x-auto rounded-xl border border-line">
            <table className="w-full text-sm">
              <thead className="text-xs text-muted">
                <tr className="border-b border-line">
                  <th className="p-2" />
                  <th className="p-2 text-right">{t("import.dup.existing")}</th>
                  <th className="p-2 text-right">{t("import.dup.new")}</th>
                  <th className="p-2" />
                </tr>
              </thead>
              <tbody>
                {compare(
                  t("import.col.title"),
                  <span className="font-sans">{cand.title}</span>,
                  <span className="font-sans">{titleCarried ? cand.title : s.title}</span>,
                )}
                {compare(t("import.dup.source"), "–", <span className="font-sans">{row.fileName}</span>)}
                {compare(t("import.col.start"), dateTime(cand.start_time, tz), dateTime(s.start_time, tz), cand.matched_criteria.includes("start_time"))}
                {compare(
                  t("import.dup.totalTime"),
                  duration(cand.total_time_ms),
                  duration(s.total_time_ms),
                  cand.matched_criteria.includes("total_time"),
                )}
                {compare(t("import.col.distance"), `${km(cand.distance_m)} km`, `${km(s.distance_m)} km`, cand.matched_criteria.includes("distance"))}
              </tbody>
            </table>
          </div>
          <p className="mt-2 text-sm text-muted">{titleCarried ? t("import.dup.titleCarried") : t("import.dup.hint")}</p>
          <Problem text={c.problem} />
        </div>
        <div className="grid content-start gap-2 lg:col-span-5" role="radiogroup">
          {options.map((o) => (
            <label
              key={o.value}
              className={`cursor-pointer rounded-xl border px-3 py-2 ${c.action === o.value ? "border-accent bg-card-2" : "border-line"}`}
            >
              <input
                type="radio"
                name={`dup-${row.key}`}
                value={o.value}
                checked={c.action === o.value}
                onChange={() => set(row.key, { action: o.value, target: cand.activity_id })}
              />{" "}
              <b>{t(o.label)}</b>
              <div className="text-sm text-muted">{t(o.text)}</div>
            </label>
          ))}
          {c.action === "replace" && (
            <div className="text-sm text-muted">
              {t("import.dup.carry")}{" "}
              {carryKeys.map((k) => (
                <label key={k} className="mr-3 whitespace-nowrap">
                  <input type="checkbox" checked={c.carry[k]} onChange={(e) => set(row.key, { carry: { ...c.carry, [k]: e.target.checked } })} />{" "}
                  {t(`carry.${k}` as TextKey)}
                </label>
              ))}
            </div>
          )}
        </div>
      </div>
    </section>
  );
}

export function TimeCard({ row, c, set }: { row: Row; c: Choice; set: SetChoice }) {
  const s = row.session;
  const open = isOpen(row, c);
  return (
    <section className={`rounded-card border bg-card p-4 shadow-sm ${open ? "border-gold" : "border-line"}`}>
      <h2 className="mb-1 flex items-center gap-2 text-xs font-semibold uppercase tracking-wide text-muted">
        <Clock size={14} className="text-gold" /> {t("import.time.title", { file: row.fileName })}
      </h2>
      <p className="mb-3 text-sm text-muted">
        {t("import.time.hint")} {t("import.time.points", { n: s.point_count })}
        {s.avg_heart_rate_bpm ? ` · Ø ${s.avg_heart_rate_bpm} bpm` : ""}
      </p>
      <div className="flex flex-wrap items-end gap-4 text-sm">
        <label className="flex items-center gap-2">
          <input type="checkbox" checked={c.include} onChange={(e) => set(row.key, { include: e.target.checked })} />
          {t("import.time.include")}
        </label>
        <span className="flex items-center gap-2">
          <SportIcon sport={s.sport_class} />
          {s.title}
        </span>
        <label className="grid gap-1">
          <span className="text-xs text-muted">{t("import.time.start")}</span>
          <input
            type="datetime-local"
            className={input}
            value={c.startLocal}
            disabled={!c.include}
            onChange={(e) => set(row.key, { startLocal: e.target.value, ambiguous: false, occurrence: undefined, problem: undefined })}
          />
        </label>
        <label className="grid gap-1">
          <span className="text-xs text-muted">{t("import.time.duration")}</span>
          <input
            className={`${input} num w-28`}
            placeholder="1:30:00"
            value={c.durationText}
            disabled={!c.include}
            onChange={(e) => set(row.key, { durationText: e.target.value })}
          />
        </label>
        {c.ambiguous && (
          <label className="grid gap-1">
            <span className="text-xs text-muted">{t("import.time.occurrence")}</span>
            <select
              className={input}
              value={c.occurrence ?? ""}
              onChange={(e) => set(row.key, { occurrence: (e.target.value || undefined) as Choice["occurrence"], problem: undefined })}
            >
              <option value="" />
              <option value="first">{t("import.time.first")}</option>
              <option value="second">{t("import.time.second")}</option>
            </select>
          </label>
        )}
      </div>
      <Problem text={c.problem} />
    </section>
  );
}

export function NotImportable({ rows, rejected, tz }: { rows: Row[]; rejected: FileEntry[]; tz?: string }) {
  const badge = (key: string, tone: string) => (
    <span className={`shrink-0 rounded-md bg-card-2 px-2 py-0.5 text-xs font-medium ${tone}`}>{t(`import.locked.${key}` as TextKey)}</span>
  );
  const tone: Record<string, string> = { locked_vehicle: "text-danger", locked_limit: "text-warn", locked_existing: "text-muted" };
  return (
    <Card title={t("import.locked.title")}>
      <ul className="divide-y divide-line">
        {rows.map((r) => (
          <li key={r.key} className="flex items-start gap-3 py-2 text-sm">
            {badge(r.session.state, tone[r.session.state] ?? "text-muted")}
            <div className="min-w-0">
              <b>{r.fileName}</b>
              {r.session.start_time && (
                <span className="text-muted">
                  {" "}
                  · {dateTime(r.session.start_time, tz)} · {km(r.session.distance_m)} km · {duration(r.session.total_time_ms)}
                </span>
              )}
              <div className="text-muted">
                {r.session.state === "locked_existing" ? t("import.locked.existingText") : (r.session.lock_reason ?? "")}
              </div>
            </div>
          </li>
        ))}
        {rejected.map((e) => (
          <li key={e.key} className="flex items-start gap-3 py-2 text-sm">
            {badge("rejected", "text-warn")}
            <div className="min-w-0">
              <b>{e.name}</b>
              <div className="text-muted">{e.error ? errorText(e.error.code, e.error.message) : ""}</div>
            </div>
          </li>
        ))}
      </ul>
    </Card>
  );
}

export function ConfirmBar({
  s,
  saving,
  onCancel,
  onConfirm,
}: {
  s: Summary;
  saving: boolean;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  return (
    <div className="sticky bottom-3 flex flex-wrap items-center gap-3 rounded-card border border-line bg-card p-4 shadow-lg">
      <span>
        <b className="num">{s.toSave}</b> {t("import.bar.toSave")}
        {s.open > 0 && <span className="text-gold"> · {t("import.bar.open", { n: s.open })}</span>}
      </span>
      <span className="flex-1" />
      <button type="button" className={secondaryButton} disabled={saving} onClick={onCancel}>
        {t("import.bar.cancel")}
      </button>
      <button type="button" className={primaryButton} disabled={saving || s.open > 0} onClick={onConfirm}>
        {saving ? t("import.bar.saving") : t("import.bar.confirm")}
      </button>
    </div>
  );
}
