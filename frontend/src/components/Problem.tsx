import { ApiError, OfflineError } from "../api/client";
import { errorText, t } from "../i18n";

/** Fehlertext zu einer fehlgeschlagenen Anfrage (RFC-0006 „Fehlercodes“). */
export function problemText(error: unknown): string {
  if (error instanceof ApiError) return errorText(error.code, error.message);
  if (error instanceof OfflineError) return t("state.offline");
  return String(error);
}

export function Problem({ error }: { error: unknown }) {
  return <p className="text-danger">{problemText(error)}</p>;
}
