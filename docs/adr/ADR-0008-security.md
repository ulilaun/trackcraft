# ADR-0008: Sicherheit

Status: Akzeptierte Regeln, konsolidiert am 2026-10-29 (Piggy 005).

Eigene FIT-/GPX-Daten lokal, keine Telemetrie oder externe Trainingssynchronisation. Zugriff ausschließlich LAN/VPN ohne App-Login, kein direkter Internetzugriff. Jeder Zugriff hat volle Lese-/Änderungs-/Löschrechte.

HTTPS für reguläre Clients: unter Windows im Backend selbst, unter Docker am Reverse Proxy, dort darf die interne Containerverbindung HTTP nutzen. Netzwerkbegrenzung (Firewall nur Profil „Privat“), Dienstkonto ohne Administratorrechte und Clientzertifikatsvertrauen sind Betriebsanforderungen.

Schutz gegen fremde Webseiten ohne Login (CSRF, DNS-Rebinding): Das Backend prüft `Host`, `Origin` und `Sec-Fetch-Site` gegen die konfigurierten Adressen, es gibt kein CORS, CSP und Security-Header setzt unter Windows das Backend, unter Docker Nginx. Kein CSRF-Token und kein Cookie. Details RFC-0011.

Externe Hintergrundkarte in V1 ausdrücklich erlaubt, IP/Ausschnittübermittlung und Internetbedarf dokumentieren. Keine vollständigen Trainingsdateien dafür hochladen. Daher keine Zusage „vollständig ohne externe Verbindungen“.

Sicherung unverschlüsselt; Sicherungsziel (USB-Datenträger, zweite Platte oder Netzwerkfreigabe) physisch und zugriffsseitig schützen. PWA ohne dauerhafte Offline-Trainingsdaten. Siehe RFC-0011.
