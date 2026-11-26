import { useQuery } from "@tanstack/react-query";
import { apiGet, type HealthResponse, type SystemResponse, type VersionResponse } from "../api/client";

export function useVersion() {
  return useQuery({ queryKey: ["version"], queryFn: () => apiGet<VersionResponse>("/version"), staleTime: 60_000 });
}

export function useHealth() {
  // /health liefert bei `fail` 503 mit gleichem Aufbau; die Seite zeigt dann den Fehler.
  return useQuery({ queryKey: ["health"], queryFn: () => apiGet<HealthResponse>("/health"), refetchInterval: 30_000 });
}

/** Betriebszustand (`GET /system`, RFC-0013): Laufzeit, Zähler, Speicher, Aufträge und Warnungen. */
export function useSystemInfo() {
  return useQuery({ queryKey: ["system"], queryFn: () => apiGet<SystemResponse>("/system"), refetchInterval: 30_000 });
}
