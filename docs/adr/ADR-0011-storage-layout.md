# ADR-0011: Speicherlayout

Status: Akzeptiert, konsolidiert am 2026-10-29 (Piggy 005).

## Initiale Pfadstruktur

```text
data                # Windows: C:\ProgramData\TrackCraft\data, Container: /data
  activities        # Archiv-Basispfad der Originale (FIT und GPX)
  database
  backups           # nur lokale Migrationssicherung, nur die letzte bleibt
  logs
```

Diese Pfade sind initiale konfigurierbare Werte (RFC-0005). Dateinamen im Archiv sind unter Windows und Linux gleich: Kleinbuchstaben aus `sha256` und Format, damit die Groß-/Kleinschreibung keine Rolle spielt. Alle Anwendungsparameter kommen aus der zentralen TOML-Datei nach RFC-0021. Keine Einzelwert-Overrides durch `GPX_PATH` oder `ACTIVITIES_PATH`.

FIT- und GPX-Originale verwenden denselben Archiv-Basispfad. Während Aufbewahrung werden Originale nicht verändert oder überschrieben.

## Referenzen und Löschen

Mehrere FIT-Sessionaktivitäten können dieselbe Originaldatei referenzieren. Fehlende Sessions derselben Datei sind ergänzbar, ohne Original zu duplizieren oder bestehende Aktivitäten zu verändern. Beim Löschen oder Ersetzen einer Aktivität wird die Originaldatei nur entfernt, wenn keine Aktivität mehr darauf verweist. Bestehende Sicherungen werden nicht rückwirkend bearbeitet. Das gilt ebenso für Reimport und Fehler-Rücknahme.

Staging (C1, 08.11.2026): Unterordner `staging` in `database_dir`, kein eigener TOML-Schlüssel. `database_dir` und `activities_dir` müssen auf demselben Laufwerk bzw. Dateisystem liegen, damit der Wechsel vom Staging in die Ablage ein atomares Umbenennen ist (Startprüfung, RFC-0021). Je Vorgang ein Ordner `staging/<vorgang-id>`, der Zustand des Vorgangs steht in der Datenbank. Beim Start räumt TrackCraft auf: Ordner abgeschlossener oder unbekannter Vorgänge werden gelöscht, unterbrochene Vorgänge nach ihrer Regel zurückgenommen (Import, Ersetzen, Sicherung) bzw. beendet (Restore: der alte Stand bleibt gültig, solange der Wechsel nicht vollständig war, RFC-0012). Abgewiesene Uploads werden nicht aufbewahrt (nur Fehlermeldung und Logeintrag), kein rejected-Ordner. Die lokale Migrationssicherung liegt im konfigurierten Migrationsordner (`data\backups` bzw. `/data/backups`), nur die letzte bleibt. Das Sicherungsziel (USB, zweite Platte oder Netzwerkfreigabe) ist ein eigener Pfad, im Container separat eingebunden.
