# Sicherheit

## Lücken melden

Bitte Sicherheitslücken **nicht** als öffentliches Issue melden, sondern vertraulich über GitHub: Reiter **Security** → **Report a vulnerability**. TrackCraft ist ein Freizeitprojekt mit einem Betreuer; eine Antwort kommt in der Regel innerhalb einer Woche.

## Unterstützte Versionen

Sicherheitskorrekturen gibt es nur für die jeweils neueste Version unter [Releases](../../releases).

## Grenzen des Schutzes

TrackCraft V1 hat bewusst **kein Login** (ADR-0012). Die Vertrauensgrenze ist das eigene Netz: Wer die Oberfläche erreicht, darf alles. Eine Portfreigabe ins Internet ist nicht vorgesehen und keine Sicherheitslücke von TrackCraft. Als Lücke zählt dagegen zum Beispiel, wenn eine fremde Webseite im Browser eines Nutzers im Heimnetz Daten in TrackCraft lesen oder ändern kann (Origin-, Host- oder CSP-Schutz umgangen, RFC-0011), wenn eine präparierte FIT- oder GPX-Datei Code ausführt oder Dateien außerhalb der Datenordner schreibt, oder wenn Schlüssel wie der Garmin-API-Schlüssel in Logs oder Antworten auftauchen.
