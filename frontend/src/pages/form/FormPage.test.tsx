import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter, Route, Routes, useLocation } from "react-router-dom";
import type { FormResponse } from "../../api/client";
import { FlashProvider } from "../../components/Flash";
import { json } from "../activities/testData";
import { FormPage } from "./FormPage";

const settings = {
  sex_factor: 1.92,
  fitness_days: 42,
  fatigue_days: 7,
  estimate_per_hour: { cycling: 50, ebike: 35, running: 65, skiing: 55, swimming: 60, studio: 40, unknown: 45 },
  curve_model: "standard" as const,
  aerobic_days: 5,
  anaerobic_days: 12,
  rest_bonus_pct: 18,
  foster_surcharge: false,
  foster_surcharge_pct: 15,
  fatigue_weight: 1,
};
const noParts = { fatigue_aerobic: null, fatigue_anaerobic: null, monotony: null, strain: null };

function response(over: Partial<FormResponse["data"]> = {}): FormResponse {
  return {
    data: {
      today: "2026-11-22",
      from: "2026-08-31",
      to: "2026-11-22",
      settings,
      zones_configured: true,
      pending: 0,
      first_date: "2026-11-20",
      current: {
        fitness: 66.4,
        fatigue: 54.2,
        form: 12.2,
        load_7d: 409,
        load_prev_7d: 198,
        fitness_week_ago: 67.1,
        fatigue_week_ago: 47,
        fatigue_aerobic: null,
        fatigue_anaerobic: null,
        monotony: 1.34,
        strain: 548,
        strain_week_ago: 300,
        compare: {
          month_ago: { date: "2026-10-23", fitness: 50, fatigue: 30, form: 5, monotony: 1, strain: 200 },
          week_ago: { date: "2026-11-15", fitness: 67.1, fatigue: 47, form: 20, monotony: 1.2, strain: 300 },
          week_ahead: { date: "2026-11-29", fitness: 60, fatigue: 20, form: 30, monotony: null, strain: null },
        },
      },
      monotony_warn: 2,
      days: [
        { date: "2026-11-19", load: 0, aerobic: 0, anaerobic: 0, estimated: 0, fitness: 0, fatigue: 0, form: 0, ...noParts },
        { date: "2026-11-20", load: 120, aerobic: 100, anaerobic: 10, estimated: 10, fitness: 60, fatigue: 50, form: 10, ...noParts, monotony: 1.1, strain: 132 },
        { date: "2026-11-22", load: 80, aerobic: 80, anaerobic: 0, estimated: 0, fitness: 66.4, fatigue: 54.2, form: 12.2, ...noParts },
      ],
      forecast: { mode: "rest", daily_load: 0, days: [{ date: "2026-11-23", load: 0, fitness: 64.9, fatigue: 47.8, form: 12.2, ...noParts }] },
      ...over,
    },
  };
}

let current: FormResponse;
let urls: string[];
let patches: unknown[];

beforeEach(() => {
  window.localStorage.clear();
  current = response();
  urls = [];
  patches = [];
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string, init?: RequestInit) => {
      urls.push(url);
      if (init?.method === "PATCH") {
        patches.push(JSON.parse(String(init.body)));
        return json(200, { data: settings });
      }
      if (url.startsWith("/api/v1/form?")) return json(200, current);
      if (url.endsWith("/config")) return json(200, { data: { timezone: "Europe/Berlin", sub_sport_catalog: {} } });
      if (url.endsWith("/seasons")) return json(200, { data: [], meta: { total: 0, default_season_id: null, today: "2026-11-22" } });
      if (url.startsWith("/api/v1/stats/periods")) return json(200, { data: { years: [{ year: 2026, count: 3 }], seasons: [] } });
      return json(404, { error: { code: "not_found", message: "nope" } });
    }),
  );
});

function Where() {
  const loc = useLocation();
  return <div data-testid="where">{loc.search}</div>;
}

function setup(path = "/form") {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter initialEntries={[path]}>
        <FlashProvider>
          <Routes>
            <Route
              path="/form"
              element={
                <>
                  <FormPage />
                  <Where />
                </>
              }
            />
          </Routes>
        </FlashProvider>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

const formUrls = () => urls.filter((u) => u.startsWith("/api/v1/form?"));

describe("Seite Form", () => {
  it("zeigt den aktuellen Stand mit Zustand und Vergleich zur Vorwoche", async () => {
    setup();
    expect(await screen.findByText("Frisch")).toBeInTheDocument();
    expect(screen.getByText("66")).toBeInTheDocument();
    expect(screen.getAllByText("+12").length).toBeGreaterThan(0);
    expect(screen.getByText("+211 zur Vorwoche")).toBeInTheDocument();
    // Kachelreihe Belastung (#25): Fitness jetzt, Monat, Woche, +1 Woche
    const load = screen.getByTestId("tiles-load");
    for (const h of ["Aktuell", "Monat", "Woche", "+1 Woche"]) expect(within(load).getByText(h)).toBeInTheDocument();
    expect(within(load).getByLabelText("+16")).toHaveTextContent("16");
    expect(within(load).getByLabelText("−1")).toHaveTextContent("1");
    expect(within(load).getByLabelText("−6")).toHaveTextContent("6");
    // Ermüdung: 54,2 − 30 / − 47 / 20 − 54,2
    expect(within(load).getByLabelText("+24")).toBeInTheDocument();
    expect(within(load).getByLabelText("+7")).toBeInTheDocument();
    expect(within(load).getByLabelText("−34")).toBeInTheDocument();
    // Kachelreihe Form
    const form = screen.getByTestId("tiles-form");
    expect(within(form).getByText("+12")).toBeInTheDocument();
    expect(within(form).getByLabelText("+18")).toBeInTheDocument();
    // Drei Tagesdiagramme und die Wochenbalken
    for (const id of ["form-load-chart", "form-form-chart", "form-foster-chart", "form-weeks"]) expect(screen.getByTestId(id)).toBeInTheDocument();
    expect(screen.getByText(/Gestrichelt: 1 Tage ohne Training/)).toBeInTheDocument();
    // Vorgabe: letzte 12 Wochen, Vorschau Ruhe
    expect(formUrls()[0]).toContain("from=2026-");
    expect(formUrls()[0]).toContain("forecast=rest&forecast_days=28");
    expect(screen.queryByText(/Noch keine HF-Zonen/)).not.toBeInTheDocument();
  });

  it("Vorschau und Zeitraum stehen in der Adresse", async () => {
    setup();
    await screen.findByText("Frisch");
    fireEvent.click(screen.getByRole("button", { name: "Wie bisher" }));
    await waitFor(() => expect(screen.getByTestId("where")).toHaveTextContent("vorschau=wie-bisher"));
    await waitFor(() => expect(formUrls().at(-1)).toContain("forecast=steady"));
    fireEvent.click(screen.getByRole("button", { name: "Aus" }));
    await waitFor(() => expect(formUrls().at(-1)).toContain("forecast_days=0"));
    fireEvent.click(screen.getByRole("button", { name: "Gesamt" }));
    await waitFor(() => expect(formUrls().at(-1)).toContain("from=1900-01-01"));
    expect(screen.getByTestId("where")).toHaveTextContent("zeitraum=alle");
  });

  it("Hinweise ohne Zonen und während der Berechnung", async () => {
    current = response({ zones_configured: false, pending: 12 });
    setup();
    expect(await screen.findByText(/Noch keine HF-Zonen eingetragen/)).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /HF-Zonen auf der Kontoseite/ })).toHaveAttribute("href", "/konto");
    expect(screen.getByRole("status")).toHaveTextContent("noch 12 Aktivitäten");
  });

  it("Monotonie und Strain mit Warnung, Zwei-Kurven-Modell zeigt die Teile der Ermüdung", async () => {
    current = response({
      settings: { ...settings, curve_model: "two_curve", foster_surcharge: true },
      current: { ...response().data.current, monotony: 2.6, fatigue_aerobic: 40.1, fatigue_anaerobic: 14.1 },
    });
    setup();
    expect(await screen.findByText("Monoton – mehr Abwechslung")).toBeInTheDocument();
    expect(screen.getByText("2,60")).toBeInTheDocument();
    expect(screen.getByText("548")).toBeInTheDocument();
    expect(screen.getByText("Aufschlag 15 % auf die Ermüdung aktiv")).toBeInTheDocument();
    // aerob/anaerob ist ausgeblendet, bis der Schalter es einblendet; die Wahl merkt sich der Browser
    expect(screen.queryByText("Ermüdung aerob")).not.toBeInTheDocument();
    fireEvent.click(screen.getByLabelText("aerob/anaerob einblenden"));
    expect(screen.getByText("Ermüdung aerob")).toBeInTheDocument();
    expect(window.localStorage.getItem("trackcraft.form.fatigueParts")).toBe("1");
  });

  it("der Schalter aerob/anaerob ist nach dem Neuladen noch an", async () => {
    window.localStorage.setItem("trackcraft.form.fatigueParts", "1");
    current = response({
      settings: { ...settings, curve_model: "two_curve" },
      current: { ...response().data.current, fatigue_aerobic: 40.1, fatigue_anaerobic: 14.1 },
    });
    setup();
    expect(await screen.findByText("Ermüdung aerob")).toBeInTheDocument();
    expect(screen.getByLabelText("aerob/anaerob einblenden")).toBeChecked();
  });

  it("Standardmodell: Monotonie unauffällig, Strain mit Vergleich", async () => {
    setup();
    expect(await screen.findByText("abwechslungsreich")).toBeInTheDocument();
    // Strain 548 gegenüber 300 vor einer Woche
    expect(within(screen.getByTestId("tiles-foster")).getByLabelText("+248")).toBeInTheDocument();
    expect(screen.queryByText("Ermüdung aerob")).not.toBeInTheDocument();
  });

  it("Modell umschalten schickt die Werte des Zwei-Kurven-Modells", async () => {
    setup();
    await screen.findByText("Frisch");
    fireEvent.click(screen.getByRole("button", { name: /Einstellungen der Formkurve/ }));
    fireEvent.click(await screen.findByRole("radio", { name: "Zwei-Kurven" }));
    expect(screen.queryByLabelText("Ermüdung, Zeitkonstante (Tage)")).not.toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("Ermüdung anaerob, Zeitkonstante (Tage)"), { target: { value: "14" } });
    fireEvent.click(screen.getByLabelText("Bei Monotonie über 2,0 die Ermüdung erhöhen"));
    fireEvent.click(screen.getByRole("button", { name: "Speichern" }));
    await waitFor(() => expect(patches).toHaveLength(1));
    expect(patches[0]).toMatchObject({ curve_model: "two_curve", aerobic_days: 5, anaerobic_days: 14, rest_bonus_pct: 18, foster_surcharge: true });
  });

  it("Einstellungen der Formkurve speichern nur gültige Werte", async () => {
    setup();
    await screen.findByText("Frisch");
    fireEvent.click(screen.getByRole("button", { name: /Einstellungen der Formkurve/ }));
    const fatigue = await screen.findByLabelText("Ermüdung, Zeitkonstante (Tage)");
    fireEvent.change(fatigue, { target: { value: "50" } });
    expect(screen.getByRole("alert")).toHaveTextContent("Zeitkonstanten");
    const save = screen.getByRole("button", { name: "Speichern" });
    expect(save).toBeDisabled();
    fireEvent.change(fatigue, { target: { value: "10" } });
    fireEvent.change(screen.getByLabelText("Studio"), { target: { value: "30" } });
    fireEvent.click(save);
    await waitFor(() => expect(patches).toHaveLength(1));
    expect(patches[0]).toMatchObject({ fitness_days: 42, fatigue_days: 10, estimate_per_hour: { studio: 30, cycling: 50 } });
  });

  it("Vorlage SportTracks-ähnlich setzt 21/12 und Gewicht 0,75; eigene Werte heben die Vorlage auf (#24)", async () => {
    setup();
    await screen.findByText("Frisch");
    fireEvent.click(screen.getByRole("button", { name: /Einstellungen der Formkurve/ }));
    const presets = await screen.findByRole("radiogroup", { name: "Vorlage" });
    expect(within(presets).getByRole("radio", { name: "Standard" })).toHaveAttribute("aria-checked", "true");
    fireEvent.click(within(presets).getByRole("radio", { name: "SportTracks-ähnlich" }));
    expect(within(presets).getByRole("radio", { name: "SportTracks-ähnlich" })).toHaveAttribute("aria-checked", "true");
    expect(screen.getByLabelText("Fitness, Zeitkonstante (Tage)")).toHaveValue("21");
    expect(screen.getByLabelText("Ermüdung, Zeitkonstante (Tage)")).toHaveValue("12");
    expect(screen.getByLabelText("Gewicht der Ermüdung")).toHaveValue("0,75");
    fireEvent.click(screen.getByRole("button", { name: "Speichern" }));
    await waitFor(() => expect(patches).toHaveLength(1));
    expect(patches[0]).toMatchObject({ curve_model: "standard", fitness_days: 21, fatigue_days: 12, fatigue_weight: 0.75 });
  });

  it("Gewicht der Ermüdung: Komma, Bereich 0,25 bis 3", async () => {
    setup();
    await screen.findByText("Frisch");
    fireEvent.click(screen.getByRole("button", { name: /Einstellungen der Formkurve/ }));
    const weight = await screen.findByLabelText("Gewicht der Ermüdung");
    const save = screen.getByRole("button", { name: "Speichern" });
    fireEvent.change(weight, { target: { value: "4" } });
    expect(save).toBeDisabled();
    fireEvent.change(weight, { target: { value: "1,5" } });
    expect(save).toBeEnabled();
    expect(screen.getByText("Eigene Werte")).toBeInTheDocument();
    fireEvent.click(save);
    await waitFor(() => expect(patches).toHaveLength(1));
    expect(patches[0]).toMatchObject({ fatigue_weight: 1.5 });
  });
});
