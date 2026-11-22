import { useQuery } from "@tanstack/react-query";
import { apiGet, type SeasonListResponse } from "../../api/client";

/** Alle Saisons, neueste zuerst, mit Vorgabesaison (RFC-0009 „Saisons“). */
export function useSeasons() {
  return useQuery({ queryKey: ["seasons"], queryFn: () => apiGet<SeasonListResponse>("/seasons") });
}
