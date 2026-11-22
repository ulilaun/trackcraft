// Geteilter Knopf je Sportklasse: Name wählt die ganze Klasse, rechts vom Trenner der
// gewählte Untertyp mit ▾; der Pfeil öffnet das Menü mit den Untertypen.
import type { SportClass, SubSportEntry } from "../api/client";
import { t } from "../i18n";
import { subSportEntryLabel } from "../utils/sports";
import { SportIcon, SubSportIcon } from "./SportIcon";
import { SplitPill } from "./SplitPill";

export function SportFilterPill({
  sport,
  label,
  on,
  sub,
  subs,
  onPick,
  onPickSub,
}: {
  sport: SportClass;
  label: string;
  on: boolean;
  /** gewählter Untertyp dieser Klasse */
  sub: string | null;
  subs: SubSportEntry[];
  onPick: () => void;
  onPickSub: (sub: string | null) => void;
}) {
  const subEntry = on && sub ? subs.find((e) => e.key === sub) : undefined;
  // Symbol auf dem gefüllten Knopf weiß
  const value = subEntry ? (
    <>
      <SubSportIcon sport={sport} entry={subEntry} plain={on} decorative />
      {subSportEntryLabel(subEntry)}
    </>
  ) : on && sub ? (
    sub
  ) : undefined;
  return (
    <SplitPill
      label={label}
      icon={<SportIcon sport={sport} plain={on} decorative />}
      on={on}
      onPick={onPick}
      value={value}
      options={
        subs.length === 0
          ? []
          : [
              { key: null, label: t("list.subAll", { sport: label }), icon: <SportIcon sport={sport} decorative /> },
              ...subs.map((e) => ({ key: e.key, label: subSportEntryLabel(e), icon: <SubSportIcon sport={sport} entry={e} decorative /> })),
            ]
      }
      selected={on ? sub : undefined}
      onSelect={onPickSub}
      menuLabel={t("list.subMenu", { sport: label })}
    />
  );
}
