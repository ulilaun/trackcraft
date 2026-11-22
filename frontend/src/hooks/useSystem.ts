import { useQuery } from "@tanstack/react-query";
import { apiGet, type HealthResponse, type VersionResponse } from "../api/client";

export function useVersion() {
  return useQuery({ queryKey: ["version"], queryFn: () => apiGet<VersionResponse>("/version"), staleTime: 60_000 });
}

export function useHealth() {
  // /health liefert bei `fail` 503 mit gleichem Aufbau; die Seite zeigt dann den Fehler.
  return useQuery({ queryKey: ["health"], queryFn: () => apiGet<HealthResponse>("/health"), refetchInterval: 30_000 });
}
