import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  apiDelete,
  apiGet,
  apiPatch,
  apiPost,
  type DeletionPreviewResponse,
  type NameTransferResponse,
  type Segment,
  type SegmentCategory,
  type SegmentKind,
  type SegmentResponse,
} from "../../api/client";
import { CategoryIcon, categoriesFor, categoryText, SegmentKindIcon } from "../../components/CategoryIcon";
import { IconSelect } from "../../components/IconSelect";
import { dangerButton, Dialog, primaryButton, secondaryButton } from "../../components/Dialog";
import { useFlash } from "../../components/Flash";
import { Problem } from "../../components/Problem";
import { t, type TextKey } from "../../i18n";
import { initialForm, patchOf, type SegmentForm } from "./segmentModel";

const input = "w-full rounded-lg border border-line bg-card-2 px-2 py-1.5 text-sm text-text disabled:opacity-60";

/** Bearbeiten nach RFC-0016: Name, Art, Kategorie, Ort und Beschreibung; Referenz, Richtung und Sportklasse sind fest. */
export function SegmentEditDialog({ segment: s, onClose }: { segment: Segment; onClose: () => void }) {
  const [form, setForm] = useState<SegmentForm>(() => initialForm(s));
  const queryClient = useQueryClient();
  const flash = useFlash();
  const patch = patchOf(s, form);
  const nameEmpty = !form.name.trim();

  const save = useMutation({
    mutationFn: () => apiPatch<SegmentResponse>(`/segments/${s.id}`, patch),
    onSuccess: (res) => {
      queryClient.setQueryData(["segments", "one", s.id], res);
      void queryClient.invalidateQueries({ queryKey: ["segments"] });
      void queryClient.invalidateQueries({ queryKey: ["activities"] });
      flash(t(patch?.kind ? "segmentEdit.savedKind" : "segmentEdit.saved"));
      onClose();
    },
  });
  const busy = save.isPending;
  const set = (p: Partial<SegmentForm>) => {
    setForm((f) => ({ ...f, ...p }));
    save.reset();
  };

  return (
    <Dialog
      title={t(`segmentEdit.title.${s.kind}` as TextKey)}
      onClose={onClose}
      busy={busy}
      footer={
        <>
          {nameEmpty && <span className="mr-auto self-center text-sm text-danger">{t("segmentEdit.problem.name")}</span>}
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
        <label className="grid gap-1">
          <span className="text-xs font-medium text-muted">{t("segmentEdit.name")}</span>
          <input className={input} value={form.name} disabled={busy} onChange={(e) => set({ name: e.target.value })} />
        </label>
        {/* eigene Auswahllisten mit Symbolen; nicht in <label>, sonst löst ein Klick in die Liste den Knopf erneut aus */}
        <div className="grid gap-1">
          <span className="text-xs font-medium text-muted">{t("segmentEdit.kind")}</span>
          <IconSelect
            label={t("segmentEdit.kind")}
            value={form.kind}
            disabled={busy}
            onChange={(k) => set({ kind: k as SegmentKind })}
            options={(["segment", "route"] as SegmentKind[]).map((k) => ({
              value: k,
              label: t(`segment.kind.${k}` as TextKey),
              icon: <SegmentKindIcon kind={k} className="text-muted" />,
            }))}
          />
          {form.kind !== s.kind && <span className="text-xs text-muted">{t("segmentEdit.kindHint")}</span>}
        </div>
        <div className="grid gap-1">
          <span className="text-xs font-medium text-muted">{t("segmentEdit.category")}</span>
          <IconSelect
            label={t("segmentEdit.category")}
            value={form.category !== "auto" && !categoriesFor(form.kind).includes(form.category) ? "auto" : form.category}
            disabled={busy}
            onChange={(c) => set({ category: c as SegmentCategory | "auto" })}
            options={[
              {
                value: "auto",
                label: form.kind === s.kind && s.category_auto ? t("category.auto", { category: categoryText(s.category_auto) }) : t("category.autoNone"),
                icon: form.kind === s.kind && s.category_auto ? <CategoryIcon category={s.category_auto} decorative /> : undefined,
              },
              ...categoriesFor(form.kind).map((c) => ({ value: c, label: categoryText(c), icon: <CategoryIcon category={c} decorative /> })),
            ]}
          />
        </div>
        <label className="grid gap-1">
          <span className="text-xs font-medium text-muted">{t("segmentEdit.location")}</span>
          <input
            className={input}
            value={form.location}
            maxLength={120}
            placeholder={t("segmentEdit.locationPlaceholder")}
            disabled={busy}
            onChange={(e) => set({ location: e.target.value })}
          />
        </label>
        <label className="grid gap-1">
          <span className="text-xs font-medium text-muted">{t("segmentEdit.description")}</span>
          <textarea className={`${input} min-h-24`} rows={4} value={form.description} disabled={busy} onChange={(e) => set({ description: e.target.value })} />
        </label>
        <p className="text-xs text-muted">{t("segmentEdit.fixed")}</p>
        {save.error && <Problem error={save.error} />}
        <button type="submit" hidden />
      </form>
    </Dialog>
  );
}

/** Löschen mit Vorschau der Folgen (RFC-0016 „Lebenszyklus“): Versuche und Bestzeiten entfallen, Aktivitäten bleiben. */
export function SegmentDeleteDialog({ segment: s, onClose, onDeleted }: { segment: Segment; onClose: () => void; onDeleted: () => void }) {
  const queryClient = useQueryClient();
  const preview = useQuery({
    queryKey: ["segments", "deletion-preview", s.id],
    queryFn: () => apiGet<DeletionPreviewResponse>(`/segments/${s.id}/deletion-preview`),
    gcTime: 0,
  });
  const remove = useMutation({
    mutationFn: () => apiDelete(`/segments/${s.id}`),
    onSuccess: () => {
      for (const part of ["one", "track", "attempts"]) queryClient.removeQueries({ queryKey: ["segments", part, s.id] });
      void queryClient.invalidateQueries({ queryKey: ["segments"] });
      void queryClient.invalidateQueries({ queryKey: ["activities"] });
      onDeleted();
    },
  });
  const p = preview.data?.data;
  const busy = remove.isPending;

  return (
    <Dialog
      title={t(`segmentDelete.title.${s.kind}` as TextKey)}
      onClose={onClose}
      busy={busy}
      footer={
        <>
          <button type="button" className={secondaryButton} onClick={onClose} disabled={busy} data-autofocus>
            {t("edit.cancel")}
          </button>
          <button type="button" className={dangerButton} disabled={!p || busy} onClick={() => remove.mutate()}>
            {busy ? t("delete.deleting") : t("delete.confirm")}
          </button>
        </>
      }
    >
      <div className="grid gap-3 text-sm">
        <p>
          <strong>{s.name}</strong> · {t(`segment.kind.${s.kind}` as TextKey)}
        </p>
        {preview.error ? (
          <Problem error={preview.error} />
        ) : !p ? (
          <p className="text-muted">{t("state.loading")}</p>
        ) : (
          <ul className="grid list-disc gap-1 pl-5">
            <li>{t("segmentDelete.reference")}</li>
            {p.effort_count > 0 && <li>{t("segmentDelete.efforts", { n: p.effort_count })}</li>}
            <li>{t("segmentDelete.activities")}</li>
            <li>{t("segmentDelete.backups")}</li>
          </ul>
        )}
        {remove.error && <Problem error={remove.error} />}
      </div>
    </Dialog>
  );
}

/** Name und Ort einer Strecke auf alle Aktivitäten mit gültiger Durchfahrt übernehmen (#23), mit Vorschau der Anzahl. */
export function NameTransferDialog({ segment: s, onClose }: { segment: Segment; onClose: () => void }) {
  const queryClient = useQueryClient();
  const flash = useFlash();
  const preview = useQuery({
    queryKey: ["segments", "name-transfer", s.id],
    queryFn: () => apiGet<NameTransferResponse>(`/segments/${s.id}/name-transfer`),
    gcTime: 0,
  });
  const apply = useMutation({
    mutationFn: () => apiPost<NameTransferResponse>(`/segments/${s.id}/name-transfer`, {}),
    onSuccess: (r) => {
      void queryClient.invalidateQueries({ queryKey: ["activities"] });
      void queryClient.invalidateQueries({ queryKey: ["segments", "attempts", s.id] });
      flash(t("nameTransfer.done", { n: r.data.changed_count }));
      onClose();
    },
  });
  const p = preview.data?.data;
  const busy = apply.isPending;

  return (
    <Dialog
      title={t("nameTransfer.title")}
      onClose={onClose}
      busy={busy}
      footer={
        <>
          <button type="button" className={secondaryButton} onClick={onClose} disabled={busy} data-autofocus>
            {t("edit.cancel")}
          </button>
          <button type="button" className={primaryButton} disabled={!p || p.changed_count === 0 || busy} onClick={() => apply.mutate()}>
            {busy ? t("nameTransfer.busy") : t("nameTransfer.confirm")}
          </button>
        </>
      }
    >
      <div className="grid gap-3 text-sm">
        {preview.error ? (
          <Problem error={preview.error} />
        ) : !p ? (
          <p className="text-muted">{t("state.loading")}</p>
        ) : (
          <>
            <p>{p.location ? t("nameTransfer.whatLocation", { name: p.name, location: p.location }) : t("nameTransfer.what", { name: p.name })}</p>
            <p className="text-muted">
              {p.activity_count === 0
                ? t("nameTransfer.empty")
                : p.changed_count === 0
                  ? t("nameTransfer.none")
                  : t("nameTransfer.count", { changed: p.changed_count, total: p.activity_count })}
            </p>
          </>
        )}
        {apply.error && <Problem error={apply.error} />}
      </div>
    </Dialog>
  );
}
