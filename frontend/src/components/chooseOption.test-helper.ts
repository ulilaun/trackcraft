// Testhilfe für IconSelect: Liste öffnen und Eintrag wählen, wie ein Klick des Benutzers.
import { fireEvent, within } from "@testing-library/react";

export function chooseOption(container: HTMLElement, field: string, option: string) {
  const box = within(container).getByRole("combobox", { name: field });
  fireEvent.click(box);
  const list = within(container).getByRole("listbox", { name: field });
  fireEvent.click(within(list).getByRole("option", { name: option }));
}

/** Namen der Einträge einer IconSelect-Liste (öffnet und schließt sie wieder). */
export function optionNames(container: HTMLElement, field: string): string[] {
  const box = within(container).getByRole("combobox", { name: field });
  fireEvent.click(box);
  const names = within(within(container).getByRole("listbox", { name: field }))
    .getAllByRole("option")
    .map((o) => o.textContent ?? "");
  fireEvent.click(box);
  return names;
}
