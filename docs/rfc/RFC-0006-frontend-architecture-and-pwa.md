# RFC-0006: Frontend-Architektur, UI-Design und PWA

Status: Bestätigte UI-Spezifikation (SportTracks.mobi Anlehnung), Piggy 010.

## 1. UI-Design-System & Design-Philosophie

Das Frontend orientiert sich an der funktionalen, hochdichten Benutzeroberfläche von **SportTracks.mobi**:
- **Hohe Datendichte & Lesbarkeit**: Rechtsbündige Monospace-Schriften für alle Mess- und Zeitwerte.
- **Schnitt beim Laufen als Pace**: Für die Sportklasse `running` zeigt die UI jeden Durchschnitt als Pace `m:ss /km` statt km/h: KPI-Leiste, Listen, Dashboard, Segmentversuche und Aktivitätsrekorde („Schnellste Pace“ statt „Höchster Durchschnitt“). Pace = 3600 / Schnitt in km/h, auf ganze Sekunden gerundet, halbe aufgerundet. Rad, E-Bike und `unknown` bleiben bei km/h. Die API liefert weiter km/h (`avg_speed_kmh`), umgerechnet wird nur in der Anzeige.
- **Kachel-Design** (Entwurf 06.11.2026): Jeder Wert, jede Karte, jedes Diagramm und jede Tabelle steht in einer eigenen Karte (abgerundet 16 px, feine Linie, leichter Schatten, 16 px Abstand). Kennzahlen sind eigene Karten mit kleinem Label oben und großem Wert mit Einheit. Raster mit 12 Spalten, unter 1000 px Breite untereinander.
- **Theme**: folgt der Systempräferenz (`prefers-color-scheme`), ohne Präferenz Dark. Ein Schalter in der Kopfzeile überschreibt das je Browser (lokal gespeichert, keine Serverdaten). Er wechselt zwischen „Wie System“ (Symbol: „A“ in einem Pfeilkreis für automatisch), Hell (Sonne) und Dunkel (Mond). Farben nur als Variablen bzw. Tailwind-`dark:`-Varianten. Der Beispielcode in Abschnitt 2 zeigt den älteren Stand und nur das dunkle Theme.
- **Farbschema**:

  | Token | Dark | Light |
  | :--- | :--- | :--- |
  | Hintergrund | `#0b1120` | `#eef2f7` |
  | Karte | `#121b2e` | `#ffffff` |
  | Karte sekundär (Eingaben, Pills) | `#1a2540` | `#f4f7fb` |
  | Linie | Slate 400 mit 14 % | Slate 900 mit 8 % |
  | Text / gedämpft | `#e5e9f0` / `#8b98ad` | `#0f172a` / `#64748b` |
  | Akzent | `#38bdf8`, kräftig `#0284c7` | `#0284c7`, kräftig `#0369a1` |
  | Zweite Diagrammkurve | `#f43f5e` | `#f43f5e` |
  | PR / Jahresbestzeit | Gold `#facc15` / Akzent (kein Silber und Bronze, Uli 12.11.2026) | gleich |
  | Sportklassen | Rad `#0ea5e9`, E-Bike `#14b8a6`, Laufen `#f43f5e`, Unbekannt `#64748b` | gleich |
- **Tech Stack** (verbindlich, ADR-0004): React 18+, TypeScript, Vite, TanStack Query, React Router, Tailwind CSS, Lucide Icons, Leaflet (OSM-Tiles) und Recharts (für synchrone Diagramme).

---

## 2. Komponenten und Ansichten

Wiederverwendbare Komponenten liegen unter `src/components/`, ganze Ansichten als Seiten unter `src/pages/` (RFC-0004). Die folgenden Beispiele sind Entwürfe der Struktur und des Aussehens.

### `KpiHeaderBar.tsx`
Kompakte 6-Kacheln-Leiste für Aktivitäts-Hauptwerte: Distanz, Gesamtzeit, Schnitt, Aufstieg, durchschnittliche HF und Segment-Erfolge. Unbekannte Werte werden als „–“ angezeigt, nie als 0. Die durchschnittliche HF ist ein reiner Anzeigewert aus den Dateidaten (RFC-0008), keine HF-Analyse. Segment-Erfolge zählen die Segmentversuche der Aktivität mit erster Bestzeit oder Verbesserung (RFC-0009), solange die Auswertung läuft mit Hinweis.

```tsx
import React from 'react';

interface KpiMetric {
  label: string;
  value: string | number;
  unit?: string;
  badge?: string;
}

export const KpiHeaderBar: React.FC<{ metrics: KpiMetric[] }> = ({ metrics }) => (
  <div className="grid grid-cols-2 md:grid-cols-3 lg:grid-cols-6 gap-2 bg-slate-800/80 p-3 rounded-lg border border-slate-700/60 mb-4">
    {metrics.map((m, idx) => (
      <div key={idx} className="flex flex-col items-center justify-center p-2 bg-slate-900/50 rounded border border-slate-800">
        <div className="flex items-baseline gap-1">
          <span className="text-xl md:text-2xl font-bold font-mono text-slate-100">{m.value}</span>
          {m.unit && <span className="text-xs text-slate-400 font-medium">{m.unit}</span>}
          {m.badge && <span className="text-[10px] bg-sky-500/20 text-sky-400 px-1 rounded">{m.badge}</span>}
        </div>
        <span className="text-[11px] text-slate-400 uppercase tracking-wider font-semibold mt-0.5">{m.label}</span>
      </div>
    ))}
  </div>
);
```

### `DashboardView.tsx`
2-Spalten-Layout mit Aktivitäts-Feed links und Wochen-/Monats-Umfängen inkl. Balkendiagrammen rechts.

```tsx
import React from 'react';

export const DashboardView: React.FC = () => (
  <div className="max-w-7xl mx-auto p-4 space-y-6">
    <div className="grid grid-cols-1 lg:grid-cols-12 gap-6">
      <section className="lg:col-span-7 space-y-4">
        <div className="bg-sky-600 text-white font-bold px-4 py-2 rounded-t-lg text-sm tracking-wide">
          Kürzlich erfolgte Trainingseinheiten
        </div>
        <div className="space-y-3">
          <div className="bg-slate-800 border border-slate-700/80 p-3 rounded-lg hover:border-sky-500 transition cursor-pointer">
            <div className="flex items-center gap-3">
              <div className="p-2 bg-rose-500/20 text-rose-400 rounded-lg">🚴</div>
              <div className="flex-1">
                <h3 className="font-bold text-slate-100 text-base">Almunecar - Puerta de la Cabra - Padul</h3>
                <p className="text-xs text-slate-400">Radfahren: Rennrad • Almuñécar • Do., 22. Okt.</p>
              </div>
            </div>
            <div className="grid grid-cols-5 gap-2 mt-3 pt-3 border-t border-slate-700/50 text-center font-mono">
              <div><div className="text-sm font-bold text-slate-200">136,21</div><div className="text-[10px] text-slate-400 uppercase">Distanz (km)</div></div>
              <div><div className="text-sm font-bold text-slate-200">6:42:02</div><div className="text-[10px] text-slate-400 uppercase">Zeit</div></div>
              <div><div className="text-sm font-bold text-slate-200">20,3</div><div className="text-[10px] text-slate-400 uppercase">Schnitt (km/h)</div></div>
              <div><div className="text-sm font-bold text-slate-200">2.572</div><div className="text-[10px] text-slate-400 uppercase">Höhe + (m)</div></div>
              <div><div className="text-sm font-bold text-slate-200">124</div><div className="text-[10px] text-slate-400 uppercase">Durchschn. HF</div></div>
            </div>
          </div>
        </div>
      </section>

      <section className="lg:col-span-5 space-y-6">
        <div className="bg-slate-800 border border-slate-700/80 rounded-lg overflow-hidden">
          <div className="bg-sky-600 text-white font-bold px-4 py-2 text-sm tracking-wide">Diese Woche</div>
          <div className="p-4 space-y-4">
            <div className="grid grid-cols-3 gap-2 text-center font-mono bg-slate-900/60 p-2 rounded border border-slate-700/40">
              <div><div className="text-lg font-bold text-slate-100">4</div><div className="text-[10px] text-slate-400 uppercase">Einheiten</div></div>
              <div><div className="text-lg font-bold text-slate-100">15:30:00</div><div className="text-[10px] text-slate-400 uppercase">Zeit</div></div>
              <div><div className="text-lg font-bold text-slate-100">320,40</div><div className="text-[10px] text-slate-400 uppercase">Distanz (km)</div></div>
            </div>
          </div>
        </div>
      </section>
    </div>
  </div>
);
```

### `ActivityDetailView.tsx`
Split-View aus Leaflet-Karte und Recharts-Höhendiagramm mit synchronisiertem Hover-Cursor sowie der Segment-Tabelle.

### `ActivityTableView.tsx`
Filter-Pills über einer kompakten Monospace-Datentabelle: `Alle Daten` und je eine Pill pro Sportklasse (`Radfahren`, `E-Bike`, `Laufen`, `Unbekannt`). Nach Wahl einer Sportklasse erscheinen darunter Pills für die Untertypen dieser Klasse aus dem Untertyp-Katalog der Datenbank (`GET /config`), beschriftet mit dem eigenen Namen oder der Übersetzung des Schlüssels (`subSport.<schlüssel>`). Die Pills filtern nur die Anzeige. Bestenlisten trennt allein die Sportklasse (ADR-0015).

---

### Gestaltung V1 im Einzelnen

Abgestimmt am 06.11.2026 an einem klickbaren Entwurf mit echten Daten, angelehnt an SportTracks.mobi und Strava.

**Navigation**: Kopfzeile mit Logo und Name, die zum Dashboard führen (kein eigener Eintrag „Dashboard“), Aktivitäten, Segmente, Strecken, Statistik, rechts **Daten** (`/daten`: Sicherung, Wiederherstellen und Export, Uli 26.11.2026) und System sowie durch einen Trennstrich abgesetzt der Import-Knopf, daneben ein Zahnrad zu den **Einstellungen** (`/einstellungen`, Uli 14.11.2026) und ganz rechts das **Benutzersymbol** zur Kontoseite `/konto` (Kreis in wählbarer Farbe mit dem Spitznamen, wenn er höchstens 3 Zeichen hat, sonst dessen erstem Zeichen). Hell/Dunkel/Wie System steht unter Einstellungen → Darstellung, nicht mehr in der Kopfzeile. Daten trägt einen orangen Punkt bei ungesicherten Änderungen oder fehlendem Ziel, System einen roten Punkt bei Warnungen. Detailseiten haben oben „← Zurück zu …“ und führen dorthin zurück, woher man kam (Aktivität, Aktivitätenliste, Segmentliste, Segment). Ein Klick auf einen Kopfzeilen-Eintrag beginnt einen neuen Weg.

**Aktivitätenliste**
- Oben vier Kennzahl-Karten über die gefilterten Aktivitäten: Anzahl, Distanz, Gesamtzeit, Aufstieg.
- Filterkarte: Chip „Letzte 12 Wochen“ (Vorgabe, abwählbar), Auswahl **Saison** (RFC-0009 „Saisons“; schließt „Letzte 12 Wochen“ aus, in der Adresse als `?saison=<id>`; nur Auswahl, erscheint erst, wenn es Saisons gibt), Suche in Titel, Beschreibung und Ort, Auswahl **Strecke** (RFC-0016), Pills für Sportklasse und daneben für Untertyp.
- Tabelle: Icon und Titel, Start `TT.MM.JJ HH:MM`, Untertyp, Distanz, Gesamtzeit, Schnitt (Laufen Pace), Aufstieg, Ø HF, Zahl der Segment-Erfolge. Sortierbar nach Start, Distanz, Zeit und Aufstieg per Klick auf den Spaltenkopf.
- **Summenzeile** am Ende über alle gefilterten Aktivitäten, nicht nur die geladenen: Anzahl, Distanz, Zeit, Aufstieg und Segmente als Summe, Schnitt aus Gesamtdistanz durch Gesamtzeit (bei gemischten Sportklassen „gemischt“), HF zeitgewichtet über Aktivitäten mit HF.
- „Mehr laden“ unter der Tabelle (seitenweise, `limit`/`offset`).

**Aktivitätsdetail**
- Kopfkarte: Icon, Titel, Start, Sportklasse und Untertyp, Blättern zur vorherigen und nächsten Aktivität, Bearbeiten, Segment erstellen, „Ganze Aktivität als Strecke“ (RFC-0016), Löschen.
- Karte „Top-Ergebnisse“, wenn die Aktivität PRs oder Plätze 2 und 3 hat (Erste Bestzeit, Verbesserung mit Zeitersparnis, Einstellung, Platz). Ein Klick öffnet das Segment.
- Darunter links 6 Kennzahl-Karten (Distanz, Gesamtzeit, Schnitt bzw. Pace, Aufstieg, Ø HF, Segmente mit Zahl der PRs), rechts die Karte, etwa 5 zu 7 Spalten. Karte mit Start (grün), Ziel (rot) und einem Punkt, der dem Diagramm-Cursor folgt. Linie in einer festen Farbe (Akzent), ohne Kilometer- oder Zeitmarken (Uli, 09.11.2026).
- Diagramm über die volle Breite mit **zwei Kurven**: Kurve 1 als Fläche mit Achse links, Kurve 2 als Linie mit Achse rechts, je Höhe, Geschwindigkeit bzw. Pace oder HF wählbar, Kurve 2 auch „keine“. Vorgabe Höhe und HF. X-Achse Distanz oder Zeit. Zoom durch Ziehen, „Gesamt“ setzt zurück. Die Distanzachse ist die GPS-Distanz der Messpunkte und kann vom KPI-Wert aus der Datei abweichen.
- Segment-Tabelle: PR-Zeichen, Name mit Kategorie-Symbol, Länge, Aufstieg und Steigung (Strecken ohne Steigung), Zeit in Zehnteln, Schnitt bzw. Pace, HF Ø und max, Rang „n von m“, Zeitersparnis. Hover hebt das Segment auf Karte und Diagramm hervor, Klick öffnet die Segmentseite.
- Karte „Beschreibung“.

**Segmentliste**: Segmente und Strecken haben je einen eigenen Eintrag in der Kopfzeile und eine eigene Liste mit gleichem Aufbau (Uli, 11.11.2026, statt zweier Reiter): vier Kennzahl-Karten (Anzahl, Versuche, Gesamtlänge, PR-Verbesserungen), Pills je Sportklasse und je Kategorie mit Symbol (RFC-0016 „Kategorien“), Tabelle mit Name, Kategorie, Sportklasse, Länge, Aufstieg, Steigung, Versuche, Bestzeit, letztem Versuch. Strecken zeigen statt der Steigung den Ort (sortierbar), die Suche findet auch den Ort (Uli, 12.11.2026).

**Segmentseite** (ein Segment oder eine Strecke)
- Kopfkarte mit Name, Kategorie mit Symbol, Sportklasse, Ort, Bearbeiten (Name, Art, Kategorie, Ort, Beschreibung).
- 6 Kennzahl-Karten: Länge, Aufstieg mit Steigung (Strecke ohne), Bestzeit mit Schnitt, Versuche seit erstem Versuch, letzter Versuch mit Rang, und „Diese Aktivität“ mit Rang, wenn man aus einer Aktivität kommt (sonst Ø Zeit).
- Karte des Segments und Höhenprofil nebeneinander.
- **Zeitraum** für Entwicklung und Tabelle: „Gesamt“, „Jahr“ oder „Saison“ (Uli, 12. und 13.11.2026). Jahr und Saison beginnen mit dem laufenden Jahr bzw. der Vorgabesaison (RFC-0009 „Saisons“), frühere sind wählbar. „Saison“ erscheint nur, wenn es Saisons gibt.
- **Bestzeiten-Entwicklung**: alle gültigen Versuche als Balken in zeitlicher Folge direkt nebeneinander (keine lineare Zeitachse, das Datum steht darunter; die Balkenbreite passt sich der Anzahl an, bis zu einer Höchstbreite). Gold mit PR-Zeichen ist die Bestzeit, Akzent die Bestzeit des laufenden Jahres, alle anderen gedämpft. Ein kleiner gelber Punkt über dem Balken markiert eine damalige Bestzeit. Der Versuch der Herkunftsaktivität hat einen orangen Rahmen (Uli, 11. und 12.11.2026).
- Tabelle aller Versuche nach Zeit im gewählten Zeitraum: Rang bzw. PR-Zeichen, Datum mit Aktivität, Zeit, Schnitt bzw. Pace, HF Ø und max mit Dreieck ▲/▼ gegenüber der Bestzeit des Zeitraums, „Bestzeit 2026“ beim besten Versuch des laufenden Jahres und „Bestzeit {Saisonname}“ (grün) beim besten Versuch der Vorgabesaison, also der laufenden oder zuletzt beendeten (Uli 14.11.2026), unabhängig vom gewählten Zeitraum, Rückstand zur Bestzeit des Zeitraums. Die Zeile der Herkunftsaktivität hat einen orangen Rahmen, Hover über Zeile und Balken ist gekoppelt, ein Klick öffnet die Aktivität.

**Statistik** (Uli, 13.11.2026): Saisons werden **nur hier** angelegt, geändert und gelöscht; Bestenliste und Aktivitätenliste wählen sie nur aus.
- **Saison-Reihe** oben: zuerst die Vorgabesaison (RFC-0009 „Saisons“), dann bis zu vier weitere angelegte Saisons, neueste zuerst, zuletzt eine leere Kachel „+ Neue Saison“. Eine Kachel zeigt den Namen, ein Symbol „läuft“ oder „nicht aktiv“, Anzahl Einheiten, km und Gesamtzeit. Ein Stift auf der Kachel öffnet denselben Dialog wie „+ Neue Saison“ (Name, erster und letzter Tag, unten links „Löschen“); Überschneidung und doppelter Name werden am Feld gemeldet und nennen die andere Saison.
- **Zeitraum-Reihe** darunter: eine Kachel mit dem Jahr (◀ Jahr ▶ für frühere Jahre), daneben Felder Von/Bis, Sportklasse und Untertyp (Katalog aus der Datenbank).
- Ein Klick auf eine Saison- oder die Jahres-Kachel wählt den Zeitraum der ganzen Seite, die gewählte Kachel ist hervorgehoben. Von/Bis überschreibt die Kachelwahl, dann ist keine Kachel gewählt. Sportklasse und Untertyp gelten für jede Zeitraumwahl.
- Darunter für den gewählten Zeitraum: Vier Kennzahl-Karten (Einheiten, Distanz, Zeit, Aufstieg). Distanz und Zeit je Monat als gestapelte Balken nach Sportklasse, daneben je ein Ring mit den Anteilen der Sportklassen. Zeit je ISO-Woche als Balken. Darunter die Aktivitätsrekorde je Sportklasse mit Datum (RFC-0009).

**Dashboard**: bleibt wie RFC-0018, im Kachel-Design.

**Importdialog** (RFC-0008, abgestimmt am 06.11.2026, geändert am 08.11.2026: Speichern ohne Rückfrage, keine Titeleingabe)
1. **Ablagefläche**: FIT- und GPX-Dateien hineinziehen oder mehrere auswählen, Hinweis auf 50 MB je Datei. ZIP-Archive werden nicht angenommen.
2. **Hochladen und prüfen**: Liste der Dateien mit Fortschritt, nacheinander hochgeladen. Jede Datei ohne Konflikt wird direkt nach der Prüfung gespeichert (RFC-0008 Abschnitt 8.1), in der Liste „importiert“.
3. **Entscheidungen**, nur wenn eine Datei einen Konflikt hat: vier Kennzahl-Karten (Dateien mit Gesamtgröße, Importiert, Entscheidung nötig mit Zahl der offenen, Nicht importierbar). Darunter:
   - Je vermutetem Duplikat eine Karte: links „Vorhanden“ und „Neu“ nebeneinander (Titel, Quelle, Start, Gesamtzeit, Distanz), passende Kriterien markiert. Rechts die drei Optionen „Nicht importieren“, „Zusätzlich importieren“, „Vorhandene ersetzen“ als Auswahlkarten, bei „Ersetzen“ die Übernahme von Titel, Beschreibung, Ort, Sportklasse und Untertyp (vorausgewählt). Ohne Auswahl ist die Karte hervorgehoben.
   - Bei Sessions ohne GPS und Zeit die Eingabe von Start und Gesamtzeit in derselben Art, abwählbar.
   - Weitere Sessions derselben Datei ohne Konflikt als Liste „Wird mit der Entscheidung gespeichert“ (eine Datei wird gemeinsam gespeichert).
   - Liste **Importiert** (Titel, Datei, Start, Untertyp, Distanz, Zeit, HF), nur Anzeige.
   - Karte **Nicht importierbar**: gesperrte Sessions und abgewiesene Dateien mit Kennzeichen (Fahrzeug erkannt, Punktlimit, bereits vorhanden, abgewiesen) und der Meldung im Wortlaut.
4. **Leiste unten** (bleibt sichtbar, nur bei offenen Entscheidungen): Zahl der Aktivitäten, die gespeichert werden, offene Entscheidungen, „Offene verwerfen“ und „Entscheidungen bestätigen“. Bestätigen ist gesperrt, solange eine Entscheidung offen ist. „Offene verwerfen“ betrifft nur die wartenden Dateien, schon gespeicherte bleiben.
5. **Ergebnis**: Kennzahl-Karten Importiert, Ersetzt, Übersprungen, Abgewiesen, dazu der Fortschritt der Auswertung (Aufträge), die Liste „Importiert“ und „Zu den Aktivitäten“ bzw. „Weitere Dateien importieren“.

Titel, Beschreibung, Ort und Untertyp werden nicht im Importdialog bearbeitet, sondern auf der Detailseite der Aktivität, wo Karte und Höhenprofil zeigen, welche Fahrt es war. Bis dahin gilt der Titel nach RFC-0008 Abschnitt 6.

**Sicherung** (RFC-0012)
- Vier Kennzahl-Karten: letzte erfolgreiche Sicherung (Zeit, Revision), letzter Versuch (Zeit, Ergebnis: gesichert, übersprungen wegen keiner Änderung, übersprungen wegen fehlendem Ziel, Fehler, jeweils mit Grund), ungesicherte Änderungen (ja/nein, kurz was), Sicherungsziel (erreichbar, fehlt, nicht beschreibbar, freier Platz).
- Karte „Jetzt sichern“ mit Hinweis auf den Tageslauf um 03:00. Während des Laufs die Phasen (Bedingungen prüfen, Stand einfrieren, kopieren, Manifest und Prüfsummen prüfen) mit Fortschritt, danach das Ergebnis. Kein zweiter Lauf parallel.
- Karte „Hinweise“: unverschlüsselt, Ziel schützen, fünf Stände, USB regelmäßig trennen und Restore testen, ein Ziel auf demselben Rechner schützt nicht vor dessen Verlust. Unter Windows zusätzlich: Sicherung um 03:00 nur bei laufendem PC, sonst Nachholen nach dem Start.
- Tabelle der Stände: Erstellt, Art (regulär, vor Migration), Aktivitäten, Segmente, Größe, Prüfstatus, Revision, „Wiederherstellen…“. Fehlerhafte Stände sind nicht wählbar. Unerreichbares, leeres und nicht lesbares Ziel werden unterschieden.
- **Wiederherstellen** als eigene Seite mit „← Zurück zur Sicherung“: aktueller Stand und Sicherung nebeneinander (Aktivitäten, Segmente, Revision, Differenz), Karte „Wirkung“ im Klartext (vollständiger Ersatz, kein Zusammenführen, was verloren geht, Profil aus der aktuellen TOML, alter Stand bleibt bei Fehler erhalten). Bei `older` und `not_comparable` ist ein Häkchen zur Zustimmung Pflicht, vorher ist „Wiederherstellen“ gesperrt. Danach die Phasen (prüfen, bereitstellen, umschalten mit Schreibsperre, Funktion prüfen) und das Ergebnis mit Hinweis auf die Neuberechnung.

**System** (RFC-0013)
- Vier Kennzahl-Karten: Version (App, Schema, Algorithmus, API, Plattform), Laufzeit seit Start, freier Speicher im Datenbereich und im Sicherungsziel.
- Karte „Warnungen“: je Warnung Farbe nach Schwere, Text, „seit“ und Code (`low_disk_space`, `backup_pending`, `backup_failed`, `job_failed`, `calculation_stale`, `certificate_expiring`).
- Karte „Berechnungsstand“: läuft, aktuell, veraltet oder Fehler, Fortschritt, veröffentlichter Stand mit Zeit und Algorithmus. Daneben Karte „Bestand“: Aktivitäten, Segmente (davon Strecken), Versuche, Originaldateien mit Größe.
- Tabelle der Auswertungsaufträge: Art, Bezug, Status, Fortschritt, erstellt, Fehlermeldung. Fehlgeschlagene haben „Wiederholen“ (`POST /jobs/{id}/retry`).
- Karte „Gesundheit“: Backend, Datenbank, Dateisystem, Speicher mit Status und Kurztext aus `/health`.
- Fußzeile der Seite: „TrackCraft, Version …, Lizenz AGPL-3.0“ mit Link zum Quellcode des laufenden Stands (AGPL § 13, Adresse aus der Markendatei).

**Später (V2)**: Kalender (Monatsansicht), manuelle Erfassung einer Aktivität ohne Datei (RFC-0017), konfigurierbare Spalten der Aktivitätenliste, Detailseite aus frei anordenbaren Karten, Vergleich zweier Aktivitäten (Differenz-Kacheln, Kurven übereinander), einschaltbare Marken in Karte und Diagramm (Kilometer, Zeit und weitere), Färbung der Kartenlinie nach Steigung, Geschwindigkeit, HF und weiteren Werten, wählbare Linienfarbe, HF-Zonen und Glättung im Diagramm, Runden und Intervalle, freier Zeitraum und Ø HF in der Statistik.

### Sprache und Formate

- **Sprache**: Alle Texte stehen zentral in Sprachdateien `src/i18n/<sprache>.json` und werden über Schlüssel angesprochen (`t(key, params)`, Platzhalter `{name}`). **Deutsch (`de.json`) ist die Referenz**: Die Schlüsseltypen kommen aus `de.json`, jede andere Sprache darf nur deren Schlüssel mit denselben Platzhaltern enthalten, und fehlende Schlüssel fallen auf Deutsch zurück. Mehrzahl über `tn(key, count)` mit `Intl.PluralRules` und Schlüsseln `…_one` / `…_other` (`{count}` als Platzhalter). Sprachen: Deutsch und Englisch, beide vollständig; ein Test prüft, dass jede als vollständig geführte Sprache (`COMPLETE_LANGUAGES`) alle Schlüssel hat. Neue Texte kommen in jedem PR in beide Sprachdateien. Englische Begriffe: Strecke = route, Segmentversuch = effort, Einheit (Plan) = session, Belastung = load, Ermüdung = fatigue, Untertyp = sub-type, Sportklasse = sport. Keine sichtbaren Texte fest im Komponenten-Code; ein Test prüft JSX-Texte und die Attribute `title`, `aria-label`, `placeholder`, `alt` (Einheiten wie `km` erlaubt, Ausnahmen per Kommentar `i18n-ignore`). Meldungen von Backend-CLI, Setup und Log bleiben Deutsch, die API liefert Codes.
- **Wahl** unter Einstellungen → Darstellung, je Browser im `localStorage` (`trackcraft.language`, `trackcraft.format`), wie das Design. Sprache: „Automatisch“ (erste vollständige Sprache des Browsers, sonst Deutsch) oder fest. Format von Datum und Zahlen getrennt davon: „Wie die Sprache“ (Deutsch → `de-DE`, Englisch → `en-US` bei amerikanischem Browser, sonst `en-GB`) oder fest `de-DE`, `en-GB`, `en-US`. Ein Wechsel baut die Oberfläche neu auf und setzt `<html lang>`. Mit mehreren Benutzern (#39) wird daraus eine Einstellung je Benutzer.
- **Formate** über `Intl` im gewählten Gebietsschema, ausschließlich in `src/utils/format.ts` (kein festes Gebietsschema außerhalb davon und `src/i18n`, ein Test prüft das). Für `de-DE`: Komma als Dezimalzeichen, Punkt als Tausendertrenner (`1.234,5 km`), Datum `TT.MM.JJJJ`, in Tabellen `TT.MM.JJ`, Uhrzeit 24 Stunden `HH:MM`, Dauer `h:mm:ss` bzw. `m:ss`, Segmentzeiten mit Zehnteln (`4:31,2`), Pace `m:ss /km`; andere Gebietsschemata entsprechend (z. B. `en-US`: `11/24/2026`, `02:05 PM`, `4:31.2`). Wochentags- und Monatsnamen kommen aus `Intl`, nicht aus festen Listen. Nur metrische Einheiten (km, m, km/h, bpm); imperiale Einheiten sind ein späterer Schritt. Wochen beginnen immer montags, Wochennummern nach ISO. Zeiten in der Zeitzone der TOML (RFC-0009).
- **Fehlercodes**: Das Frontend übersetzt jeden Code aus RFC-0003 Abschnitt 7 über eine Tabelle in derselben Übersetzungsdatei. Für einen unbekannten Code zeigt es die `message` der API und den Code. Feldfehler (`details.fields`) erscheinen am jeweiligen Eingabefeld.

### Produktname und PWA-Auftritt

- **Name**: Das Produkt heißt **TrackCraft** (Kurzname ebenfalls TrackCraft). Die technischen Bezeichner heißen ebenfalls `trackcraft` und sind eingefroren (RFC-0021, G1).
- **Eine Stelle**: Name, Kurzname, Icon und Farben stehen in einer Markendatei des Frontends (z. B. `src/brand.ts`). Seitentitel, Kopfzeile, Texte und das Manifest beziehen sie von dort, das Manifest wird beim Build daraus erzeugt. Hostname und Zertifikat legt der Betrieb fest (`allowed_origins`, RFC-0011), nichts davon steht im Code.
- **Manifest**: `name` und `short_name` TrackCraft, `start_url` `/`, `display` `standalone`, `theme_color` und `background_color` `#0b1120`, `lang` `de`.
- **Icon**: geschwungener, nach hinten leicht schmaler werdender Pfad in `#38bdf8` auf `#0b1120`, um 25° geneigt ([docs/assets/trackcraft-icon.svg](../assets/trackcraft-icon.svg)). Daraus beim Build PNG 192 und 512 px (auch `maskable`), Apple-Touch-Icon 180 px und Favicon 32 px.
- **Sportklassen-Icons** aus Lucide, in der Sportklassenfarbe: Radfahren `Bike`, E-Bike `Zap`, Laufen `Footprints`, Skifahren `Snowflake` (#6366f1), Schwimmen `Waves` (#2563eb), Studio `Dumbbell` (#f59e0b), Unbekannt `CircleHelp` (Uli 14.11.2026). Die Liste der Sportklassen steht in `src/utils/sports.ts`.

## 3. PWA-Verhalten

- **Installierbare PWA**: Der Service Worker macht nur die App-Shell und statische Assets offline verfügbar.
- **Keine Offline-Trainingsdaten**: Keine dauerhafte Offline-Kopie von Aktivitäten, Segmenten oder API-Antworten auf dem Client (ADR-0004, ADR-0008, RFC-0011). Der TanStack-Query-Cache liegt nur im Arbeitsspeicher, nicht in IndexedDB.
- **Keine Offline-Änderungen**: Es gibt keine Offline-Queue und keine Synchronisation mit Last-Write-Wins. Alle Änderungen erfordern eine Verbindung zum TrackCraft-Rechner.
- **Ohne Serververbindung** (z. B. PC ausgeschaltet oder im Energiesparmodus) zeigt die App einen Hinweis, keine gespeicherten Daten.

## 4. Seitenstruktur (V1)

| Seite | Inhalt | Quelle |
| :--- | :--- | :--- |
| Dashboard | Feed der letzten Aktivitäten, laufende Woche, laufender Monat, Zugang zum Import | RFC-0018 |
| Aktivitäten, Liste | Kompakte Tabelle mit Filter-Pills nach Sportklasse und Untertyp | Abschnitt 2 |
| Aktivitäten, Detail | Karte, Höhenprofil, KPI-Leiste, Segmentversuche, Bearbeiten, Löschen, Segment erstellen | RFC-0017, RFC-0016 |
| Import | Dialog mit Upload (auch mehrere Dateien), Speichern ohne Rückfrage bei Dateien ohne Konflikt, Duplikatvergleich, Eingabe von Start und Gesamtzeit | RFC-0008 |
| Segmente, Strecken | Je eine Liste mit Suche und Filter, Segmentseite mit Karte, Höhenprofil, Bestzeiten-Entwicklung und allen Versuchen, Erstellen aus einer Aktivität | RFC-0016, RFC-0009, Abschnitt 2 |
| Statistik | Gesamtübersicht (Anzahl, Distanz, Aufstieg, Zeit, Segmentzahl, PR-Verbesserungen), Jahresübersicht nach ISO-Woche, Monat und Jahr je Sportklasse, Aktivitätsrekorde | RFC-0009 |
| Sicherung | Status, „Jetzt sichern“, Stände mit Datenstand und Prüfstatus, Wiederherstellen mit Vorschau und Zustimmung | RFC-0012, Abschnitt 2 |
| System | Version, Laufzeit, Speicher, Warnungen, Berechnungsstand, Bestand, Aufträge mit Wiederholen, Gesundheit | RFC-0013, Abschnitt 2 |
| Einstellungen (Zahnrad) | Master-Detail: Synchronisierung (Garmin Connect), Sportarten (Zuordnung der Sporttypen, Untertyp-Katalog), Darstellung (Hell/Dunkel/Wie System) | RFC-0008 Abschnitt 4, Abschnitt 4 unten |
| Konto (Benutzersymbol) | Spitzname (Pflicht), Vorname, Nachname, Geburtstag mit Alter, Farbe des Symbols; eigenes Bild später | RFC-0001 „Benutzerprofil“ |

Alle Seiten sind gestaltet (Abschnitt 2).

**Einstellungen** (Uli 16.11.2026): Master-Detail mit der Abschnittsliste links (ca. 28 % Breite) und dem gewählten Abschnitt rechts, Abschnitt in der URL (`/einstellungen/synchronisierung`, `/sportarten`, `/darstellung`); ohne Auswahl zeigt der große Bildschirm den ersten Abschnitt, unter `lg` erst nur die Liste und ein Abschnitt dann allein mit „← Alle Einstellungen“. Abschnitte: **Synchronisierung** (Garmin Connect), **Sportarten**, **Darstellung** (Hell, Dunkel, Wie System als drei Karten, Wahl je Browser).

**Einstellungen → Sportarten** (Uli 14.11.2026): Zuordnung der Sporttypen und Untertyp-Katalog.
- Karte **Zuordnung der Sporttypen**: Tabelle aller `source_sport` mit eigenem Eintrag oder aus vorhandenen Aktivitäten (Typ, Anzahl Aktivitäten, Sportklasse, Untertyp), jede Änderung wird sofort gespeichert. Typen mit Aktivitäten, die ein neuer Import als „Unbekannt“ anlegen würde, stehen oben, gelb hinterlegt mit Warnzeichen und einem Satz mit ihrer Zahl. Die Auswahl „– nicht zugeordnet –“ bzw. „– wie *sport* (Klasse) –“ löscht den eigenen Eintrag. Wechselt die Klasse, bleibt der Untertyp nur, wenn die neue Klasse ihn kennt. Unten lässt sich ein neuer Typ anlegen, auch vor dem ersten Import; er wird wie ein GPX-Typ normalisiert (Kleinbuchstaben, `_`, der Bindestrich bleibt). Der Hinweis „Änderungen gelten nur für neue Importe“ steht über der Tabelle.
- Karte **Untertypen je Sportklasse**: je Klasse (ohne Unbekannt) die Einträge mit Verwendung (Aktivitäten, Zuordnungen), Stift zum Umbenennen, bei umbenannten eingebauten Einträgen „ursprüngliche Bezeichnung wiederherstellen“, Papierkorb nur, solange nichts den Eintrag verwendet (sonst gesperrt mit Hinweis), darunter ein Feld für einen neuen Untertyp.
- Nach jeder Änderung lädt die Seite die Konfiguration neu, sodass Bearbeiten-Dialog, Filter und Anzeige den neuen Katalog sofort nutzen.
