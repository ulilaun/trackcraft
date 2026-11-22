# ADR-0003: Datenbank

Status: Akzeptiert, konsolidiert am 2026-10-28.

SQLite als V1-Primärdatenbank, einfacher Betrieb unter Windows und im Container ohne zusätzlichen DB-Server. Fremdschlüssel auf jeder Verbindung aktivieren.

PostgreSQL ist spätere Option, kein V1-Ziel. WAL, ein Schreiber und mehrere Leser, Schemaversion in `user_version`, Migrationen je Datei in einer Transaktion (C5, RFC-0001). Indizes folgen mit den Abfragen. Referenz RFC-0001.
