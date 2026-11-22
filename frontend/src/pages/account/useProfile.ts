import { useQuery } from "@tanstack/react-query";
import { apiGet, type ProfileResponse } from "../../api/client";

/** Profil des Benutzers. */
export function useProfile() {
  return useQuery({ queryKey: ["profile"], queryFn: () => apiGet<ProfileResponse>("/profile") });
}
