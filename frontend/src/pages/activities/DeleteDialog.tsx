import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { apiDelete, apiGet, type Activity, type DeletionPreviewResponse } from "../../api/client";
import { dangerButton, Dialog, secondaryButton } from "../../components/Dialog";
import { Problem } from "../../components/Problem";
import { t } from "../../i18n";
import { dateTime } from "../../utils/format";

/** Löschen mit Vorschau der Folgen (RFC-0017 „Löschen und Datenstände“, OpenAPI deletion-preview). */
export function DeleteDialog({
  activity: a,
  timeZone,
  onClose,
  onDeleted,
}: {
  activity: Activity;
  timeZone?: string;
  onClose: () => void;
  onDeleted: () => void;
}) {
  const queryClient = useQueryClient();
  const preview = useQuery({
    queryKey: ["activities", "deletion-preview", a.id],
    queryFn: () => apiGet<DeletionPreviewResponse>(`/activities/${a.id}/deletion-preview`),
    gcTime: 0,
  });
  const remove = useMutation({
    mutationFn: () => apiDelete(`/activities/${a.id}`),
    onSuccess: () => {
      queryClient.removeQueries({ queryKey: ["activities", "one", a.id] });
      queryClient.removeQueries({ queryKey: ["activities", "track", a.id] });
      void queryClient.invalidateQueries({ queryKey: ["activities"] });
      void queryClient.invalidateQueries({ queryKey: ["stats"] });
      onDeleted();
    },
  });
  const p = preview.data?.data;
  const busy = remove.isPending;

  return (
    <Dialog
      title={t("delete.title")}
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
          <strong>{a.title}</strong> · {dateTime(a.start_time, timeZone, true)}
        </p>
        {preview.error ? (
          <Problem error={preview.error} />
        ) : !p ? (
          <p className="text-muted">{t("state.loading")}</p>
        ) : (
          <ul className="grid list-disc gap-1 pl-5">
            <li>{t("delete.points")}</li>
            {p.effort_count > 0 && <li>{t("delete.efforts", { n: p.effort_count })}</li>}
            {p.affected_segment_count > 0 && <li>{t("delete.segments", { n: p.affected_segment_count })}</li>}
            <li>{t(p.original_file_removed ? "delete.fileRemoved" : "delete.fileKept")}</li>
            <li>{t("delete.backups")}</li>
          </ul>
        )}
        {remove.error && <Problem error={remove.error} />}
      </div>
    </Dialog>
  );
}
