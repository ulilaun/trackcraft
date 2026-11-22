import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Plus, Trash2 } from "lucide-react";
import {
  apiDelete,
  apiGet,
  apiPatch,
  apiPost,
  type HrZoneSet,
  type HrZoneSetResponse,
  type HrZonesResponse,
  type LoadSettingsResponse,
  type SportClass,
  type ZoneModel,
} from "../../api/client";
import { Card } from "../../components/Card";
import { primaryButton, secondaryButton } from "../../components/Dialog";
import { useFlash } from "../../components/Flash";
import { Problem } from "../../components/Problem";
import { SportIcon } from "../../components/SportIcon";
import { t, type TextKey } from "../../i18n";
import { SPORT_CLASSES } from "../../utils/sports";
import { germanDate } from "../seasons/seasonModel";
import { addDays } from "../../utils/format";
import {
  DEFAULT_BOUNDS,
  ZONE_MODELS,
  anaerobicFrom,
  newZoneForm,
  previewBpm,
  zoneFormOf,
  zoneInput,
  zoneProblem,
  type ZoneForm,
} from "../form/formModel";

const input = "w-full rounded-lg border border-line bg-card-2 px-2 py-1.5 text-sm text-text disabled:opacity-60";
const small = "w-16 rounded-lg border border-line bg-card-2 px-2 py-1 text-right text-sm text-text";
/** Sportklassen, die eigene Zonen bekommen können */
const ZONE_CLASSES: SportClass[] = SPORT_CLASSES.filter((c) => c !== "unknown");

type Tab = SportClass | "general";
const scopeOf = (tab: Tab): SportClass | null => (tab === "general" ? null : tab);

/** Abschnitt „Herzfrequenz“ der Kontoseite: Faktor der Formel und HF-Zonensätze je Sportklasse mit Verlauf. */
export function HeartRateZones({ today }: { today: string }) {
  const zones = useQuery({ queryKey: ["hr-zones"], queryFn: () => apiGet<HrZonesResponse>("/hr-zones") });
  const settings = useQuery({ queryKey: ["form-settings"], queryFn: () => apiGet<LoadSettingsResponse>("/form/settings") });
  const queryClient = useQueryClient();
  const [tab, setTab] = useState<Tab>("general");
  const [extra, setExtra] = useState<SportClass | null>(null);
  const [picked, setPicked] = useState<string | "new" | null>(null);

  const factor = useMutation({
    mutationFn: (sex_factor: number) => apiPatch<LoadSettingsResponse>("/form/settings", { sex_factor }),
    onSuccess: (r) => {
      queryClient.setQueryData(["form-settings"], r);
      void queryClient.invalidateQueries({ queryKey: ["form"] });
    },
  });

  const sets = zones.data?.data ?? [];
  const classesWithSets = ZONE_CLASSES.filter((c) => sets.some((s) => s.sport_class === c) || c === extra);
  const free = ZONE_CLASSES.filter((c) => !classesWithSets.includes(c));
  const history = sets.filter((s) => s.sport_class === scopeOf(tab));
  // Vorgabe: der neueste Stand; ohne Stand gleich ein neuer
  const current = picked === "new" ? null : (history.find((s) => s.id === picked) ?? history[history.length - 1] ?? null);
  const draft = picked === "new" || !current;
  const suggested = zones.data?.meta.suggestion.max_hr ?? null;
  const base = history[history.length - 1] ?? sets.find((s) => s.sport_class === null);

  const choose = (next: Tab) => {
    setTab(next);
    setPicked(null);
  };

  return (
    <Card>
      <div className="grid gap-4">
        <div className="flex flex-wrap items-baseline justify-between gap-2">
          <h2 className="text-lg font-semibold">{t("zones.title")}</h2>
          <span className="text-xs text-muted">{t("zones.subtitle")}</span>
        </div>
        {(zones.error || settings.error) && <Problem error={zones.error ?? settings.error} />}

        <label className="grid max-w-xs gap-1">
          <span className="text-xs font-medium text-muted">{t("zones.sexFactor")}</span>
          <select
            className={input}
            value={String(settings.data?.data.sex_factor ?? 1.92)}
            disabled={!settings.data || factor.isPending}
            onChange={(e) => factor.mutate(Number(e.target.value))}
          >
            <option value="1.92">{t("zones.sexFactor.male")}</option>
            <option value="1.67">{t("zones.sexFactor.female")}</option>
          </select>
        </label>

        <div className="flex flex-wrap items-end gap-1 border-b border-line" role="tablist">
          {(["general", ...classesWithSets] as Tab[]).map((k) => (
            <button
              key={k}
              type="button"
              role="tab"
              aria-selected={tab === k}
              onClick={() => choose(k)}
              className={`-mb-px flex items-center gap-1.5 border-b-2 px-3 py-1.5 text-sm ${tab === k ? "border-accent font-semibold text-text" : "border-transparent text-muted hover:text-text"}`}
            >
              {k === "general" ? (
                <>
                  {t("zones.general")}
                  <span className="text-[11px] font-normal text-muted">{t("zones.default")}</span>
                </>
              ) : (
                <>
                  <SportIcon sport={k} size={14} decorative />
                  {t(`sport.${k}` as TextKey)}
                </>
              )}
            </button>
          ))}
          {free.length > 0 && sets.some((s) => s.sport_class === null) && (
            <select
              className="mb-1 ml-1 rounded-lg border border-line bg-card-2 px-2 py-1 text-sm text-muted"
              aria-label={t("zones.addClass")}
              value=""
              onChange={(e) => {
                const c = e.target.value as SportClass;
                setExtra(c);
                setTab(c);
                setPicked("new");
              }}
            >
              <option value="">+ {t("zones.addClass")}</option>
              {free.map((c) => (
                <option key={c} value={c}>
                  {t(`sport.${c}` as TextKey)}
                </option>
              ))}
            </select>
          )}
        </div>

        {zones.data && (
          <ZoneEditor
            key={`${tab}/${draft ? "new" : current?.id}/${current?.valid_from ?? ""}`}
            scope={scopeOf(tab)}
            set={draft ? null : current}
            initial={draft ? newZoneForm(base, today, suggested) : zoneFormOf(current as HrZoneSet)}
            history={history}
            suggested={suggested}
            onPick={(id) => setPicked(id)}
            onSaved={(id) => setPicked(id)}
            onDeleted={() => {
              setPicked(null);
              if (history.length <= 1 && tab !== "general") {
                setExtra(null);
                setTab("general");
              }
            }}
          />
        )}
      </div>
    </Card>
  );
}

function ZoneEditor({
  scope,
  set,
  initial,
  history,
  suggested,
  onPick,
  onSaved,
  onDeleted,
}: {
  scope: SportClass | null;
  /** `null`: neuer, noch nicht gespeicherter Stand */
  set: HrZoneSet | null;
  initial: ZoneForm;
  history: HrZoneSet[];
  suggested: number | null;
  onPick: (id: string | "new") => void;
  onSaved: (id: string) => void;
  onDeleted: () => void;
}) {
  const [form, setForm] = useState<ZoneForm>(initial);
  const queryClient = useQueryClient();
  const flash = useFlash();
  const problem = zoneProblem(form);
  const lower = previewBpm(form);
  const anaerobic = anaerobicFrom(form, lower);
  const max = Number(form.max_hr) || 0;

  const after = () => {
    void queryClient.invalidateQueries({ queryKey: ["hr-zones"] });
    void queryClient.invalidateQueries({ queryKey: ["form"] });
    void queryClient.invalidateQueries({ queryKey: ["activities"] });
  };
  // „update“ ändert den gewählten Stand, „create“ legt einen weiteren Stand an (ab seinem Datum bis zum nächsten).
  const save = useMutation({
    mutationFn: (mode: "update" | "create") =>
      set && mode === "update"
        ? apiPatch<HrZoneSetResponse>(`/hr-zones/${set.id}`, zoneInput(form))
        : apiPost<HrZoneSetResponse>("/hr-zones", { ...zoneInput(form), sport_class: scope }),
    onSuccess: (r) => {
      after();
      flash(t("zones.saved"));
      onSaved(r.data.id);
    },
  });
  const remove = useMutation({
    mutationFn: () => apiDelete(`/hr-zones/${set?.id}`),
    onSuccess: () => {
      after();
      flash(t("zones.deleted"));
      onDeleted();
    },
  });
  const busy = save.isPending || remove.isPending;
  const set_ = (p: Partial<ZoneForm>) => {
    setForm((f) => ({ ...f, ...p }));
    save.reset();
  };
  const setModel = (model: ZoneModel) => set_({ model, bounds: DEFAULT_BOUNDS[model].map(String) });
  // Ein weiterer Stand braucht ein Datum, an dem in dieser Sportklasse noch keiner beginnt.
  const dateTaken = history.some((h) => h.valid_from === form.valid_from);
  const dateMoved = !!set && form.valid_from !== set.valid_from;
  const top = lower ? Math.max(max, (lower[4] ?? 0) + 5) : 0;
  const scale = lower ? top - (lower[0] ?? 0) : 0;

  return (
    <form
      className="grid gap-4"
      onSubmit={(e) => {
        e.preventDefault();
        if (!problem && !busy) save.mutate("update");
      }}
    >
      <p className="text-xs text-muted">
        {scope ? t("zones.classHint", { sport: t(`sport.${scope}` as TextKey) }) : history.length === 0 ? t("zones.none") : t("zones.generalHint")}
      </p>
      <div className="grid gap-4 md:grid-cols-[minmax(0,1.4fr)_minmax(0,1fr)]">
        <div className="grid content-start gap-3">
          <div className="grid grid-cols-2 gap-3">
            <label className="grid gap-1">
              <span className="text-xs font-medium text-muted">{t("zones.validFrom")}</span>
              <input type="date" className={input} value={form.valid_from} disabled={busy} onChange={(e) => set_({ valid_from: e.target.value })} />
              {dateMoved && <span className="text-xs text-warn">{t("zones.dateMoved")}</span>}
            </label>
            <div className="grid content-start gap-1">
              <label className="grid gap-1">
                <span className="text-xs font-medium text-muted">{t("zones.maxHr")}</span>
                <input className={input} inputMode="numeric" value={form.max_hr} disabled={busy} onChange={(e) => set_({ max_hr: e.target.value })} />
              </label>
              {suggested && <span className="text-xs text-muted">{t("zones.suggestion", { max: suggested })}</span>}
            </div>
            <label className="grid gap-1">
              <span className="text-xs font-medium text-muted">{t("zones.restingHr")}</span>
              <input className={input} inputMode="numeric" value={form.resting_hr} disabled={busy} onChange={(e) => set_({ resting_hr: e.target.value })} />
            </label>
            <label className="grid gap-1">
              <span className="text-xs font-medium text-muted">{t("zones.thresholdHr")}</span>
              <input
                className={input}
                inputMode="numeric"
                placeholder="–"
                value={form.threshold_hr}
                disabled={busy}
                onChange={(e) => set_({ threshold_hr: e.target.value })}
              />
            </label>
          </div>
          <label className="grid gap-1">
            <span className="text-xs font-medium text-muted">{t("zones.model")}</span>
            <select className={input} value={form.model} disabled={busy} onChange={(e) => setModel(e.target.value as ZoneModel)}>
              {ZONE_MODELS.map((m) => (
                <option key={m} value={m}>
                  {t(`zones.model.${m}` as TextKey)}
                </option>
              ))}
            </select>
          </label>
          <table className="w-full text-sm">
            <thead>
              <tr className="text-xs text-muted">
                <th className="py-1 text-left font-medium">{t("zones.zone")}</th>
                <th className="py-1 text-right font-medium">{t("zones.from")}</th>
                <th className="py-1 text-right font-medium">{t("zones.bpm")}</th>
              </tr>
            </thead>
            <tbody>
              {form.bounds.map((b, i) => (
                <tr key={i} className="border-t border-line">
                  <td className="py-1">
                    <span className="inline-flex items-center gap-2">
                      <i className="inline-block h-2.5 w-2.5 rounded-sm" style={{ background: `var(--color-zone-${i + 1})` }} />
                      Z{i + 1} {t(`zones.name.${i + 1}` as TextKey)}
                    </span>
                  </td>
                  <td className="py-1 text-right">
                    <input
                      className={small}
                      inputMode="numeric"
                      aria-label={`Z${i + 1} ${t("zones.from")}`}
                      value={b}
                      disabled={busy}
                      onChange={(e) => set_({ bounds: form.bounds.map((x, j) => (j === i ? e.target.value : x)) })}
                    />
                  </td>
                  <td className="num py-1 text-right" data-testid={`zone-bpm-${i + 1}`}>
                    {lower ? `${lower[i]}–${i < 4 ? (lower[i + 1] ?? 0) - 1 : form.model === "threshold" ? "max" : form.max_hr}` : "–"}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
          {lower && scale > 0 && (
            <div>
              <div className="flex h-3 overflow-hidden rounded-full" aria-hidden="true">
                {lower.map((lo, i) => {
                  const up = i < 4 ? (lower[i + 1] ?? top) : top;
                  return <i key={i} style={{ width: `${(Math.max(0, up - lo) / scale) * 100}%`, background: `var(--color-zone-${i + 1})` }} />;
                })}
              </div>
              <div className="num mt-0.5 flex justify-between text-xs text-muted">
                <span>{lower[0]}</span>
                <span>{top}</span>
              </div>
            </div>
          )}
          <p className="text-xs text-muted">
            {anaerobic != null &&
              t("zones.anaerobic", { bpm: anaerobic, source: t(form.threshold_hr.trim() ? "zones.anaerobic.threshold" : "zones.anaerobic.zone4") })}
            {" · "}
            <button type="button" className="text-accent hover:underline" onClick={() => set_({ bounds: DEFAULT_BOUNDS[form.model].map(String) })}>
              {t("zones.resetBounds")}
            </button>
          </p>
        </div>

        <div className="grid content-start gap-2">
          <h3 className="text-xs font-semibold uppercase tracking-wide text-muted">{t("zones.history")}</h3>
          <ul className="grid gap-1">
            {history.map((h, i) => (
              <li key={h.id}>
                <button
                  type="button"
                  onClick={() => onPick(h.id)}
                  className={`num flex w-full justify-between gap-3 rounded-lg px-2 py-1.5 text-left text-sm ${set?.id === h.id ? "bg-card-2 font-semibold" : "hover:bg-card-2"}`}
                >
                  <span>
                    {germanDate(h.valid_from)} –{" "}
                    {history[i + 1] ? germanDate(addDays(history[i + 1]!.valid_from, -1)) : t("zones.open")}
                  </span>
                  <span className="text-muted">
                    {h.max_hr} / {h.resting_hr}
                    {h.threshold_hr ? ` / ${h.threshold_hr}` : ""}
                  </span>
                </button>
              </li>
            ))}
            {!set && <li className="rounded-lg bg-card-2 px-2 py-1.5 text-sm italic text-muted">{t("zones.unsaved")}</li>}
          </ul>
          {set && (
            <button type="button" className={`${secondaryButton} inline-flex items-center justify-center gap-1.5`} onClick={() => onPick("new")} disabled={busy}>
              <Plus size={14} />
              {t("zones.newState")}
            </button>
          )}
          <p className="text-xs text-muted">{t("zones.historyHint")}</p>
          {set && (
            <button
              type="button"
              className="inline-flex items-center gap-1.5 self-start text-sm text-danger hover:underline disabled:opacity-50"
              onClick={() => remove.mutate()}
              disabled={busy}
            >
              <Trash2 size={14} />
              {scope && history.length === 1 ? t("zones.removeClass", { sport: t(`sport.${scope}` as TextKey) }) : t("zones.deleteState")}
            </button>
          )}
        </div>
      </div>

      {problem && (
        <p className="text-sm text-danger" role="alert">
          {t(`zones.problem.${problem}` as TextKey)}
        </p>
      )}
      {(save.error || remove.error) && <Problem error={save.error ?? remove.error} />}
      <div className="flex flex-wrap items-center justify-end gap-2 border-t border-line pt-3">
        {set && dateTaken && <span className="mr-auto text-xs text-muted">{t("zones.newNeedsDate")}</span>}
        {set && (
          <button
            type="button"
            className={secondaryButton}
            disabled={!!problem || busy || dateTaken}
            onClick={() => save.mutate("create")}
          >
            {t("zones.saveAsNew")}
          </button>
        )}
        <button type="submit" className={primaryButton} disabled={!!problem || busy}>
          {save.isPending ? t("edit.saving") : t(set ? "zones.saveChanges" : "zones.save")}
        </button>
      </div>
    </form>
  );
}
