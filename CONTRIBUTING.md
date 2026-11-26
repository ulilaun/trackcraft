# Mitmachen

Danke für das Interesse an TrackCraft. Es ist ein Freizeitprojekt mit einem Betreuer; Antworten können ein paar Tage dauern.

## Fragen, Fehler, Ideen

Als [Issue](../../issues) melden. Bei Fehlern hilft:

- Version und Plattform (Seite **System** zeigt die Version; Windows, Linux oder Docker),
- was du getan hast, was du erwartet hast und was passiert ist,
- die passende Stelle aus dem Log (`app.<datum>.log`, siehe Betriebsanleitung), ohne Schlüssel oder Positionsdaten.

Sicherheitslücken bitte **nicht** als Issue, sondern wie in [SECURITY.md](SECURITY.md) beschrieben.

**Trainingsdateien nie öffentlich anhängen**: FIT- und GPX-Dateien enthalten Positionen, oft auch rund um die eigene Wohnung. Wenn eine Datei zum Nachstellen nötig ist, im Issue nachfragen.

## Code-Beiträge

Pull Requests bitte **erst nach Absprache im Issue**: kurz beschreiben, was du ändern willst, und auf ein Okay warten. So geht keine Arbeit verloren, weil etwas anders geplant ist.

Dann gilt:

- **Spec first:** Verhalten steht zuerst im passenden RFC bzw. in [docs/api/openapi.yaml](docs/api/openapi.yaml) ([Spezifikation](docs/README.md)), Code und Doku ändern sich im selben Pull Request.
- **Deutsch** für Doku, Commit-Nachrichten und Kommentare; Bezeichner im Code englisch.
- **Oberfläche zweisprachig:** Neue Texte kommen als Schlüssel in `frontend/src/i18n/de.json` und `en.json`, nie fest ins JSX; Datum und Zahlen nur über `frontend/src/utils/format.ts`. Die Tests prüfen beides. Änderungen an den Betriebsanleitungen (`infrastructure/*/BETRIEB.md`, `DEMO.md`) kommen auch in die englische Fassung `*.en.md`.
- Ein Thema je Pull Request, mit Tests für neues Verhalten.
- Vor dem Pull Request lokal prüfen (wie in der CI):

  ```powershell
  cd backend; cargo fmt --check; cargo clippy --all-targets -- -D warnings; cargo test
  cd frontend; npm ci; npm run lint; npm test; npm run build
  npx --yes @redocly/cli@2.76.0 lint docs/api/openapi.yaml
  ```

- **API-Vertrag:** Die API-Tests im Backend prüfen jede Antwort gegen `docs/api/openapi.yaml` (`backend/tests/common/contract.rs`). Neue API-Tests schicken ihre Anfragen über `common::contract::call` bzw. `call_json`, damit sie mitgeprüft werden.

- Keine echten Trainingsdaten, Hostnamen, Pfade oder Namen ins Repository; Beispiele mit `*.home.arpa`, `localhost` und den erfundenen Mallorca-Daten aus [scripts/demo](scripts/demo/README.md).
- Mit dem Pull Request stimmst du zu, dass dein Beitrag unter der [AGPL-3.0-or-later](LICENSE) steht.

Wie die Entwicklungsumgebung aufgebaut ist, steht im [README](README.md#entwicklung).
