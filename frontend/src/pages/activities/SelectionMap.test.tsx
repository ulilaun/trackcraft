import { render } from "@testing-library/react";
import { SelectionMap } from "./SelectionMap";
import type { SelPoint } from "./createModel";

const points: SelPoint[] = Array.from({ length: 20 }, (_, i) => ({
  index: i,
  lat: 48 + i * 0.001,
  lon: 8 + i * 0.001,
  ele: null,
  t: i * 10,
  d: i * 130,
}));

function pointerDown(el: Element) {
  const ev = new MouseEvent("pointerdown", { bubbles: true, cancelable: true, button: 0 });
  Object.assign(ev, { pointerId: 1, pointerType: "mouse" });
  el.dispatchEvent(ev);
  return ev;
}

describe("SelectionMap", () => {
  // #21: Ohne Ansicht beim Hinzufügen hatten die Marker noch kein Element, das eigene Ziehen hing nirgends,
  // die Maus verschob die Karte statt des Punkts.
  it("hängt das Ziehen an Start- und Zielmarker", () => {
    const { container } = render(
      <SelectionMap points={points} range={[2, 10]} onChange={() => {}} hoverIndex={null} tileUrl="" attribution="" />,
    );
    for (const cls of [".tc-mk-start", ".tc-mk-end"]) {
      const marker = container.querySelector(cls);
      expect(marker).not.toBeNull();
      expect(pointerDown(marker!).defaultPrevented).toBe(true);
    }
  });
});
