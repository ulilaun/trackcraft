import { useQuery } from "@tanstack/react-query";
import { apiGet, type ConfigResponse } from "../api/client";
import { subSportLabel } from "../utils/sports";

/** Öffentliche Konfiguration (RFC-0021 Abschnitt 4). Der Untertyp-Katalog kommt aus der Datenbank; wer ihn ändert,
 *  macht die Abfrage `["config"]` ungültig. */
export function useConfig() {
  return useQuery({ queryKey: ["config"], queryFn: () => apiGet<ConfigResponse>("/config"), staleTime: Infinity });
}

/** Anzeige eines Untertyp-Schlüssels mit dem Katalog aus `GET /config`. */
export function useSubSportLabel(): (sportClass: string, key: string | null | undefined) => string {
  const catalog = useConfig().data?.data.sub_sport_catalog;
  return (sportClass, key) => subSportLabel(catalog, sportClass, key);
}
