import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { FORMAT_KEY, LANGUAGE_KEY, resetLocaleForTests, setLocale } from "../../i18n";
import { chooseOption } from "../../components/chooseOption.test-helper";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { resetThemeForTests } from "../../hooks/useTheme";
import { THEME_KEY } from "../../utils/theme";
import type { SportMapping, SubSport } from "../../api/client";
import { json } from "../activities/testData";
import { SettingsPage } from "./SettingsPage";
import { classChange, inheritedFrom, needsAttention, normalizeSource, sortMappings } from "./sportsModel";

const mapping = (p: Partial<SportMapping> & { source_sport: string }): SportMapping => ({
  mapped: true,
  sport_class: "cycling",
  sub_sport: null,
  effective_sport_class: "cycling",
  effective_sub_sport: null,
  activity_count: 0,
  ...p,
});

const sub = (p: Partial<SubSport> & { key: string }): SubSport => ({
  sport_class: "cycling",
  name: null,
  builtin: true,
  activity_count: 0,
  mapping_count: 0,
  ...p,
});

describe("Sportarten-Modell", () => {
  it("nicht zugeordnete Typen mit Aktivitäten fallen auf und stehen oben", () => {
    const fe = mapping({ source_sport: "fitness_equipment", mapped: false, sport_class: null, effective_sport_class: "unknown", activity_count: 189 });
    const cy = mapping({ source_sport: "cycling", activity_count: 12 });
    const ro = mapping({ source_sport: "rowing", mapped: false, sport_class: null, effective_sport_class: "unknown" });
    expect(needsAttention(fe)).toBe(true);
    expect(needsAttention(ro)).toBe(false);
    expect(sortMappings([cy, fe, ro]).map((m) => m.source_sport)).toEqual(["fitness_equipment", "cycling", "rowing"]);
  });

  it("Rückfall auf den Sport, Klassenwechsel und neuer Typ", () => {
    expect(inheritedFrom(mapping({ source_sport: "cycling-gravel", mapped: false, sport_class: null }))).toBe("cycling");
    expect(inheritedFrom(mapping({ source_sport: "cycling" }))).toBeNull();
    const subs = [sub({ key: "indoor" }), sub({ key: "indoor", sport_class: "swimming" }), sub({ key: "mtb" })];
    expect(classChange(mapping({ source_sport: "x", sub_sport: "indoor" }), "swimming", subs)).toEqual({ sport_class: "swimming", sub_sport: "indoor" });
    expect(classChange(mapping({ source_sport: "x", sub_sport: "mtb" }), "running", subs)).toEqual({ sport_class: "running", sub_sport: null });
    expect(normalizeSource("  Gravel Ride! ")).toBe("gravel_ride");
    expect(normalizeSource("cycling-gravel")).toBe("cycling-gravel");
  });
});

let mappings: SportMapping[];
let subs: SubSport[];
let writes: { method: string; url: string; body: unknown }[];
let failNext: unknown;
let osmEnabled: boolean;

beforeEach(() => {
  mappings = [
    mapping({ source_sport: "cycling", activity_count: 12 }),
    mapping({ source_sport: "cycling-mountain", sub_sport: "mtb", effective_sub_sport: "mtb", activity_count: 3 }),
    mapping({ source_sport: "fitness_equipment", mapped: false, sport_class: null, effective_sport_class: "unknown", activity_count: 189 }),
  ];
  subs = [sub({ key: "road" }), sub({ key: "mtb", activity_count: 3, mapping_count: 1 }), sub({ key: "custom_1", name: "Gravel", builtin: false })];
  writes = [];
  failNext = null;
  osmEnabled = true;
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string, init?: RequestInit) => {
      const method = init?.method ?? "GET";
      if (method !== "GET") {
        writes.push({ method, url, body: init?.body ? JSON.parse(String(init.body)) : undefined });
        if (failNext) {
          const f = failNext;
          failNext = null;
          return json(409, f);
        }
        if (url.endsWith("/places/settings")) {
          osmEnabled = (JSON.parse(String(init?.body)) as { osm_enabled: boolean }).osm_enabled;
          return json(200, { data: { osm_enabled: osmEnabled } });
        }
        return method === "DELETE" ? new Response(null, { status: 204 }) : json(200, { data: {} });
      }
      if (url.endsWith("/sport-mappings")) return json(200, { data: mappings, meta: { total: mappings.length } });
      if (url.endsWith("/sub-sports")) return json(200, { data: subs, meta: { total: subs.length } });
      if (url.endsWith("/places/settings")) return json(200, { data: { osm_enabled: osmEnabled } });
      return json(404, { error: { code: "not_found", message: "nope" } });
    }),
  );
});

function setup(path = "/einstellungen/sportarten") {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter initialEntries={[path]}>
        <Routes>
          <Route path="/einstellungen" element={<SettingsPage />} />
          <Route path="/einstellungen/:section" element={<SettingsPage />} />
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe("Einstellungen als Master-Detail", () => {
  it("listet die Abschnitte und zeigt ohne Auswahl die Synchronisierung", () => {
    setup("/einstellungen");
    const nav = screen.getByRole("navigation", { name: "Abschnitte der Einstellungen" });
    const links = within(nav).getAllByRole("link");
    expect(links.map((l) => l.getAttribute("href"))).toEqual([
      "/einstellungen/synchronisierung",
      "/einstellungen/sportarten",
      "/einstellungen/plan-vorlagen",
      "/einstellungen/orte",
      "/einstellungen/export",
      "/einstellungen/darstellung",
    ]);
    expect(links[0]).toHaveAttribute("aria-current", "page");
    expect(screen.getByRole("heading", { level: 2, name: "Synchronisierung" })).toBeInTheDocument();
    expect(screen.queryByRole("link", { name: /Alle Einstellungen/ })).toBeNull();
  });

  it("Orte: OpenStreetMap-Abruf lässt sich abschalten (#22)", async () => {
    setup("/einstellungen/orte");
    const box = await screen.findByRole("checkbox", { name: /Sonst OpenStreetMap fragen/ });
    expect(box).toBeChecked();
    fireEvent.click(box);
    await waitFor(() => expect(writes).toEqual([{ method: "PATCH", url: "/api/v1/places/settings", body: { osm_enabled: false } }]));
    await waitFor(() => expect(box).not.toBeChecked());
  });

  it("wechselt per Link den Abschnitt, mit Zurück-Link für kleine Bildschirme", async () => {
    setup("/einstellungen");
    fireEvent.click(screen.getByRole("link", { name: /Darstellung/ }));
    expect(await screen.findByRole("heading", { level: 2, name: "Darstellung" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /Alle Einstellungen/ })).toHaveAttribute("href", "/einstellungen");
  });

  it("unbekannter Abschnitt führt zur Übersicht", () => {
    setup("/einstellungen/gibtsnicht");
    expect(screen.getByRole("heading", { level: 2, name: "Synchronisierung" })).toBeInTheDocument();
  });

  it("Darstellung: drei Karten, Wahl wird übernommen und gespeichert", () => {
    window.localStorage.clear();
    resetThemeForTests();
    setup("/einstellungen/darstellung");
    const group = screen.getByRole("radiogroup", { name: "Design" });
    expect(within(group).getAllByRole("radio").map((r) => r.textContent)).toEqual(["Hell", "Dunkel", "Wie System"]);
    expect(within(group).getByRole("radio", { name: "Wie System" })).toHaveAttribute("aria-checked", "true");
    fireEvent.click(within(group).getByRole("radio", { name: "Dunkel" }));
    expect(within(group).getByRole("radio", { name: "Dunkel" })).toHaveAttribute("aria-checked", "true");
    expect(window.localStorage.getItem(THEME_KEY)).toBe("dark");
  });

  it("Darstellung: Sprache und Format wählen, Wahl wird im Browser gespeichert", () => {
    window.localStorage.clear();
    resetLocaleForTests();
    setup("/einstellungen/darstellung");
    const language = screen.getByRole("combobox", { name: "Sprache" });
    const format = screen.getByRole("combobox", { name: "Datum und Zahlen" });
    expect(language).toHaveValue("auto");
    expect(within(language).getByRole("option", { name: "Automatisch (Deutsch)" })).toBeInTheDocument();
    expect(within(format).getByRole("option", { name: "Wie die Sprache (24.11.2026 · 1.234,5)" })).toBeInTheDocument();
    expect(within(format).getByRole("option", { name: "11/24/2026 · 1,234.5" })).toBeInTheDocument();
    act(() => {
      fireEvent.change(language, { target: { value: "en" } });
    });
    expect(window.localStorage.getItem(LANGUAGE_KEY)).toBe("en");
    // Englisch ist vollständig: kein Hinweis auf fehlende Texte
    expect(screen.getByRole("combobox", { name: "Language" })).toHaveValue("en");
    expect(screen.queryByText("Not fully translated yet, missing texts appear in German.")).not.toBeInTheDocument();
    expect(screen.getByRole("radiogroup", { name: "Theme" })).toHaveTextContent("Light");
    act(() => {
      fireEvent.change(screen.getByRole("combobox", { name: "Dates and numbers" }), { target: { value: "en-US" } });
    });
    expect(window.localStorage.getItem(FORMAT_KEY)).toBe("en-US");
    act(() => setLocale({ language: "auto", format: "auto" }));
  });
});

describe("Einstellungen → Sportarten", () => {
  it("zeigt nicht zugeordnete Typen zuerst und ordnet zu", async () => {
    setup();
    expect(await screen.findByText(/1 Sporttyp\(en\) mit Aktivitäten sind nicht zugeordnet/)).toBeInTheDocument();
    const rows = screen.getAllByRole("row").slice(1);
    expect(within(rows[0]!).getByText("fitness_equipment")).toBeInTheDocument();
    expect(within(rows[0]!).getByText("189")).toBeInTheDocument();

    chooseOption(rows[0]!, "Sportklasse", "Studio");
    await waitFor(() =>
      expect(writes).toEqual([{ method: "PUT", url: "/api/v1/sport-mappings/fitness_equipment", body: { sport_class: "studio", sub_sport: null } }]),
    );
    // Untertyp der MTB-Zeile entfernen, eigene Zuordnung von cycling löschen
    const mtbRow = screen.getByText("cycling-mountain").closest("tr")!;
    // Eigene Auswahlliste mit Symbolen
    expect(within(mtbRow).getByRole("combobox", { name: "Untertyp" })).toHaveTextContent("MTB");
    chooseOption(mtbRow, "Untertyp", "– kein Untertyp –");
    const cyRow = screen.getByText("cycling", { selector: "span" }).closest("tr")!;
    fireEvent.click(within(cyRow).getByRole("combobox", { name: "Sportklasse" }));
    fireEvent.click(within(within(cyRow).getByRole("listbox")).getAllByRole("option")[0]!);
    await waitFor(() => expect(writes).toHaveLength(3));
    expect(writes[1]).toEqual({ method: "PUT", url: "/api/v1/sport-mappings/cycling-mountain", body: { sport_class: "cycling", sub_sport: null } });
    expect(writes[2]).toEqual({ method: "DELETE", url: "/api/v1/sport-mappings/cycling", body: undefined });
  });

  it("legt einen neuen Typ normalisiert an", async () => {
    setup();
    const field = await screen.findByLabelText("Neuer Typ, z. B. rowing");
    fireEvent.change(field, { target: { value: "Rowing Indoor" } });
    expect(screen.getByText("wird als rowing_indoor gespeichert")).toBeInTheDocument();
    chooseOption(field.closest("form")!, "Sportklasse des neuen Typs", "Studio");
    fireEvent.click(within(field.closest("form")!).getByRole("button", { name: "Hinzufügen" }));
    await waitFor(() =>
      expect(writes).toEqual([{ method: "PUT", url: "/api/v1/sport-mappings/rowing_indoor", body: { sport_class: "studio", sub_sport: null } }]),
    );
  });

  it("Katalog: anlegen, umbenennen, Löschen gesperrt bei Verwendung", async () => {
    setup();
    expect(await screen.findByText("Rennrad", { selector: "span" })).toBeInTheDocument();
    expect(screen.getByText("Gravel", { selector: "span" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "MTB löschen" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Gravel löschen" })).toBeEnabled();

    fireEvent.change(screen.getByLabelText("Neuer Untertyp für Radfahren"), { target: { value: " Bahn " } });
    fireEvent.click(within(screen.getByLabelText("Neuer Untertyp für Radfahren").closest("form")!).getByRole("button", { name: "Hinzufügen" }));
    await waitFor(() => expect(writes[0]).toEqual({ method: "POST", url: "/api/v1/sub-sports", body: { sport_class: "cycling", name: "Bahn" } }));

    fireEvent.click(screen.getByRole("button", { name: "MTB umbenennen" }));
    const name = screen.getByLabelText("Neuer Name");
    expect(name).toHaveValue("MTB");
    fireEvent.change(name, { target: { value: "Mountainbike" } });
    fireEvent.click(screen.getByRole("button", { name: "Speichern" }));
    await waitFor(() => expect(writes[1]).toEqual({ method: "PATCH", url: "/api/v1/sub-sports/cycling/mtb", body: { name: "Mountainbike" } }));

    failNext = { error: { code: "sub_sport_in_use", message: "in use", details: { activity_count: 2, mapping_count: 0 } } };
    fireEvent.click(screen.getByRole("button", { name: "Gravel löschen" }));
    expect(await screen.findByText(/Löschen gesperrt: noch 2 Aktivität\(en\) und 0 Zuordnung\(en\)/)).toBeInTheDocument();
  });

  it("Symbol je Untertyp: Vorgabe nach Schlüssel, eigene Wahl, zurück auf Vorgabe", async () => {
    setup();
    const mtb = await screen.findByRole("button", { name: "Symbol für MTB wählen" });
    expect(mtb.querySelector("[data-glyph]")).toHaveAttribute("data-glyph", "mtb");
    // Eigener Untertyp ohne Wahl: Etikett
    expect(screen.getByRole("button", { name: "Symbol für Gravel wählen" }).querySelector("[data-glyph]")).toHaveAttribute("data-glyph", "tag");
    fireEvent.click(screen.getByRole("button", { name: "Symbol für Gravel wählen" }));
    fireEvent.click(within(screen.getByRole("dialog", { name: "Symbol für Gravel wählen" })).getByRole("button", { name: "MTB" }));
    await waitFor(() => expect(writes).toEqual([{ method: "PATCH", url: expect.stringMatching(/^\/api\/v1\/sub-sports\/cycling\//), body: { icon: "mtb" } }]));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("gewähltes Symbol lässt sich auf die Vorgabe zurücksetzen", async () => {
    subs = [sub({ key: "mtb", icon: "star" })];
    setup();
    const button = await screen.findByRole("button", { name: "Symbol für MTB wählen" });
    expect(button.querySelector("[data-glyph]")).toHaveAttribute("data-glyph", "star");
    fireEvent.click(button);
    fireEvent.click(screen.getByRole("button", { name: "Vorgabe" }));
    await waitFor(() => expect(writes).toEqual([{ method: "PATCH", url: "/api/v1/sub-sports/cycling/mtb", body: { icon: null } }]));
  });

  it("umbenannter eingebauter Untertyp lässt sich zurücksetzen", async () => {
    subs = [sub({ key: "mtb", name: "Mountainbike" })];
    setup();
    fireEvent.click(await screen.findByRole("button", { name: "Mountainbike: ursprüngliche Bezeichnung wiederherstellen" }));
    await waitFor(() => expect(writes).toEqual([{ method: "PATCH", url: "/api/v1/sub-sports/cycling/mtb", body: { name: null } }]));
  });
});
