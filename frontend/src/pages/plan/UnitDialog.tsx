import { useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { apiDelete, apiPatch, apiPost, type HrZoneSet, type LoadSettings, type PlannedUnit, type PlannedUnitResponse, type SubSportCatalog } from "../../api/client";
import { Dialog, primaryButton, secondaryButton } from "../../components/Dialog";
import { useFlash } from "../../components/Flash";
import { Problem } from "../../components/Problem";
import { t, type TextKey } from "../../i18n";
import { num } from "../../utils/format";
import { SPORT_CLASSES } from "../../utils/sports";
import { germanDate } from "../seasons/seasonModel";
import { applicableSet, formOf, hm, previewLoad, unitInput, unitProblem, type UnitForm } from "./planModel";

const input = "w-full rounded-lg border border-line bg-card-2 px-2 py-1.5 text-sm text-text disabled:opacity-60";
const pill = (on: boolean) =>
  `rounded-full border px-3 py-1 text-sm ${on ? "border-transparent bg-accent-strong text-white" : "border-line bg-card-2 text-muted hover:text-text"}`;
const ZONE_LABELS = ["Z1", "Z2", "Z3", "Z4", "Z5"];
const CLASSES = SPORT_CLASSES.filter((c) => c !== "unknown");

/** Geplante Einheit anlegen oder ändern (E8): Belastung als Minuten je Zone oder als Zielbelastung, mit Vorschau. */
export function UnitDialog({
  unit,
  date,
  zoneSets,
  settings,
  catalog,
  onClose,
}: {
  unit: PlannedUnit | null;
  date: string;
  zoneSets: HrZoneSet[];
  settings?: LoadSettings;
  catalog?: SubSportCatalog;
  onClose: () => void;
}) {
  const [form, setForm] = useState<UnitForm>(() => formOf(unit, date));
  const [copyTo, setCopyTo] = useState<string | null>(null);
  const queryClient = useQueryClient();
  const flash = useFlash();
  const set = (patch: Partial<UnitForm>) => setForm((f) => ({ ...f, ...patch }));
  const zoneSet = applicableSet(zoneSets, form.sportClass, form.date);
  const preview = previewLoad(form, zoneSet, settings);
  const problem = unitProblem(form);
  const done = unit?.status === "done";
  const subs = catalog?.[form.sportClass] ?? [];

  const refresh = () => {
    void queryClient.invalidateQueries({ queryKey: ["plan"] });
    void queryClient.invalidateQueries({ queryKey: ["form"] });
  };
  const save = useMutation({
    mutationFn: () =>
      unit
        ? apiPatch<PlannedUnitResponse>(`/plan/units/${unit.id}`, unitInput(form, !done))
        : apiPost<PlannedUnitResponse>("/plan/units", unitInput(form, false)),
    onSuccess: () => {
      refresh();
      flash(t("plan.saved"));
      onClose();
    },
  });
  const remove = useMutation({
    mutationFn: () => apiDelete(`/plan/units/${unit!.id}`),
    onSuccess: () => {
      refresh();
      flash(t("plan.deleted"));
      onClose();
    },
  });
  const copy = useMutation({
    mutationFn: (day: string) => apiPost<PlannedUnitResponse>("/plan/units", unitInput({ ...form, date: day }, false)),
    onSuccess: (_, day) => {
      refresh();
      flash(t("plan.copied", { date: germanDate(day) }));
      onClose();
    },
  });
  const busy = save.isPending || remove.isPending || copy.isPending;

  return (
    <Dialog
      title={t(unit ? "plan.unit.edit" : "plan.unit.new")}
      onClose={onClose}
      busy={busy}
      footer={
        <>
          {unit && (
            <button type="button" className={`${secondaryButton} text-danger`} disabled={busy} onClick={() => remove.mutate()}>
              {t("plan.delete")}
            </button>
          )}
          {unit && copyTo == null && (
            <button type="button" className={secondaryButton} disabled={busy || problem != null} onClick={() => setCopyTo(form.date)}>
              {t("plan.copy")}
            </button>
          )}
          <span className="flex-1" />
          <button type="button" className={secondaryButton} onClick={onClose} disabled={busy}>
            {t("edit.cancel")}
          </button>
          <button type="button" className={primaryButton} disabled={busy || problem != null} onClick={() => save.mutate()}>
            {save.isPending ? t("edit.saving") : t("edit.save")}
          </button>
        </>
      }
    >
      <div className="grid gap-3 text-sm">
        {copyTo != null && (
          <div className="flex flex-wrap items-end gap-2 rounded-lg border border-accent/50 bg-card-2 p-2">
            <label className="grid gap-1">
              <span className="text-xs font-medium text-muted">{t("plan.copyTo")}</span>
              <input type="date" className={input} value={copyTo} onChange={(e) => setCopyTo(e.target.value)} />
            </label>
            <button type="button" className={primaryButton} disabled={busy || !copyTo} onClick={() => copy.mutate(copyTo)}>
              {t("plan.copyConfirm")}
            </button>
            <button type="button" className={secondaryButton} onClick={() => setCopyTo(null)}>
              {t("edit.cancel")}
            </button>
          </div>
        )}
        <div className="grid gap-3 sm:grid-cols-3">
          <label className="grid gap-1">
            <span className="text-xs font-medium text-muted">{t("plan.field.date")}</span>
            <input type="date" className={input} value={form.date} disabled={busy} onChange={(e) => set({ date: e.target.value })} />
          </label>
          <label className="grid gap-1">
            <span className="text-xs font-medium text-muted">{t("plan.field.sport")}</span>
            <select
              className={input}
              value={form.sportClass}
              disabled={busy}
              onChange={(e) => set({ sportClass: e.target.value as UnitForm["sportClass"], subSport: "" })}
            >
              {CLASSES.map((c) => (
                <option key={c} value={c}>
                  {t(`sport.${c}` as TextKey)}
                </option>
              ))}
            </select>
          </label>
          <label className="grid gap-1">
            <span className="text-xs font-medium text-muted">{t("plan.field.subSport")}</span>
            <select className={input} value={form.subSport} disabled={busy} onChange={(e) => set({ subSport: e.target.value })}>
              <option value="">—</option>
              {subs.map((s) => (
                <option key={s.key} value={s.key}>
                  {s.name}
                </option>
              ))}
            </select>
          </label>
        </div>
        <label className="grid gap-1">
          <span className="text-xs font-medium text-muted">{t("plan.field.title")}</span>
          <input
            className={input}
            value={form.title}
            maxLength={120}
            placeholder={t("plan.field.titlePlaceholder")}
            disabled={busy}
            onChange={(e) => set({ title: e.target.value })}
          />
        </label>

        <div className="grid gap-1">
          <span className="text-xs font-medium text-muted">{t("plan.field.mode")}</span>
          <div className="flex flex-wrap gap-2" role="radiogroup" aria-label={t("plan.field.mode")}>
            {(["zones", "target"] as const).map((m) => (
              <button key={m} type="button" role="radio" aria-checked={form.mode === m} className={pill(form.mode === m)} onClick={() => set({ mode: m })}>
                {t(`plan.mode.${m}` as TextKey)}
              </button>
            ))}
          </div>
        </div>

        {form.mode === "zones" ? (
          <div className="grid gap-1">
            <div className="grid grid-cols-5 gap-2">
              {form.zones.map((v, i) => (
                <label key={i} className="grid gap-0.5 text-xs text-muted">
                  <span>
                    {ZONE_LABELS[i]}
                    {zoneSet && (
                      <span className="num ml-1">
                        {zoneSet.zones_bpm[i]![0]}
                        {zoneSet.zones_bpm[i]![1] != null ? `–${zoneSet.zones_bpm[i]![1]}` : "+"}
                      </span>
                    )}
                  </span>
                  <input
                    className={`${input} num text-right`}
                    inputMode="numeric"
                    aria-label={t("plan.zoneMinutes", { zone: i + 1 })}
                    value={v}
                    disabled={busy}
                    onChange={(e) => set({ zones: form.zones.map((z, k) => (k === i ? e.target.value : z)) })}
                  />
                </label>
              ))}
            </div>
            <span className="text-xs text-muted">{zoneSet ? t("plan.zonesHint") : t("plan.noZones")}</span>
          </div>
        ) : (
          <div className="grid gap-3 sm:grid-cols-3">
            <label className="grid gap-1">
              <span className="text-xs font-medium text-muted">{t("plan.field.duration")}</span>
              <input className={`${input} num`} value={form.duration} disabled={busy} onChange={(e) => set({ duration: e.target.value })} />
            </label>
            <label className="grid gap-1">
              <span className="text-xs font-medium text-muted">{t("plan.field.targetLoad")}</span>
              <input className={`${input} num`} inputMode="numeric" value={form.targetLoad} disabled={busy} onChange={(e) => set({ targetLoad: e.target.value })} />
            </label>
            <label className="grid gap-1">
              <span className="text-xs font-medium text-muted">{t("plan.field.intensive")}</span>
              <input className={`${input} num`} inputMode="numeric" value={form.intensivePct} disabled={busy} onChange={(e) => set({ intensivePct: e.target.value })} />
            </label>
          </div>
        )}

        {preview && (
          <div className="flex flex-wrap gap-x-4 gap-y-1 rounded-lg bg-card-2 px-3 py-2" data-testid="plan-preview">
            <span>
              {t("plan.duration")} <b className="num">{hm(preview.minutes)}</b>
            </span>
            <span>
              {t("plan.load")} <b className="num">{num(preview.total)}</b>
              {preview.method === "estimated" && <span className="text-muted"> ({t("plan.estimated")})</span>}
            </span>
            <span>
              {t("load.aerobic")} <b className="num">{num(preview.aerobic)}</b>
            </span>
            <span>
              {t("load.anaerobic")} <b className="num">{num(preview.anaerobic)}</b>
            </span>
          </div>
        )}
        {problem && problem !== "title" && <p className="text-xs text-danger">{t(`plan.problem.${problem}` as TextKey)}</p>}

        <label className="grid gap-1">
          <span className="text-xs font-medium text-muted">{t("plan.field.note")}</span>
          <textarea className={`${input} min-h-16`} rows={2} value={form.note} disabled={busy} onChange={(e) => set({ note: e.target.value })} />
        </label>

        {unit && !done && (
          <div className="grid gap-1">
            <span className="text-xs font-medium text-muted">{t("plan.field.status")}</span>
            <div className="flex flex-wrap items-center gap-2" role="radiogroup" aria-label={t("plan.field.status")}>
              {(["planned", "skipped"] as const).map((st) => (
                <button key={st} type="button" role="radio" aria-checked={form.status === st} className={pill(form.status === st)} onClick={() => set({ status: st })}>
                  {t(`plan.status.${st}` as TextKey)}
                </button>
              ))}
              <span className="text-xs text-muted">{t("plan.statusHint")}</span>
            </div>
          </div>
        )}
        {done && <p className="text-xs text-muted">{t("plan.doneHint")}</p>}
        {(save.error ?? remove.error ?? copy.error) && <Problem error={save.error ?? remove.error ?? copy.error} />}
      </div>
    </Dialog>
  );
}
