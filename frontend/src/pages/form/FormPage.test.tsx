import { fireEvent, render, screen, waitFor } from "@testing-library/react";
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
};

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
      current: { fitness: 66.4, fatigue: 54.2, form: 12.2, load_7d: 409, load_prev_7d: 198, fitness_week_ago: 67.1, fatigue_week_ago: 47 },
      days: [
        { date: "2026-11-19", load: 0, aerobic: 0, anaerobic: 0, estimated: 0, fitness: 0, fatigue: 0, form: 0 },
        { date: "2026-11-20", load: 120, aerobic: 100, anaerobic: 10, estimated: 10, fitness: 60, fatigue: 50, form: 10 },
        { date: "2026-11-22", load: 80, aerobic: 80, anaerobic: 0, estimated: 0, fitness: 66.4, fatigue: 54.2, form: 12.2 },
      ],
      forecast: { mode: "rest", daily_load: 0, days: [{ date: "2026-11-23", load: 0, fitness: 64.9, fatigue: 47.8, form: 12.2 }] },
      ...over,
    },
  };
}

let current: FormResponse;
let urls: string[];
let patches: unknown[];

beforeEach(() => {
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
    expect(screen.getByText("+12")).toBeInTheDocument();
    expect(screen.getByText("+211 zur Vorwoche")).toBeInTheDocument();
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
});
