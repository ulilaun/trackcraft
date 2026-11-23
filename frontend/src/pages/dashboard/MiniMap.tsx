import { useEffect, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import L from "leaflet";
import "leaflet/dist/leaflet.css";
import { apiGet, type TrackResponse } from "../../api/client";

/** Punkte der Vorschau; reicht für die Form der Strecke und hält die Startseite leicht. */
export const MINI_POINTS = 200;

const START = "#16a34a";
const END = "#dc2626";

function accent(): string {
  return getComputedStyle(document.documentElement).getPropertyValue("--tc-accent").trim() || "#38bdf8";
}

/**
 * Feste kleine Karte einer Aktivität im Dashboard-Feed: ohne Zoom und Verschieben, Klick geht an die Kachel.
 * Lädt Spur und Kacheln erst, wenn sie ins Bild kommt (ohne IntersectionObserver, etwa in jsdom, gar nicht).
 */
export function MiniMap({ id, tileUrl, attribution }: { id: string; tileUrl: string; attribution: string }) {
  const box = useRef<HTMLDivElement>(null);
  const [visible, setVisible] = useState(false);

  useEffect(() => {
    const el = box.current;
    if (!el || typeof IntersectionObserver === "undefined") return;
    const io = new IntersectionObserver(
      (entries) => {
        if (entries.some((e) => e.isIntersecting)) {
          setVisible(true);
          io.disconnect();
        }
      },
      { rootMargin: "200px" },
    );
    io.observe(el);
    return () => io.disconnect();
  }, []);

  const track = useQuery({
    queryKey: ["activities", id, "track", "mini"],
    queryFn: () => apiGet<TrackResponse>(`/activities/${id}/track?max_points=${MINI_POINTS}`),
    enabled: visible,
    staleTime: Infinity,
  });

  useEffect(() => {
    const el = box.current;
    const sections = track.data?.data.sections;
    if (!el || !sections) return;
    const lines = sections
      .map((s) => s.lat.flatMap((lat, i) => (lat != null && s.lon[i] != null ? [L.latLng(lat, s.lon[i])] : [])))
      .filter((l) => l.length > 1);
    const first = lines[0]?.[0];
    const lastLine = lines[lines.length - 1];
    const last = lastLine?.[lastLine.length - 1];
    if (!first || !last) return;
    const map = L.map(el, {
      zoomControl: false,
      dragging: false,
      scrollWheelZoom: false,
      doubleClickZoom: false,
      boxZoom: false,
      keyboard: false,
      touchZoom: false,
      attributionControl: true,
    });
    map.attributionControl.setPrefix(false);
    L.tileLayer(tileUrl, { maxZoom: 19, attribution }).addTo(map);
    const line = L.polyline(lines, { color: accent(), weight: 3.5, interactive: false }).addTo(map);
    const dot = { radius: 4, color: "#fff", weight: 1.5, fillOpacity: 1, interactive: false };
    L.circleMarker(first, { ...dot, fillColor: START }).addTo(map);
    L.circleMarker(last, { ...dot, fillColor: END }).addTo(map);
    map.fitBounds(line.getBounds(), { padding: [14, 14] });
    return () => {
      map.remove();
    };
  }, [track.data, tileUrl, attribution]);

  return <div ref={box} className="tc-mini-map absolute inset-0" data-testid="mini-map" />;
}
