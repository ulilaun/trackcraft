import { useEffect, useRef } from "react";
import L from "leaflet";
import "leaflet/dist/leaflet.css";
import type { TrackPoint } from "./trackModel";

interface Props {
  points: TrackPoint[];
  tileUrl: string;
  attribution: string;
  /** Index des Punkts unter dem Diagramm-Cursor. */
  hoverIndex: number | null;
  /** Hervorgehobener Abschnitt (Segmentversuch unter dem Mauszeiger), Indizes in `points`. */
  highlight?: [number, number] | null;
}

const START = "#16a34a";
const END = "#dc2626";
const CURSOR = "#f43f5e";
const HIGHLIGHT = "#f59e0b";

function accent(): string {
  return getComputedStyle(document.documentElement).getPropertyValue("--tc-accent").trim() || "#38bdf8";
}

/**
 * Leaflet-Karte mit Spur, Start (grün), Ziel (rot) und einem Punkt, der dem Diagramm-Cursor folgt
 * (RFC-0006, RFC-0020). Nur Kreis-Marker, Leaflet setzt deren Stil per SVG ohne Inline-Styles (CSP).
 */
export function ActivityMap({ points, tileUrl, attribution, hoverIndex, highlight = null }: Props) {
  const el = useRef<HTMLDivElement>(null);
  const cursor = useRef<L.CircleMarker | null>(null);
  const marked = useRef<L.Polyline | null>(null);

  useEffect(() => {
    if (!el.current) return;
    const ll = points.flatMap((p) => (p.lat != null && p.lon != null ? [L.latLng(p.lat, p.lon)] : []));
    const first = ll[0];
    const last = ll[ll.length - 1];
    if (!first || !last) return;
    const map = L.map(el.current, { zoomControl: true, attributionControl: true });
    L.tileLayer(tileUrl, { maxZoom: 19, attribution }).addTo(map);
    const line = L.polyline(ll, { color: accent(), weight: 4 }).addTo(map);
    map.fitBounds(line.getBounds(), { padding: [16, 16] });
    marked.current = L.polyline([], { color: HIGHLIGHT, weight: 6 }).addTo(map);
    const dot = { radius: 6, color: "#fff", weight: 2, fillOpacity: 1 };
    L.circleMarker(first, { ...dot, fillColor: START }).addTo(map);
    L.circleMarker(last, { ...dot, fillColor: END }).addTo(map);
    cursor.current = L.circleMarker(first, { ...dot, radius: 7, fillColor: CURSOR, opacity: 0, fillOpacity: 0 }).addTo(map);
    return () => {
      cursor.current = null;
      marked.current = null;
      map.remove();
    };
  }, [points, tileUrl, attribution]);

  useEffect(() => {
    const c = cursor.current;
    if (!c) return;
    const p = hoverIndex == null ? undefined : points[hoverIndex];
    if (p && p.lat != null && p.lon != null) {
      c.setLatLng([p.lat, p.lon]);
      c.setStyle({ opacity: 1, fillOpacity: 1 });
    } else {
      c.setStyle({ opacity: 0, fillOpacity: 0 });
    }
  }, [hoverIndex, points]);

  useEffect(() => {
    const m = marked.current;
    if (!m) return;
    const part = highlight ? points.slice(highlight[0], highlight[1] + 1) : [];
    m.setLatLngs(part.flatMap((p) => (p.lat != null && p.lon != null ? [L.latLng(p.lat, p.lon)] : [])));
    if (highlight) m.bringToFront();
  }, [highlight, points]);

  return <div ref={el} className="h-full min-h-[380px] w-full overflow-hidden rounded-xl" data-testid="activity-map" />;
}
