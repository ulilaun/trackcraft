import { useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { apiPatch, type PlannedUnit, type PlannedUnitResponse } from "../../api/client";
import { Dialog, primaryButton, secondaryButton } from "../../components/Dialog";
import { useFlash } from "../../components/Flash";
import { Problem } from "../../components/Problem";
import { t } from "../../i18n";
import { germanDate } from "../seasons/seasonModel";

type Move = { unit: PlannedUnit; date: string };

/** Erledigte Einheiten hängen an einer Aktivität und bleiben an ihrem Tag. */
export function canMove(u: PlannedUnit): boolean {
  return u.status !== "done";
}

/** Bei einer Serie mit offenen Einheiten danach fragt das Verschieben „Nur diese / Diese und folgende“. */
export function asksScope(u: PlannedUnit): boolean {
  return !!u.series_id && (u.series_following ?? 0) > 0;
}

/**
 * Verschieben per Drag & Drop (#38, Desktop): `move(unit, date)` speichert sofort oder fragt bei Serien nach dem
 * Geltungsbereich; „Diese und folgende“ verschiebt die offenen Einheiten der Serie um dieselbe Zahl Tage.
 * `dialog` gehört in die Seite.
 */
export function useMoveUnit() {
  const [ask, setAsk] = useState<Move | null>(null);
  const queryClient = useQueryClient();
  const flash = useFlash();
  const save = useMutation({
    mutationFn: ({ unit, date, following }: Move & { following: boolean }) =>
      apiPatch<PlannedUnitResponse>(`/plan/units/${unit.id}${following ? "?scope=following" : ""}`, { date }),
    onSuccess: (_, { date }) => {
      void queryClient.invalidateQueries({ queryKey: ["plan"] });
      void queryClient.invalidateQueries({ queryKey: ["form"] });
      flash(t("plan.move.done", { date: germanDate(date) }));
      setAsk(null);
    },
  });

  const move = (unit: PlannedUnit, date: string) => {
    if (!canMove(unit) || date === unit.date || save.isPending) return;
    if (asksScope(unit)) setAsk({ unit, date });
    else save.mutate({ unit, date, following: false });
  };

  const dialog = ask && (
    <Dialog
      title={t("plan.move.title")}
      onClose={() => setAsk(null)}
      busy={save.isPending}
      footer={
        <>
          <span className="flex-1" />
          <button type="button" className={secondaryButton} disabled={save.isPending} onClick={() => setAsk(null)}>
            {t("edit.cancel")}
          </button>
          <button type="button" className={secondaryButton} disabled={save.isPending} onClick={() => save.mutate({ ...ask, following: false })}>
            {t("plan.series.this")}
          </button>
          <button type="button" className={primaryButton} disabled={save.isPending} onClick={() => save.mutate({ ...ask, following: true })}>
            {t("plan.series.following")}
          </button>
        </>
      }
    >
      <div className="grid gap-2 text-sm">
        <p>{t("plan.move.question", { title: ask.unit.title, from: germanDate(ask.unit.date), to: germanDate(ask.date) })}</p>
        <p className="text-xs text-muted">
          {ask.unit.series_following === 1 ? t("plan.series.infoOne") : t("plan.series.info", { n: ask.unit.series_following ?? 0 })}.{" "}
          {t("plan.move.followingHint")}
        </p>
        {save.error && <Problem error={save.error} />}
      </div>
    </Dialog>
  );

  // Fehler ohne Rückfrage (Einzelverschiebung) als Hinweis unter dem Kalender
  const error = ask ? null : save.error;
  return { move, dialog, error, busy: save.isPending };
}
