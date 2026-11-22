import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { apiGet, apiPatch, type PlaceSettingsResponse } from "../../api/client";
import { Card } from "../../components/Card";
import { Problem } from "../../components/Problem";
import { t } from "../../i18n";

/** Einstellungen → Orte (#22): ob der Ortsvorschlag ohne eigenen Treffer OpenStreetMap fragen darf. */
export function PlaceSettings() {
  const queryClient = useQueryClient();
  const settings = useQuery({ queryKey: ["places", "settings"], queryFn: () => apiGet<PlaceSettingsResponse>("/places/settings") });
  const save = useMutation({
    mutationFn: (osm_enabled: boolean) => apiPatch<PlaceSettingsResponse>("/places/settings", { osm_enabled }),
    onSuccess: (r) => queryClient.setQueryData(["places", "settings"], r),
  });
  const s = settings.data?.data;

  return (
    <Card>
      <div className="grid gap-3 text-sm">
        <p className="text-muted">{t("places.intro")}</p>
        {settings.error ? (
          <Problem error={settings.error} />
        ) : !s ? (
          <p className="text-muted">{t("state.loading")}</p>
        ) : (
          <label className="flex items-start gap-2">
            <input type="checkbox" className="mt-0.5" checked={s.osm_enabled} disabled={save.isPending} onChange={(e) => save.mutate(e.target.checked)} />
            <span className="grid gap-0.5">
              <span>{t("places.osm")}</span>
              <span className="text-xs text-muted">{t("places.osmHint")}</span>
            </span>
          </label>
        )}
        {save.error && <Problem error={save.error} />}
      </div>
    </Card>
  );
}
