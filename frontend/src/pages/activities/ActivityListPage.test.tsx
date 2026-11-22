import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter, Route, Routes, useLocation } from "react-router-dom";
import { ActivityListPage } from "./ActivityListPage";
import type { Season } from "../../api/client";
import { activity, config, json, summary } from "./testData";

const calls: string[] = [];
let total = 1;
let seasons: Season[] = [];

function Where() {
  const l = useLocation();
  return <div data-testid="where">{l.pathname + l.search}</div>;
}

function setup(path = "/aktivitaeten") {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter initialEntries={[path]}>
        <Routes>
          <Route path="/aktivitaeten" element={<ActivityListPage />} />
          <Route path="/aktivitaeten/:id" element={<div>Detail</div>} />
        </Routes>
        <Where />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  calls.length = 0;
  total = 1;
  seasons = [];
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string) => {
      if (url.endsWith("/config")) return json(200, config);
      if (url.endsWith("/segments/r1")) return json(200, { data: { id: "r1", name: "Hausrunde", kind: "route" }, meta: {} });
      if (url.includes("/segments")) throw new Error(`unerwartet: ${url}`);
      if (url.endsWith("/seasons")) return json(200, { data: seasons, meta: { total: seasons.length, default_season_id: null, today: "2026-11-12" } });
      if (url.endsWith("/stats/periods")) {
        const y = (year: number, count: number) => ({ year, count, distance_m: 0, total_time_ms: 0 });
        return json(200, { data: { seasons: [], years: [y(2019, 3), y(2018, 0)] } });
      }
      calls.push(url);
      const offset = Number(new URL(url, "http://x").searchParams.get("offset"));
      const running = url.includes("sport_class=running");
      const a = activity({
        id: `a${offset}`,
        title: running ? "Lauf am Fluss" : `Runde ${offset}`,
        sport_class: running ? "running" : "cycling",
        avg_speed_kmh: running ? 10 : 25,
      });
      return json(200, {
        data: [a],
        meta: { total, limit: 50, offset, summary: summary({ count: total, mixed_sport_classes: total > 1 }) },
      });
    }),
  );
});

describe("Aktivitätenliste", () => {
  it("zeigt Kennzahlen, Zeile und Summenzeile, Vorgabe letzte 12 Wochen", async () => {
    setup();
    expect(await screen.findByText("Runde 0")).toBeInTheDocument();
    expect(calls[0]).toMatch(/\/activities\?from=\d{4}-\d\d-\d\d&sort=-start_time&limit=50&offset=0$/);
    expect(screen.getByText("Summe · 1 Aktivitäten")).toBeInTheDocument();
    expect(screen.getAllByText("25,0 km/h").length).toBe(2);
    expect(screen.getByText("Letzte 12 Wochen", { selector: "div" })).toBeInTheDocument();
    expect(screen.getAllByText("312 m").length).toBe(2);
  });

  it("Gesamt, Sportklasse und Untertyp über das Pfeilmenü, sortieren", async () => {
    setup();
    await screen.findByText("Runde 0");
    expect(screen.getByRole("button", { name: "Letzte 12 Wochen" })).toHaveAttribute("aria-pressed", "true");
    fireEvent.click(screen.getByRole("button", { name: "Gesamt" }));
    await waitFor(() => expect(screen.getByTestId("where").textContent).toBe("/aktivitaeten?zeitraum=alle"));
    // Name wählt die ganze Klasse
    fireEvent.click(screen.getByRole("button", { name: "Radfahren" }));
    await waitFor(() => expect(calls.at(-1)).toContain("sport_class=cycling&sort"));
    // Pfeil öffnet die Untertypen, Wahl steht rechts vom Trenner
    fireEvent.click(screen.getByRole("button", { name: "Untertyp Radfahren wählen" }));
    fireEvent.click(within(screen.getByRole("menu")).getByRole("menuitemradio", { name: "MTB" }));
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
    expect(await screen.findByRole("button", { name: "Radfahren" })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByRole("button", { name: "Untertyp Radfahren wählen" })).toHaveTextContent("MTB");
    fireEvent.click(await screen.findByText("Distanz", { selector: "th" }));
    await waitFor(() =>
      expect(calls.at(-1)).toBe("/api/v1/activities?sport_class=cycling&sub_sport=mtb&sort=-distance_m&limit=50&offset=0"),
    );
    // „Alle Radfahren“ nimmt den Untertyp zurück; direkt aus dem Menü einer anderen Klasse geht auch
    fireEvent.click(screen.getByRole("button", { name: "Untertyp Radfahren wählen" }));
    expect(screen.getByRole("menuitemradio", { name: "MTB" })).toHaveAttribute("aria-checked", "true");
    fireEvent.click(screen.getByRole("menuitemradio", { name: "Alle Radfahren" }));
    await waitFor(() => expect(screen.getByTestId("where").textContent).toBe("/aktivitaeten?zeitraum=alle&sport=cycling&sort=-distance_m"));
  });

  it("Jahr und Von–Bis", async () => {
    setup();
    await screen.findByText("Runde 0");
    const thisYear = new Date().getFullYear();
    // geteilter Knopf „Jahr | 2026 ▾“
    fireEvent.click(await screen.findByRole("button", { name: "Jahr" }));
    await waitFor(() => expect(calls.at(-1)).toContain(`from=${thisYear}-01-01&to=${thisYear}-12-31`));
    expect(screen.getByRole("button", { name: "Jahr" })).toHaveAttribute("aria-pressed", "true");
    // Menü enthält Jahre mit Aktivitäten und das laufende
    await waitFor(() => expect(screen.getByRole("button", { name: "Jahr wählen" })).toBeInTheDocument());
    fireEvent.click(screen.getByRole("button", { name: "Jahr wählen" }));
    expect(screen.getAllByRole("menuitemradio").map((o) => o.textContent)).toEqual([String(thisYear), "2019"]);
    fireEvent.click(screen.getByRole("menuitemradio", { name: "2019" }));
    await waitFor(() => expect(screen.getByTestId("where").textContent).toBe("/aktivitaeten?jahr=2019"));
    expect(screen.getByText("Jahr 2019", { selector: "div" })).toBeInTheDocument();

    // Von–Bis übernimmt das Jahr und lässt sich anpassen
    fireEvent.click(screen.getByRole("button", { name: "Von – Bis" }));
    await waitFor(() => expect(screen.getByTestId("where").textContent).toBe("/aktivitaeten?zeitraum=frei&von=2019-01-01&bis=2019-12-31"));
    fireEvent.change(screen.getByLabelText("Bis"), { target: { value: "2019-06-30" } });
    await waitFor(() => expect(calls.at(-1)).toBe("/api/v1/activities?from=2019-01-01&to=2019-06-30&sort=-start_time&limit=50&offset=0"));
    expect(screen.getByText("01.01.2019 – 30.06.2019")).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("Von"), { target: { value: "" } });
    await waitFor(() => expect(calls.at(-1)).toBe("/api/v1/activities?to=2019-06-30&sort=-start_time&limit=50&offset=0"));
    expect(screen.getByText("bis 30.06.2019")).toBeInTheDocument();
  });

  it("Laufen zeigt Pace, gemischte Summen als gemischt", async () => {
    total = 2;
    setup("/aktivitaeten?sport=running");
    expect(await screen.findByText("Lauf am Fluss")).toBeInTheDocument();
    expect(screen.getByText("6:00 /km")).toBeInTheDocument();
    expect(screen.getByText("gemischt")).toBeInTheDocument();
  });

  it("Mehr laden holt die nächste Seite", async () => {
    total = 60;
    setup();
    await screen.findByText("Runde 0");
    fireEvent.click(screen.getByRole("button", { name: /Mehr laden/ }));
    expect(await screen.findByText("Runde 1")).toBeInTheDocument();
    expect(calls.at(-1)).toContain("offset=1");
  });

  it("Suche geht nach der Tipp-Pause an die API", async () => {
    setup();
    await screen.findByText("Runde 0");
    fireEvent.change(screen.getByRole("searchbox"), { target: { value: "Wald" } });
    await waitFor(() => expect(calls.at(-1)).toContain("q=Wald"), { timeout: 2000 });
    expect(screen.getByTestId("where").textContent).toBe("/aktivitaeten?q=Wald");
  });

  it("Streckenfilter aus der Adresse steht als entfernbarer Filter mit Namen da", async () => {
    setup("/aktivitaeten?strecke=r1");
    await screen.findByText("Runde 0");
    expect(calls.at(-1)).toContain("route_id=r1");
    expect(await screen.findByText("Strecke: Hausrunde")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Streckenfilter entfernen" }));
    await waitFor(() => expect(calls.at(-1)).not.toContain("route_id"));
    expect(screen.getByTestId("where").textContent).toBe("/aktivitaeten");
  });

  it("Saisonfilter: Zeitraum der Saison, schließt die letzten 12 Wochen aus, unbekannte Saison fällt weg", async () => {
    seasons = [
      {
        id: "s26",
        name: "Saison 2026",
        start_date: "2026-03-01",
        end_date: "2026-10-31",
        state: "ended",
        created_at: "2026-11-13T08:00:00.000Z",
        updated_at: "2026-11-13T08:00:00.000Z",
      },
    ];
    seasons.push({ ...seasons[0]!, id: "s25", name: "Saison 2025", start_date: "2025-03-01", end_date: "2025-10-31" });
    setup();
    await screen.findByText("Runde 0");
    // geteilter Knopf: rechts Name und Zustand als Punkt
    const button = await screen.findByRole("button", { name: "Saison" });
    expect(button).toHaveAttribute("title", "Saison 2026 · beendet");
    expect(screen.getByRole("button", { name: "Saison wählen" })).toHaveTextContent("Saison 2026");
    fireEvent.click(button);
    await waitFor(() => expect(calls.at(-1)).toBe("/api/v1/activities?from=2026-03-01&to=2026-10-31&sort=-start_time&limit=50&offset=0"));
    expect(screen.getByTestId("where").textContent).toBe("/aktivitaeten?saison=s26");
    expect(screen.getByRole("button", { name: "Letzte 12 Wochen" })).toHaveAttribute("aria-pressed", "false");
    expect(screen.getByRole("button", { name: "Saison" })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByText("01.03.2026 – 31.10.2026")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Saison wählen" }));
    // Nur Auswahl, keine Verwaltung (Uli 13.11.2026: Saisons entstehen auf der Statistikseite)
    expect(screen.getAllByRole("menuitemradio").map((o) => o.textContent)).toEqual(["Saison 2026", "Saison 2025"]);
    fireEvent.click(screen.getByRole("menuitemradio", { name: "Saison 2025" }));
    await waitFor(() => expect(calls.at(-1)).toContain("from=2025-03-01&to=2025-10-31"));
  });

  it("gelöschte Saison aus der Adresse wird verworfen", async () => {
    setup("/aktivitaeten?saison=weg");
    await waitFor(() => expect(screen.getByTestId("where").textContent).toBe("/aktivitaeten?zeitraum=alle"));
    await waitFor(() => expect(calls.at(-1)).toBe("/api/v1/activities?sort=-start_time&limit=50&offset=0"));
  });

  it("ohne Saisons keine Saisonauswahl", async () => {
    setup();
    await screen.findByText("Runde 0");
    expect(screen.queryByText(/Saison/)).not.toBeInTheDocument();
  });

  it("ohne Streckenfilter keine Streckenauswahl und keine Abfrage der Strecken", async () => {
    setup();
    await screen.findByText("Runde 0");
    expect(screen.queryByText(/^Strecke:/)).not.toBeInTheDocument();
    expect(screen.queryByLabelText("Strecke")).not.toBeInTheDocument();
  });

  it("Klick auf eine Zeile öffnet die Detailseite", async () => {
    setup();
    const row = (await screen.findByText("Runde 0")).closest("tr")!;
    fireEvent.click(within(row).getByText("312 m"));
    expect(await screen.findByText("Detail")).toBeInTheDocument();
  });
});
