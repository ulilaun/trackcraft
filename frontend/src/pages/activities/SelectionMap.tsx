import { useEffect, useLayoutEffect, useRef, useState } from "react";
import L from "leaflet";
import "leaflet/dist/leaflet.css";
import { t } from "../../i18n";
import { follow, type Range, type SelPoint } from "./createModel";
import { attachPointerDrag } from "./pointerDrag";

interface Props {
  points: SelPoint[];
  range: Range;
  onChange: (r: Range) => void;
  /** Punkt unter dem Profil-Cursor. */
  hoverIndex: number | null;
  tileUrl: string;
  attribution: string;
}

function token(name: string, fallback: string): string {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim() || fallback;
}

/**
 * Marker nur über CSS-Klassen (CSP style-src 'self'), Stil in index.css. Trefferfläche 40 px für den Finger,
 * sichtbar bleibt der kleine Punkt.
 */
const icon = (cls: string) => L.divIcon({ className: `tc-mk ${cls}`, iconSize: [40, 40], iconAnchor: [20, 20] });

/**
 * Karte des Erstellen-Dialogs (RFC-0016): ganze Aufzeichnung abgeblendet, Auswahl hervorgehoben, Start und Ziel
 * als ziehbare Marker, die auf der Linie einrasten.
 */
export function SelectionMap({ points, range, onChange, hoverIndex, tileUrl, attribution }: Props) {
  const el = useRef<HTMLDivElement>(null);
  const layers = useRef<{ map: L.Map; full: L.Polyline; sel: L.Polyline; s: L.Marker; e: L.Marker; cursor: L.CircleMarker } | null>(null);
  const current = useRef({ range, onChange });
  const [hint, setHint] = useState(false);
  useLayoutEffect(() => {
    current.current = { range, onChange };
  });

  useEffect(() => {
    if (!el.current || points.length < 2) return;
    const ll = points.map((p) => L.latLng(p.lat, p.lon));
    const map = L.map(el.current, { zoomControl: true, attributionControl: true });
    L.tileLayer(tileUrl, { maxZoom: 19, attribution }).addTo(map);
    const full = L.polyline(ll, { color: token("--tc-muted", "#64748b"), weight: 4, opacity: 0.6 }).addTo(map);
    const sel = L.polyline([], { color: token("--tc-accent", "#38bdf8"), weight: 6 }).addTo(map);
    const s = L.marker(ll[0]!, { icon: icon("tc-mk-start"), keyboard: false, zIndexOffset: 1000, bubblingMouseEvents: false }).addTo(map);
    const e = L.marker(ll[ll.length - 1]!, { icon: icon("tc-mk-end"), keyboard: false, zIndexOffset: 1000, bubblingMouseEvents: false }).addTo(map);
    const cursor = L.circleMarker(ll[0]!, { radius: 6, color: "#fff", weight: 2, fillColor: "#111", opacity: 0, fillOpacity: 0 }).addTo(map);
    // Eigenes Ziehen statt Leaflet-`draggable`: Leaflet setzt den Marker sonst an die Maus und die Auswahl ihn an den
    // Linienpunkt zurück, er zappelt. So steht er immer auf der Linie und folgt ihr (`follow`, ohne Sprung).
    // Pointer-Ereignisse statt Leaflets Mausereignissen, damit auch der Finger zieht.
    let dragging: "s" | "e" | null = null;
    const moveTo = (ev: PointerEvent) => {
      if (!dragging) return;
      const ll = map.mouseEventToLatLng(ev);
      const q = { lat: ll.lat, lon: ll.lng };
      const [rs, re] = current.current.range;
      const i = dragging === "s" ? follow(points, q, 0, re - 1, rs) : follow(points, q, rs + 1, points.length - 1, re);
      if (i === (dragging === "s" ? rs : re)) return;
      const next: Range = dragging === "s" ? [i, re] : [rs, i];
      // Bis zum nächsten Zeichnen gilt schon die neue Lage, damit schnelle Bewegungen davon aus weiter folgen.
      current.current.range = next;
      current.current.onChange(next);
    };
    // Finger: Karte nur mit zwei Fingern verschieben/zoomen (Leaflets touchZoom), ein Finger zieht nur die Marker
    // (Uli 16.11.2026). Maus und Stift verschieben wie gewohnt. Entschieden wird je Druck, vor Leaflets
    // eigenem `mousedown`/`touchstart`, darum in der Capture-Phase am Container.
    const box = map.getContainer();
    box.classList.add("tc-two-fingers");
    let touch = false;
    const decide = (ev: PointerEvent) => {
      touch = ev.pointerType === "touch";
      if (touch) map.dragging.disable();
      else if (!dragging) map.dragging.enable();
    };
    box.addEventListener("pointerdown", decide, true);
    // Ein Finger bewegt sich auf der Karte (nicht am Marker): kurz den Hinweis auf zwei Finger zeigen
    let hintTimer: ReturnType<typeof setTimeout> | undefined;
    const oneFinger = (ev: TouchEvent) => {
      if (dragging || ev.touches.length !== 1) return;
      setHint(true);
      clearTimeout(hintTimer);
      hintTimer = setTimeout(() => setHint(false), 1500);
    };
    const twoFingers = (ev: TouchEvent) => {
      if (ev.touches.length < 2) return;
      clearTimeout(hintTimer);
      setHint(false);
    };
    box.addEventListener("touchmove", oneFinger, { passive: true });
    box.addEventListener("touchstart", twoFingers, { passive: true });
    const drag = (which: "s" | "e") => ({
      onStart: () => {
        dragging = which;
        map.dragging.disable();
        box.classList.add("tc-dragging");
      },
      onMove: moveTo,
      onEnd: () => {
        dragging = null;
        if (!touch) map.dragging.enable();
        box.classList.remove("tc-dragging");
      },
    });
    const detach = [s, e].map((m, k) => {
      const node = m.getElement();
      return node ? attachPointerDrag(node, drag(k === 0 ? "s" : "e")) : () => {};
    });
    map.fitBounds(full.getBounds(), { padding: [16, 16] });
    layers.current = { map, full, sel, s, e, cursor };
    return () => {
      clearTimeout(hintTimer);
      box.removeEventListener("pointerdown", decide, true);
      box.removeEventListener("touchmove", oneFinger);
      box.removeEventListener("touchstart", twoFingers);
      detach.forEach((f) => f());
      layers.current = null;
      map.remove();
    };
  }, [points, tileUrl, attribution]);

  useEffect(() => {
    const l = layers.current;
    if (!l) return;
    const [s, e] = range;
    l.sel.setLatLngs(points.slice(s, e + 1).map((p) => L.latLng(p.lat, p.lon)));
    const ps = points[s];
    const pe = points[e];
    if (ps) l.s.setLatLng([ps.lat, ps.lon]);
    if (pe) l.e.setLatLng([pe.lat, pe.lon]);
  }, [range, points]);

  useEffect(() => {
    const l = layers.current;
    if (!l) return;
    const p = hoverIndex == null ? undefined : points[hoverIndex];
    if (p) {
      l.cursor.setLatLng([p.lat, p.lon]);
      l.cursor.setStyle({ opacity: 1, fillOpacity: 1 });
    } else l.cursor.setStyle({ opacity: 0, fillOpacity: 0 });
  }, [hoverIndex, points]);

  const fit = (what: "sel" | "all") => {
    const l = layers.current;
    if (!l) return;
    const b = (what === "sel" ? l.sel : l.full).getBounds();
    if (b.isValid()) l.map.fitBounds(b, { padding: [24, 24] });
  };
  const small = "min-h-9 rounded-lg border border-line bg-card-2 px-3 py-1.5 text-xs hover:border-accent";

  return (
    <div className="grid gap-1.5">
      <div className="relative">
        <div ref={el} className="h-[clamp(180px,38dvh,360px)] w-full overflow-hidden rounded-xl border border-line" data-testid="selection-map" />
        {hint && (
          <div className="pointer-events-none absolute inset-0 z-[1000] grid place-items-center rounded-xl bg-black/45 text-sm font-semibold text-white" role="status">
            {t("create.twoFingers")}
          </div>
        )}
      </div>
      <div className="flex flex-wrap items-center gap-2">
        <button type="button" className={small} onClick={() => fit("sel")}>
          {t("create.zoomSelection")}
        </button>
        <button type="button" className={small} onClick={() => fit("all")}>
          {t("create.zoomAll")}
        </button>
        <span className="text-xs text-muted">{t("create.mapHint")}</span>
      </div>
    </div>
  );
}
