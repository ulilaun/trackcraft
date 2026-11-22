import { attachPointerDrag, preventContextMenu } from "./pointerDrag";

/** jsdom kennt PointerEvent nicht immer: MouseEvent mit pointerId/pointerType nachbilden. */
function pointer(type: string, init: { pointerId?: number; pointerType?: string; button?: number; clientX?: number } = {}) {
  const ev = new MouseEvent(type, { bubbles: true, cancelable: true, button: init.button ?? 0, clientX: init.clientX ?? 0 });
  Object.assign(ev, { pointerId: init.pointerId ?? 1, pointerType: init.pointerType ?? "mouse" });
  return ev as unknown as PointerEvent;
}

describe("Ziehen per Pointer", () => {
  it("zieht mit dem Finger: Start, Bewegungen, Ende; Karte darunter bekommt nichts", () => {
    const parent = document.createElement("div");
    const el = document.createElement("div");
    parent.appendChild(el);
    const seen: string[] = [];
    parent.addEventListener("pointerdown", () => seen.push("karte"));
    parent.addEventListener("pointermove", () => seen.push("karte"));
    const moves: number[] = [];
    const h = { onStart: vi.fn(), onMove: (ev: PointerEvent) => moves.push(ev.clientX), onEnd: vi.fn() };
    const detach = attachPointerDrag(el, h);

    const down = pointer("pointerdown", { pointerType: "touch", pointerId: 7 });
    el.dispatchEvent(down);
    expect(down.defaultPrevented).toBe(true);
    expect(h.onStart).toHaveBeenCalledTimes(1);
    el.dispatchEvent(pointer("pointermove", { pointerType: "touch", pointerId: 7, clientX: 10 }));
    el.dispatchEvent(pointer("pointermove", { pointerType: "touch", pointerId: 7, clientX: 20 }));
    // anderer Finger zählt nicht
    el.dispatchEvent(pointer("pointermove", { pointerType: "touch", pointerId: 8, clientX: 99 }));
    el.dispatchEvent(pointer("pointerup", { pointerType: "touch", pointerId: 7 }));
    expect(moves).toEqual([10, 20]);
    expect(h.onEnd).toHaveBeenCalledTimes(1);
    expect(seen.filter((s) => s === "karte").length).toBe(1); // nur die Bewegung des fremden Fingers
    // nach dem Ende keine Bewegung mehr
    el.dispatchEvent(pointer("pointermove", { pointerType: "touch", pointerId: 7, clientX: 30 }));
    expect(moves).toEqual([10, 20]);

    // Abbruch durch das System beendet ebenfalls
    el.dispatchEvent(pointer("pointerdown", { pointerType: "touch", pointerId: 9 }));
    el.dispatchEvent(pointer("pointercancel", { pointerType: "touch", pointerId: 9 }));
    expect(h.onEnd).toHaveBeenCalledTimes(2);
    detach();
    el.dispatchEvent(pointer("pointerdown", { pointerType: "touch", pointerId: 10 }));
    expect(h.onStart).toHaveBeenCalledTimes(2);
  });

  it("langes Drücken öffnet kein Kontextmenü; rechte Maustaste zieht nicht", () => {
    const el = document.createElement("div");
    const h = { onStart: vi.fn(), onMove: vi.fn(), onEnd: vi.fn() };
    attachPointerDrag(el, h);
    const menu = new MouseEvent("contextmenu", { bubbles: true, cancelable: true });
    el.dispatchEvent(menu);
    expect(menu.defaultPrevented).toBe(true);
    el.dispatchEvent(pointer("pointerdown", { button: 2 }));
    expect(h.onStart).not.toHaveBeenCalled();
  });
});

describe("Kontextmenü im Bedienbereich", () => {
  it("sperrt das Menü auf Links, Bildern und Flächen, nicht in Eingabefeldern", () => {
    const area = document.createElement("div");
    area.innerHTML = `<a href="#">+</a><img alt=""><div><span>x</span></div><input><textarea></textarea>`;
    area.addEventListener("contextmenu", preventContextMenu, true);
    const open = (sel: string) => {
      const ev = new MouseEvent("contextmenu", { bubbles: true, cancelable: true });
      area.querySelector(sel)!.dispatchEvent(ev);
      return ev.defaultPrevented;
    };
    expect(open("a")).toBe(true);
    expect(open("img")).toBe(true);
    expect(open("span")).toBe(true);
    expect(open("input")).toBe(false);
    expect(open("textarea")).toBe(false);
  });
});
