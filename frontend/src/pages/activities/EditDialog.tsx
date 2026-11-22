import { useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import {
  ApiError,
  apiPatch,
  apiPost,
  type Activity,
  type ActivityResponse,
  type SportClass,
  type SubSportCatalog,
  type SubSportEntry,
} from "../../api/client";
import { Dialog, primaryButton, secondaryButton } from "../../components/Dialog";
import { useFlash } from "../../components/Flash";
import { problemText } from "../../components/Problem";
import { IconSelect } from "../../components/IconSelect";
import { SportIcon, SubSportIcon } from "../../components/SportIcon";
import { t, type TextKey } from "../../i18n";
import { dateTime, duration } from "../../utils/format";
import { subSportEntryLabel } from "../../utils/sports";
import { formProblem, initialForm, NEW_SUB_SPORT, patchOf, SPORT_CLASSES, subSportOptions, withSportClass, type EditForm } from "./editModel";

const input = "w-full rounded-lg border border-line bg-card-2 px-2 py-1.5 text-sm text-text disabled:opacity-60";

function Field({ label, children, hint, plain = false }: { label: string; children: React.ReactNode; hint?: string; plain?: boolean }) {
  // Eigene Auswahllisten nicht in <label>: ein Klick in die Liste würde sonst den Knopf erneut auslösen.
  const Box = plain ? "div" : "label";
  return (
    <Box className="grid gap-1">
      <span className="text-xs font-medium text-muted">{label}</span>
      {children}
      {hint && <span className="text-xs text-muted">{hint}</span>}
    </Box>
  );
}

/** Bearbeiten nach RFC-0017: Titel, Beschreibung, Ort, Sportklasse, Untertyp, Korrektur von Start und Gesamtzeit. */
export function EditDialog({
  activity: a,
  timeZone,
  catalog,
  onClose,
}: {
  activity: Activity;
  timeZone?: string;
  catalog?: SubSportCatalog;
  onClose: () => void;
}) {
  const [form, setForm] = useState<EditForm>(() => initialForm(a, timeZone));
  const [error, setError] = useState<string | null>(null);
  const corrected = a.start_time_original !== null || a.total_time_original_ms !== null;
  const [timeOpen, setTimeOpen] = useState(corrected);
  const queryClient = useQueryClient();
  const flash = useFlash();

  const set = (patch: Partial<EditForm>) => {
    setForm((f) => ({ ...f, ...patch }));
    setError(null);
  };
  const problem = formProblem(form);
  const patch = problem ? null : patchOf(a, form, timeZone);

  const save = useMutation({
    mutationFn: async () => {
      // Neuer Untertyp: erst im Katalog der Klasse anlegen, dann mit seinem Schlüssel speichern.
      let body = patch;
      if (form.subSport === NEW_SUB_SPORT) {
        const created = await apiPost<{ data: SubSportEntry }>("/sub-sports", { sport_class: form.sportClass, name: form.newSubSport.trim() });
        void queryClient.invalidateQueries({ queryKey: ["config"] });
        body = patchOf(a, { ...form, subSport: created.data.key }, timeZone);
      }
      return apiPatch<ActivityResponse>(`/activities/${a.id}`, body);
    },
    onSuccess: (res) => {
      queryClient.setQueryData(["activities", "one", a.id], { data: res.data });
      void queryClient.invalidateQueries({ queryKey: ["activities"] });
      void queryClient.invalidateQueries({ queryKey: ["stats"] });
      void queryClient.invalidateQueries({ queryKey: ["form"] });
      const shifted = res.meta?.notices?.some((n) => n.code === "local_time_shifted");
      flash(t(shifted ? "edit.savedShifted" : "edit.saved"));
      onClose();
    },
    onError: (e) => {
      if (e instanceof ApiError && e.code === "local_time_ambiguous") {
        setTimeOpen(true);
        setForm((f) => ({ ...f, ambiguous: true, occurrence: undefined }));
      }
      setError(problemText(e));
    },
  });

  const options = subSportOptions(catalog, form.sportClass);
  const busy = save.isPending;

  return (
    <Dialog
      title={t("edit.title")}
      onClose={onClose}
      busy={busy}
      footer={
        <>
          {problem && <span className="mr-auto self-center text-sm text-danger">{t(`edit.problem.${problem}` as TextKey)}</span>}
          <button type="button" className={secondaryButton} onClick={onClose} disabled={busy}>
            {t("edit.cancel")}
          </button>
          <button type="button" className={primaryButton} disabled={!patch || busy} onClick={() => save.mutate()}>
            {busy ? t("edit.saving") : t("edit.save")}
          </button>
        </>
      }
    >
      <form
        className="grid gap-3"
        onSubmit={(e) => {
          e.preventDefault();
          if (patch && !busy) save.mutate();
        }}
      >
        <Field label={t("edit.field.title")}>
          <input className={input} value={form.title} disabled={busy} onChange={(e) => set({ title: e.target.value })} />
        </Field>
        <div className="grid gap-3 sm:grid-cols-2">
          <Field plain label={t("edit.field.sportClass")} hint={form.sportClass !== a.sport_class ? t("edit.sportClassHint") : undefined}>
            <IconSelect
              label={t("edit.field.sportClass")}
              value={form.sportClass}
              disabled={busy}
              options={SPORT_CLASSES.map((c) => ({ value: c, label: t(`sport.${c}` as TextKey), icon: <SportIcon sport={c} decorative /> }))}
              onChange={(c) => {
                setForm((f) => withSportClass(f, c as SportClass, catalog));
                setError(null);
              }}
            />
          </Field>
          <Field plain label={t("edit.field.subSport")}>
            <IconSelect
              label={t("edit.field.subSport")}
              value={form.subSport}
              disabled={busy || form.sportClass === "unknown"}
              onChange={(v) => set({ subSport: v })}
              options={[
                { value: "", label: t("edit.subSport.none") },
                ...options.map((o) => ({ value: o.key, label: subSportEntryLabel(o), icon: <SubSportIcon sport={form.sportClass} entry={o} decorative /> })),
                { value: NEW_SUB_SPORT, label: t("edit.subSport.new") },
              ]}
            />
            {form.subSport === NEW_SUB_SPORT && (
              <input
                className={input}
                value={form.newSubSport}
                placeholder={t("edit.subSport.newName")}
                aria-label={t("edit.subSport.newName")}
                maxLength={40}
                disabled={busy}
                autoFocus
                onChange={(e) => set({ newSubSport: e.target.value })}
              />
            )}
          </Field>
        </div>
        <Field label={t("edit.field.location")}>
          <input className={input} value={form.location} disabled={busy} onChange={(e) => set({ location: e.target.value })} />
        </Field>
        <Field label={t("edit.field.description")}>
          <textarea
            className={`${input} min-h-24`}
            rows={4}
            value={form.description}
            disabled={busy}
            onChange={(e) => set({ description: e.target.value })}
          />
        </Field>

        <section className="rounded-lg border border-line">
          <button
            type="button"
            className="flex w-full items-center justify-between px-3 py-2 text-left text-sm font-medium"
            aria-expanded={timeOpen}
            onClick={() => setTimeOpen((o) => !o)}
          >
            {t("edit.time.title")}
            <span className="text-muted" aria-hidden="true">
              {timeOpen ? "−" : "+"}
            </span>
          </button>
          {timeOpen && (
            <div className="grid gap-3 border-t border-line px-3 py-3">
              <p className="text-xs text-muted">{t("edit.time.hint")}</p>
              <div className="grid gap-3 sm:grid-cols-2">
                <Field label={t("edit.time.start")}>
                  <input
                    type="datetime-local"
                    step={1}
                    className={input}
                    value={form.startLocal}
                    disabled={busy}
                    onChange={(e) => set({ startLocal: e.target.value, ambiguous: false, occurrence: undefined })}
                  />
                </Field>
                <Field label={t("edit.time.duration")}>
                  <input
                    className={`${input} num`}
                    placeholder="1:30:00"
                    value={form.durationText}
                    disabled={busy}
                    onChange={(e) => set({ durationText: e.target.value })}
                  />
                </Field>
              </div>
              {form.ambiguous && (
                <Field label={t("import.time.occurrence")}>
                  <select
                    className={input}
                    value={form.occurrence ?? ""}
                    disabled={busy}
                    onChange={(e) => set({ occurrence: (e.target.value || undefined) as EditForm["occurrence"] })}
                  >
                    <option value="" />
                    <option value="first">{t("import.time.first")}</option>
                    <option value="second">{t("import.time.second")}</option>
                  </select>
                </Field>
              )}
              {corrected && (
                <p className="text-xs text-muted">
                  {t("edit.time.original", {
                    start: dateTime(a.start_time_original ?? a.start_time, timeZone, true),
                    duration: duration(a.total_time_original_ms ?? a.total_time_ms),
                  })}
                </p>
              )}
            </div>
          )}
        </section>
        {error && <p className="text-sm text-danger">{error}</p>}
        <button type="submit" hidden />
      </form>
    </Dialog>
  );
}
