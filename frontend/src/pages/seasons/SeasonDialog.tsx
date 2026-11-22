import { useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { apiDelete, apiPatch, apiPost, type Season, type SeasonResponse } from "../../api/client";
import { Dialog, dangerButton, primaryButton, secondaryButton } from "../../components/Dialog";
import { useFlash } from "../../components/Flash";
import { Problem } from "../../components/Problem";
import { t } from "../../i18n";
import { conflictText, emptyForm, formOf, formProblem, inputOf, type SeasonForm } from "./seasonModel";

const input = "w-full rounded-lg border border-line bg-card-2 px-2 py-1.5 text-sm text-text disabled:opacity-60";

/**
 * Saison anlegen (`season` fehlt) oder ändern mit „Löschen“ unten links; der zweite Klick bestätigt
 * (RFC-0006 „Statistik“, Uli 13.11.2026). `onDeleted` erhält die ID der gelöschten Saison.
 */
export function SeasonDialog({ season, onClose, onDeleted }: { season?: Season; onClose: () => void; onDeleted?: (id: string) => void }) {
  const [form, setForm] = useState<SeasonForm>(() => (season ? formOf(season) : emptyForm));
  const [confirm, setConfirm] = useState(false);
  const queryClient = useQueryClient();
  const flash = useFlash();
  const problem = formProblem(form);
  const body = inputOf(form, season);
  // Saisons wirken auf Statistik, Bestenliste, Liste und Dashboard
  const refresh = () => {
    void queryClient.invalidateQueries({ queryKey: ["seasons"] });
    void queryClient.invalidateQueries({ queryKey: ["stats"] });
  };
  const save = useMutation({
    mutationFn: () => (season ? apiPatch<SeasonResponse>(`/seasons/${season.id}`, body) : apiPost<SeasonResponse>("/seasons", body)),
    onSuccess: () => {
      refresh();
      flash(t(season ? "season.saved" : "season.created"));
      onClose();
    },
  });
  const remove = useMutation({
    mutationFn: () => apiDelete(`/seasons/${season!.id}`),
    onSuccess: () => {
      refresh();
      flash(t("season.deleted"));
      onDeleted?.(season!.id);
      onClose();
    },
  });
  const busy = save.isPending || remove.isPending;
  const set = (p: Partial<SeasonForm>) => {
    setForm((f) => ({ ...f, ...p }));
    save.reset();
  };
  const conflict = conflictText(save.error);
  const submit = () => {
    if (!problem && body && !busy) save.mutate();
  };
  const touched = !!(form.name || form.start_date || form.end_date);

  return (
    <Dialog
      title={t(season ? "season.edit" : "season.new")}
      onClose={onClose}
      busy={busy}
      footer={
        <>
          {season && (
            <button
              type="button"
              className={`${confirm ? dangerButton : secondaryButton} mr-auto`}
              disabled={busy}
              onClick={() => (confirm ? remove.mutate() : setConfirm(true))}
            >
              {t(confirm ? "season.confirmDelete" : "season.delete")}
            </button>
          )}
          <button type="button" className={secondaryButton} onClick={onClose} disabled={busy}>
            {t("edit.cancel")}
          </button>
          <button type="button" className={primaryButton} disabled={!!problem || !body || busy} onClick={submit}>
            {save.isPending ? t("edit.saving") : t("edit.save")}
          </button>
        </>
      }
    >
      <form
        className="grid gap-3"
        onSubmit={(e) => {
          e.preventDefault();
          submit();
        }}
      >
        <label className="grid gap-1">
          <span className="text-xs font-medium text-muted">{t("season.name")}</span>
          <input
            className={input}
            value={form.name}
            maxLength={80}
            placeholder={t("season.namePlaceholder")}
            disabled={busy}
            onChange={(e) => set({ name: e.target.value })}
          />
        </label>
        <div className="grid grid-cols-2 gap-3">
          <label className="grid gap-1">
            <span className="text-xs font-medium text-muted">{t("season.start")}</span>
            <input type="date" className={input} value={form.start_date} disabled={busy} onChange={(e) => set({ start_date: e.target.value })} />
          </label>
          <label className="grid gap-1">
            <span className="text-xs font-medium text-muted">{t("season.end")}</span>
            <input type="date" className={input} value={form.end_date} disabled={busy} onChange={(e) => set({ end_date: e.target.value })} />
          </label>
        </div>
        <p className="text-xs text-muted">{t("season.intro")}</p>
        {(touched && problem) || conflict ? (
          <p className="text-sm text-danger" role="alert">
            {conflict ?? problem}
          </p>
        ) : (
          save.error && <Problem error={save.error} />
        )}
        {remove.error && <Problem error={remove.error} />}
        <button type="submit" hidden />
      </form>
    </Dialog>
  );
}
