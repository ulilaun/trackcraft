# ADR-0012: Authentifizierung und Zugriff

Status: Akzeptiert, konsolidiert am 2026-10-28.

V1 ohne App-Login/Benutzerverwaltung/Rollen. Ausschließlich eigenes LAN oder VPN. Keine Router-Portfreigabe oder öffentliche Reverse-Proxyroute zu TrackCraft. Jeder mit Anwendungszugriff darf lesen, ändern und löschen.

Vertrauensgrenze ist Netzwerk-/VPN-Zugang, nicht ein Anwendungskonto. HTTPS bleibt verpflichtend. Der einzelne Benutzer in `users` ist das fachliche Besitzerobjekt mit Profil aus TOML, keine Authentifizierung. Keine Benutzerauswahl in V1. Passwortlogin, Authentik/Keycloak/OIDC nur spätere gesonderte Optionen.
