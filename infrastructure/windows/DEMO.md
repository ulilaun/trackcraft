# TrackCraft-Demo für Windows

Erfundene Trainingsdaten auf Mallorca (knapp zwei Jahre Laufen, Radfahren und Wandern, Segmente, Strecken, Plan).
Zum Ausprobieren. Es wird nichts installiert, und Administratorrechte sind nicht nötig.

## Starten

1. ZIP entpacken, z. B. in den Download-Ordner.
2. `demo.cmd` doppelklicken. Es öffnet sich ein Konsolenfenster, und kurz danach öffnet sich der Browser mit
   <https://localhost:8443>.
3. **Zertifikatswarnung bestätigen.** Beim ersten Aufruf warnt der Browser („Ihre Verbindung ist nicht privat“,
   „Warnung: Mögliches Sicherheitsrisiko erkannt“). Das ist bei der Demo erwartet: Das Zertifikat hat die Demo beim
   ersten Start selbst erstellt, und kein Gerät kennt es. Weiter über **Erweitert** und **Weiter zu localhost
   (unsicher)** bzw. **Risiko akzeptieren und fortfahren**. Bestätige das nur für diese Demo-Adresse.

Windows kann beim ersten Start mit „Der Computer wurde durch Windows geschützt“ warnen, weil das Programm aus dem
Internet stammt und nicht signiert ist. Dann über **Weitere Informationen** und **Trotzdem ausführen** starten.

Beenden: Konsolenfenster schließen oder darin Strg+C drücken.

## Was die Demo zeigt

Die Daten werden beim Start so verschoben, dass die letzte Aktivität gestern war. Dashboard, Woche, Form und Plan
sehen deshalb immer aktuell aus. Ein Hinweisband oben zeigt, dass es eine Demo ist. Eigene Dateien importieren,
Segmente anlegen und planen geht wie in der richtigen Installation.

## Ordner, Neubeginn, Entfernen

- Alles liegt in `%LOCALAPPDATA%\TrackCraft-Demo` (Datenbank, Originaldateien, Protokolle, Zertifikat).
- Neu beginnen mit den ursprünglichen Demo-Daten: `demo.cmd --reset`.
- Port 8443 belegt: `demo.cmd --port 9443`.
- Entfernen: Konsolenfenster schließen, den Ordner `%LOCALAPPDATA%\TrackCraft-Demo` und den entpackten Ordner löschen.

Eine eigene TrackCraft-Installation (Dienst, `C:\ProgramData\TrackCraft`) bleibt unberührt. Die Demo verwendet eigene
Ordner und einen eigenen Port und verschiebt nur Daten, die als Demo-Datenbank gekennzeichnet sind.

Für eine richtige Installation gibt es das ZIP `trackcraft-<version>-windows-x86_64.zip` mit `setup.cmd`.
