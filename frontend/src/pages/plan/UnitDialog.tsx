import { useState } from "react";
import { Repeat } from "lucide-react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  apiDelete,
  apiGet,
  apiPatch,
  apiPost,
  type HrZoneSet,
  type LoadSettings,
  type PlanTemplate,
  type PlanTemplateResponse,
  type PlanTemplatesResponse,
  type PlannedUnit,
  type PlannedUnitResponse,
  type SubSportCatalog,
} from "../../api/client";
import { Dialog, primaryButton, secondaryButton } from "../../components/Dialog";
import { useFlash } from "../../components/Flash";
import { Problem } from "../../components/Problem";
import { t, type TextKey } from "../../i18n";
import { addDays, num } from "../../utils/format";
import { SPORT_CLASSES } from "../../utils/sports";
import { germanDate } from "../seasons/seasonModel";
import {
  MAX_SERIES,
  applicableSet,
  changedInput,
  formOf,
  formOfTemplate,
  hm,
  previewLoad,
  repeatProblem,
  seriesDates,
  templateInput,
  unitInput,
  unitProblem,
  type UnitForm,
} from "./planModel";

const input = "w-full rounded-lg border border-line bg-card-2 px-2 py-1.5 text-sm text-text disabled:opacity-60";
const pill = (on: boolean) =>
  `rounded-full border px-3 py-1 text-sm ${on ? "border-transparent bg-accent-strong text-white" : "border-line bg-card-2 text-muted hover:text-text"}`;
const ZONE_LABELS = ["Z1", "Z2", "Z3", "Z4", "Z5"];
const CLASSES = SPORT_CLASSES.filter((c) => c !== "unknown");

/** Wohin eine neue Einheit gespeichert wird (#38). */
export type SaveTarget = "calendar" | "both" | "template";

export function useTemplates(enabled = true) {
  return useQuery({ queryKey: ["plan-templates"], enabled, queryFn: () => apiGet<PlanTemplatesResponse>("/plan/templates") });
}

/**
 * Geplante Einheit anlegen oder ändern (E8): Belastung als Minuten je Zone oder als Zielbelastung, mit Vorschau.
 * Neue Einheiten können aus einer Vorlage gefüllt und zusätzlich oder nur als Vorlage gespeichert werden. Mit
 * `template` (ohne `unit`) bearbeitet der Dialog eine Vorlage (Einstellungen → Plan-Vorlagen); `template: null`
 * zusammen mit `templateOnly` legt dort eine neue an.
 */
export function UnitDialog({
  unit,
  date,
  zoneSets,
  settings,
  catalog,
  template,
  templateOnly = false,
  onClose,
}: {
  unit: PlannedUnit | null;
  date: string;
  zoneSets: HrZoneSet[];
  settings?: LoadSettings;
  catalog?: SubSportCatalog;
  /** Vorlage bearbeiten (nur mit `templateOnly`) */
  template?: PlanTemplate | null;
  /** Dialog für eine Vorlage (Einstellungen): kein Datum, kein Status, kein Wiederholen */
  templateOnly?: boolean;
  onClose: () => void;
}) {
  const [form, setForm] = useState<UnitForm>(() => (template ? formOfTemplate(template, { date, status: "planned" }) : formOf(unit, date)));
  const [copyTo, setCopyTo] = useState<string | null>(null);
  // Wiederholen: 0 = nein, sonst alle N Wochen bis `until`
  const [repeat, setRepeat] = useState(0);
  const [until, setUntil] = useState(() => addDays(unit?.date ?? date, 56));
  const inSeries = !!unit?.series_id && (unit.series_following ?? 0) > 0;
  const [scope, setScope] = useState<"this" | "following">("this");
  // Neue Einheit: Kalender, Kalender + Vorlage oder nur Vorlage; Name der Vorlage folgt dem Titel, bis man ihn ändert
  const isNew = !unit && !templateOnly;
  const [target, setTarget] = useState<SaveTarget>(templateOnly ? "template" : "calendar");
  const [name, setName] = useState<string | null>(template?.name ?? null);
  const templateName = name ?? form.title;
  const withTemplate = target !== "calendar";
  const inCalendar = target !== "template";
  const templates = useTemplates(isNew);
  const [chosen, setChosen] = useState("");

  const queryClient = useQueryClient();
  const flash = useFlash();
  const set = (patch: Partial<UnitForm>) => setForm((f) => ({ ...f, ...patch }));
  const zoneSet = applicableSet(zoneSets, form.sportClass, form.date);
  const preview = previewLoad(form, zoneSet, settings);
  const dates = repeat && inCalendar ? seriesDates(form.date, repeat, until) : [];
  const problem = unitProblem(form) ?? (repeat && inCalendar ? repeatProblem(dates) : null);
  const nameMissing = withTemplate && !templateName.trim();
  const query = scope === "following" ? "?scope=following" : "";
  const done = unit?.status === "done";
  const subs = catalog?.[form.sportClass] ?? [];

  const refresh = () => {
    void queryClient.invalidateQueries({ queryKey: ["plan"] });
    void queryClient.invalidateQueries({ queryKey: ["form"] });
  };
  const save = useMutation({
    mutationFn: async () => {
      if (templateOnly) {
        const body = templateInput(form, templateName);
        return template ? apiPatch<PlanTemplateResponse>(`/plan/templates/${template.id}`, body) : apiPost<PlanTemplateResponse>("/plan/templates", body);
      }
      // Erst die Vorlage: ein vergebener Name bricht ab, bevor etwas im Kalender landet
      if (withTemplate) await apiPost<PlanTemplateResponse>("/plan/templates", templateInput(form, templateName));
      if (!inCalendar) return null;
      const body: Record<string, unknown> =
        unit && scope === "following" ? changedInput(unitInput(formOf(unit, unit.date), !done), unitInput(form, !done)) : unitInput(form, !!unit && !done);
      if (repeat) body.repeat = { every_weeks: repeat, until };
      return unit ? apiPatch<PlannedUnitResponse>(`/plan/units/${unit.id}${query}`, body) : apiPost<PlannedUnitResponse>("/plan/units", body);
    },
    onSuccess: () => {
      if (withTemplate) void queryClient.invalidateQueries({ queryKey: ["plan-templates"] });
      if (inCalendar) refresh();
      flash(
        templateOnly
          ? t("plan.template.saved", { name: templateName.trim() })
          : !inCalendar
            ? t("plan.template.saved", { name: templateName.trim() })
            : repeat
              ? t("plan.repeat.saved", { n: dates.length })
              : t("plan.saved"),
      );
      onClose();
    },
  });
  const remove = useMutation({
    mutationFn: () => (templateOnly ? apiDelete(`/plan/templates/${template!.id}`) : apiDelete(`/plan/units/${unit!.id}${query}`)),
    onSuccess: () => {
      if (templateOnly) {
        void queryClient.invalidateQueries({ queryKey: ["plan-templates"] });
        flash(t("plan.template.deleted", { name: template!.name }));
      } else {
        refresh();
        flash(t("plan.deleted"));
      }
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
  const title = templateOnly ? (template ? "plan.template.edit" : "plan.template.new") : unit ? "plan.unit.edit" : "plan.unit.new";
  const canDelete = templateOnly ? !!template : !!unit;

  return (
    <Dialog
      title={t(title)}
      onClose={onClose}
      busy={busy}
      footer={
        <>
          {canDelete && (
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
          <button type="button" className={primaryButton} disabled={busy || problem != null || nameMissing} onClick={() => save.mutate()}>
            {save.isPending ? t("edit.saving") : t("edit.save")}
          </button>
        </>
      }
    >
      <div className="grid gap-3 text-sm">
        {isNew && (templates.data?.data.length ?? 0) > 0 && (
          <label className="grid gap-1 rounded-lg border border-line bg-card-2 p-2">
            <span className="text-xs font-medium text-muted">{t("plan.template.from")}</span>
            <select
              className={input}
              value={chosen}
              disabled={busy}
              onChange={(e) => {
                const tpl = templates.data?.data.find((x) => x.id === e.target.value);
                setChosen(e.target.value);
                if (tpl) setForm((f) => formOfTemplate(tpl, { date: f.date, status: f.status }));
              }}
            >
              <option value="">{t("plan.template.none")}</option>
              {templates.data?.data.map((x) => (
                <option key={x.id} value={x.id}>
                  {x.name} · {hm(x.load.minutes)} · {num(x.load.total)}
                </option>
              ))}
            </select>
          </label>
        )}
        {templateOnly && (
          <label className="grid gap-1">
            <span className="text-xs font-medium text-muted">{t("plan.template.name")}</span>
            <input className={input} value={templateName} maxLength={80} disabled={busy} onChange={(e) => setName(e.target.value)} />
          </label>
        )}
        {inSeries && (
          <div className="grid gap-1 rounded-lg border border-line bg-card-2 p-2" data-testid="plan-series-scope">
            <span className="flex items-center gap-1.5 text-xs text-muted">
              <Repeat size={13} aria-hidden />
              {unit!.series_following === 1 ? t("plan.series.infoOne") : t("plan.series.info", { n: unit!.series_following ?? 0 })}
            </span>
            <div className="flex flex-wrap gap-2" role="radiogroup" aria-label={t("plan.series.scope")}>
              {(["this", "following"] as const).map((sc) => (
                <button
                  key={sc}
                  type="button"
                  role="radio"
                  aria-checked={scope === sc}
                  className={pill(scope === sc)}
                  onClick={() => {
                    setScope(sc);
                    if (sc === "following") setRepeat(0);
                  }}
                >
                  {t(`plan.series.${sc}` as TextKey)}
                </button>
              ))}
            </div>
            {scope === "following" && <span className="text-xs text-muted">{t("plan.series.followingHint")}</span>}
          </div>
        )}
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
        <div className={`grid gap-3 ${inCalendar ? "sm:grid-cols-3" : "sm:grid-cols-2"}`}>
          {inCalendar && (
            <label className="grid gap-1">
              <span className="text-xs font-medium text-muted">{t("plan.field.date")}</span>
              <input type="date" className={input} value={form.date} disabled={busy} onChange={(e) => set({ date: e.target.value })} />
            </label>
          )}
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
        {problem && problem !== "title" && problem !== "repeatNone" && problem !== "repeatMany" && <p className="text-xs text-danger">{t(`plan.problem.${problem}` as TextKey)}</p>}

        <label className="grid gap-1">
          <span className="text-xs font-medium text-muted">{t("plan.field.note")}</span>
          <textarea className={`${input} min-h-16`} rows={2} value={form.note} disabled={busy} onChange={(e) => set({ note: e.target.value })} />
        </label>

        {scope === "this" && inCalendar && !templateOnly && (
          <div className="grid gap-1">
            <span className="text-xs font-medium text-muted">{t(unit ? "plan.repeat.fromHere" : "plan.repeat.title")}</span>
            <div className="flex flex-wrap gap-2" role="radiogroup" aria-label={t("plan.repeat.title")}>
              {[0, 1, 2, 3, 4].map((w) => (
                <button key={w} type="button" role="radio" aria-checked={repeat === w} className={pill(repeat === w)} onClick={() => setRepeat(w)}>
                  {w === 0 ? t("plan.repeat.none") : w === 1 ? t("plan.repeat.weekly") : t("plan.repeat.every", { n: w })}
                </button>
              ))}
            </div>
            {repeat > 0 && (
              <div className="flex flex-wrap items-end gap-3">
                <label className="grid gap-1">
                  <span className="text-xs font-medium text-muted">{t("plan.repeat.until")}</span>
                  <input type="date" className={input} value={until} disabled={busy} onChange={(e) => setUntil(e.target.value)} />
                </label>
                <span className={`pb-2 text-xs ${problem === "repeatNone" || problem === "repeatMany" ? "text-danger" : "text-muted"}`} data-testid="plan-repeat-count">
                  {problem === "repeatNone"
                    ? t("plan.problem.repeatNone")
                    : problem === "repeatMany"
                      ? t("plan.problem.repeatMany", { n: MAX_SERIES })
                      : t("plan.repeat.count", { n: dates.length, from: germanDate(dates[0]!), to: germanDate(dates.at(-1)!) })}
                </span>
              </div>
            )}
          </div>
        )}

        {isNew && (
          <div className="grid gap-1" data-testid="plan-save-target">
            <span className="text-xs font-medium text-muted">{t("plan.target.title")}</span>
            <div className="flex flex-wrap gap-2" role="radiogroup" aria-label={t("plan.target.title")}>
              {(["calendar", "both", "template"] as const).map((x) => (
                <button key={x} type="button" role="radio" aria-checked={target === x} className={pill(target === x)} onClick={() => setTarget(x)}>
                  {t(`plan.target.${x}` as TextKey)}
                </button>
              ))}
            </div>
            {withTemplate && (
              <label className="grid gap-1">
                <span className="text-xs font-medium text-muted">{t("plan.template.name")}</span>
                <input className={input} value={templateName} maxLength={80} disabled={busy} onChange={(e) => setName(e.target.value)} />
              </label>
            )}
          </div>
        )}

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
