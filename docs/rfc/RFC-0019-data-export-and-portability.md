# RFC-0019: Export und Portabilität

Status: Konsolidierte V1-Abgrenzung, 2026-10-29 (Piggy 005).

## V1

V1 enthält vollständig keine Datenexportfunktion: weder JSON/CSV-/Account-/Gesamtexport, FIT-/GPX-Originaldownload, Konvertierung noch Backup-Download. Kein Exportendpunkt, keine Export-/Downloadaktion in der UI.

Reguläre API-Leseoperationen für Analyse/Kartendarstellung bleiben notwendig und sind kein angebotener Exportworkflow. Betriebliche Vollsicherung in konfiguriertes Verzeichnis und Einspielen eines Sicherungsstands bleiben Bestandteil von V1.

Originaldateien/Fachdaten bleiben auf eigener Infrastruktur nach Referenz-/Löschregeln verfügbar, ohne dafür eine Exportoberfläche zu versprechen.

## Spätere Optionen

Maschinenlesbarer versionierter JSON-/CSV-/Gesamtexport, Originaldownload oder Import solcher Exportarchive können gesondert definiert werden. Keine dieser Optionen ist bereits Teil des V1-Vertrags; kein fiktiver `/export`-Endpunkt in der aktuellen API.
