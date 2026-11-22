/**
 * Ziehen eines Elements mit Maus, Finger oder Stift über Pointer-Ereignisse: Leaflets eigene
 * `mousedown`/`mousemove` kommen bei Touch nicht an, dann verschiebt der Finger die Karte statt des Markers.
 * Der Pointer wird am Element festgehalten (`setPointerCapture`), so kommen alle Bewegungen dort an, auch
 * wenn der Finger das Element verlässt. Langes Drücken öffnet kein Kontextmenü.
 */
export interface DragHandlers {
  onStart: () => void;
  onMove: (ev: PointerEvent) => void;
  onEnd: () => void;
}

/** Hängt das Ziehen an `el`; gibt eine Funktion zum Abmelden zurück. */
export function attachPointerDrag(el: HTMLElement, h: DragHandlers): () => void {
  let active: number | null = null;
  const down = (ev: PointerEvent) => {
    // Nur linke Maustaste bzw. Finger/Stift
    if (ev.pointerType === "mouse" && ev.button !== 0) return;
    if (active != null) return;
    active = ev.pointerId;
    ev.preventDefault();
    ev.stopPropagation();
    el.setPointerCapture?.(ev.pointerId);
    h.onStart();
  };
  const move = (ev: PointerEvent) => {
    if (ev.pointerId !== active) return;
    ev.preventDefault();
    ev.stopPropagation();
    h.onMove(ev);
  };
  const up = (ev: PointerEvent) => {
    if (ev.pointerId !== active) return;
    active = null;
    el.releasePointerCapture?.(ev.pointerId);
    h.onEnd();
  };
  const noMenu = (ev: Event) => ev.preventDefault();
  // Leaflet soll den Druck auf den Marker nicht als Beginn eines Kartenziehens sehen
  const stop = (ev: Event) => ev.stopPropagation();
  el.addEventListener("pointerdown", down);
  el.addEventListener("pointermove", move);
  el.addEventListener("pointerup", up);
  el.addEventListener("pointercancel", up);
  el.addEventListener("lostpointercapture", up);
  el.addEventListener("contextmenu", noMenu);
  el.addEventListener("mousedown", stop);
  el.addEventListener("touchstart", stop, { passive: true });
  return () => {
    el.removeEventListener("pointerdown", down);
    el.removeEventListener("pointermove", move);
    el.removeEventListener("pointerup", up);
    el.removeEventListener("pointercancel", up);
    el.removeEventListener("lostpointercapture", up);
    el.removeEventListener("contextmenu", noMenu);
    el.removeEventListener("mousedown", stop);
    el.removeEventListener("touchstart", stop);
  };
}

/**
 * Langes Drücken mit dem Finger öffnet sonst das Kontextmenü des Browsers (Bild der Kachel, Link der Zoomknöpfe,
 * Markieren). Für ganze Bedienbereiche als `onContextMenuCapture`; Eingabefelder behalten ihr Menü (Einfügen).
 */
export function preventContextMenu(ev: { target: EventTarget | null; preventDefault: () => void }): void {
  const el = ev.target as Element | null;
  if (el?.closest?.("input, textarea, [contenteditable='true']")) return;
  ev.preventDefault();
}
