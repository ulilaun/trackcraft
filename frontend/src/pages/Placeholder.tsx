import { Card } from "../components/Card";
import { t, type TextKey } from "../i18n";

/** Platzhalter bis zum Meilenstein der Seite. */
export function Placeholder({ titleKey, milestone }: { titleKey: TextKey; milestone: string }) {
  return (
    <Card title={t(titleKey)}>
      <p className="text-muted">{t("page.coming", { milestone })}</p>
    </Card>
  );
}
