import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter, Route, Routes, useLocation } from "react-router-dom";
import type { Season, StatisticsResponse } from "../../api/client";
import { FlashProvider } from "../../components/Flash";
import { config, json } from "../activities/testData";
import { StatisticsPage } from "./StatisticsPage";

// Recharts braucht Layout, das jsdom nicht hat; die Balken prüft statsModel.
vi.mock("./StatsCharts", () => ({
  StackedBars: (p: { bars: { label: string }[]; unit: string }) => (
    <div>
      Balken {p.bars.length} ({p.unit})
    </div>
  ),
  ShareRing: (p: { parts: { sport: string }[] }) => <div>Ring {p.parts.map((x) => x.sport).join("+")}</div>,
}));

const season = (p: Partial<Season>): Season => ({
  id: "s",
  name: "Saison",
  start_date: "2026-01-01",
  end_date: "2026-12-31",
  state: "ended",
  created_at: "2026-11-13T08:00:00.000Z",
  updated_at: "2026-11-13T08:00:00.000Z",
  ...p,
});

let seasons: Season[] = [];
let defaultId: string | null = null;
const calls: string[] = [];
const writes: { method: string; url: string; body: unknown }[] = [];

const summary = (sport_class: string, count: number) => ({
  sport_class,
  count,
  distance_m: count * 40_000,
  total_time_ms: count * 5_400_000,
  elevation_gain_m: count * 500,
  activities_without_distance: 0,
  activities_without_elevation: 0,
  segment_success_count: count,
});

function stats(): StatisticsResponse["data"] {
  return {
    from: null,
    to: null,
    first_day: "2026-03-02",
    last_day: "2026-04-10",
    summaries: [summary("cycling", 3), summary("ebike", 0), summary("running", 1), summary("all", 4)] as StatisticsResponse["data"]["summaries"],
    by_month: [
      { month: "2026-03", sport_class: "cycling", count: 3, distance_m: 120_000, total_time_ms: 16_200_000, elevation_gain_m: 1500 },
      { month: "2026-04", sport_class: "running", count: 1, distance_m: 10_000, total_time_ms: 3_000_000, elevation_gain_m: 0 },
    ],
    by_week: [{ week: "2026-W10", week_start: "2026-03-02", sport_class: "cycling", count: 3, distance_m: 120_000, total_time_ms: 16_200_000 }],
    records: [
      { sport_class: "cycling", kind: "distance", value: 61_234, equaled_count: 0, activity: { id: "a1", title: "Große Runde", start_time: "2026-03-05T09:00:00.000Z", sub_sport: null } },
      { sport_class: "running", kind: "speed", value: 12, equaled_count: 1, activity: { id: "r1", title: "Lauf am Fluss", start_time: "2026-04-10T07:00:00.000Z", sub_sport: null } },
    ],
  };
}

function Where() {
  const l = useLocation();
  return <div data-testid="where">{l.pathname + l.search}</div>;
}

function setup(path = "/statistik") {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter initialEntries={[path]}>
        <FlashProvider>
          <Routes>
            <Route path="/statistik" element={<StatisticsPage />} />
            <Route path="/aktivitaeten/:id" element={<div>Aktivität</div>} />
          </Routes>
          <Where />
        </FlashProvider>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  calls.length = 0;
  writes.length = 0;
  seasons = [
    season({ id: "s1", name: "Herbst 2026", start_date: "2026-09-01", end_date: "2026-11-30", state: "current" }),
    season({ id: "s2", name: "Sommer 2026", start_date: "2026-05-01", end_date: "2026-08-31" }),
    season({ id: "s3", name: "Frühjahr 2026", start_date: "2026-03-01", end_date: "2026-04-30" }),
    season({ id: "s4", name: "Saison 2025", start_date: "2025-03-01", end_date: "2025-10-31" }),
    season({ id: "s5", name: "Saison 2024", start_date: "2024-03-01", end_date: "2024-10-31" }),
    season({ id: "s6", name: "Saison 2023", start_date: "2023-03-01", end_date: "2023-10-31" }),
  ];
  defaultId = "s1";
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string, init?: RequestInit) => {
      const method = init?.method ?? "GET";
      if (url.endsWith("/config")) return json(200, config);
      if (method !== "GET") {
        const body: unknown = init?.body ? JSON.parse(String(init.body)) : null;
        writes.push({ method, url, body });
        if (method === "DELETE") return new Response(null, { status: 204 });
        if (url.endsWith("/seasons") && (body as { name?: string }).name === "Herbst 2026") {
          return json(409, { error: { code: "season_name_taken", message: "taken" } });
        }
        return json(method === "POST" ? 201 : 200, { data: season({ id: "neu", ...(body as object) }) });
      }
      calls.push(url);
      if (url.endsWith("/seasons")) return json(200, { data: seasons, meta: { total: seasons.length, default_season_id: defaultId, today: "2026-11-13" } });
      if (url.includes("/stats/periods")) {
        return json(200, {
          data: {
            seasons: seasons.map((s, i) => ({ season_id: s.id, count: 10 + i, distance_m: 400_000, total_time_ms: 72_000_000 })),
            years: [2026, 2025, 2024].map((year) => ({ year, count: year - 2000, distance_m: 1_000_000, total_time_ms: 36_000_000 })),
          },
        });
      }
      if (url.includes("/stats?") || url.endsWith("/stats")) return json(200, { data: stats(), meta: { calculation: { status: "current", version_id: "v", published_at: null } } });
      throw new Error(`unerwartet: ${url}`);
    }),
  );
});

afterEach(() => vi.unstubAllGlobals());

const lastStats = () => calls.filter((c) => c.includes("/stats?") || c.endsWith("/stats")).at(-1);

describe("Statistikseite", () => {
  it("Vorgabesaison zuerst und gewählt, bis vier weitere, Umschalter für alle", async () => {
    setup();
    await waitFor(() => expect(screen.getAllByTestId("season-tile")).toHaveLength(5));
    const tiles = screen.getAllByTestId("season-tile");
    expect(within(tiles[0]!).getByText("Herbst 2026")).toBeInTheDocument();
    expect(within(tiles[0]!).getByText("aktuell")).toBeInTheDocument();
    expect(within(tiles[0]!).getByLabelText("läuft")).toBeInTheDocument();
    expect(within(tiles[1]!).getByLabelText("beendet")).toBeInTheDocument();
    expect(within(tiles[0]!).getByRole("button", { name: "Zeitraum Herbst 2026 wählen" })).toHaveAttribute("aria-pressed", "true");
    expect(within(tiles[0]!).getByText("10")).toBeInTheDocument();
    await waitFor(() => expect(lastStats()).toBe("/api/v1/stats?from=2026-09-01&to=2026-11-30"));
    expect(screen.getByTestId("selection-title").textContent).toBe("Herbst 2026 · 01.09.2026 – 30.11.2026");

    fireEvent.click(screen.getByRole("button", { name: "Alle Saisons (6)" }));
    expect(screen.getAllByTestId("season-tile")).toHaveLength(6);
    fireEvent.click(screen.getByRole("button", { name: "Weniger Saisons" }));
    expect(screen.getAllByTestId("season-tile")).toHaveLength(5);
  });

  it("Kachel, Jahr und Von/Bis wählen den Zeitraum; Sportklasse und Untertyp filtern; Adresse merkt sich alles", async () => {
    setup();
    fireEvent.click(await screen.findByRole("button", { name: "Zeitraum Sommer 2026 wählen" }));
    expect(screen.getByTestId("where").textContent).toBe("/statistik?saison=s2");
    await waitFor(() => expect(lastStats()).toBe("/api/v1/stats?from=2026-05-01&to=2026-08-31"));

    fireEvent.click(screen.getByRole("button", { name: "Jahr 2026 wählen" }));
    expect(screen.getByTestId("where").textContent).toBe("/statistik?jahr=2026");
    expect(screen.getByRole("button", { name: "Späteres Jahr" })).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "Früheres Jahr" }));
    await waitFor(() => expect(lastStats()).toBe("/api/v1/stats?from=2025-01-01&to=2025-12-31"));
    expect(screen.getByText("25")).toBeInTheDocument(); // Kurzsumme der Jahres-Kachel

    fireEvent.click(screen.getByRole("button", { name: "Radfahren" }));
    // Untertyp im Pfeilmenü des geteilten Knopfs
    fireEvent.click(await screen.findByRole("button", { name: "Untertyp Radfahren wählen" }));
    fireEvent.click(within(screen.getByRole("menu")).getByRole("menuitemradio", { name: "MTB" }));
    expect(screen.getByRole("button", { name: "Untertyp Radfahren wählen" })).toHaveTextContent("MTB");
    expect(screen.getByTestId("where").textContent).toBe("/statistik?jahr=2025&sport=cycling&sub=mtb");
    await waitFor(() => expect(lastStats()).toBe("/api/v1/stats?from=2025-01-01&to=2025-12-31&sport_class=cycling&sub_sport=mtb"));
    expect(calls).toContain("/api/v1/stats/periods?sport_class=cycling&sub_sport=mtb");

    // Von–Bis zeigt das gewählte Jahr und bleibt änderbar
    expect(screen.getByLabelText("Von")).toHaveValue("2025-01-01");
    expect(screen.getByLabelText("Bis")).toHaveValue("2025-12-31");
    fireEvent.change(screen.getByLabelText("Von"), { target: { value: "2025-03-01" } });
    expect(screen.getByTestId("where").textContent).toBe("/statistik?von=2025-03-01&bis=2025-12-31&sport=cycling&sub=mtb");
    expect(screen.getByRole("button", { name: "Jahr 2026 wählen" })).toHaveAttribute("aria-pressed", "false");
    await waitFor(() => expect(lastStats()).toBe("/api/v1/stats?from=2025-03-01&to=2025-12-31&sport_class=cycling&sub_sport=mtb"));

    // Saison füllt Von–Bis ebenso
    fireEvent.click(screen.getByRole("button", { name: "Zeitraum Sommer 2026 wählen" }));
    expect(screen.getByLabelText("Von")).toHaveValue("2026-05-01");
    expect(screen.getByLabelText("Bis")).toHaveValue("2026-08-31");

    fireEvent.click(screen.getByRole("button", { name: "Filter zurücksetzen" }));
    expect(screen.getByTestId("where").textContent).toBe("/statistik");
  });

  it("Kennzahlen, Diagramme und Rekorde mit Pace und Link zur Aktivität", async () => {
    setup("/statistik?von=2026-03-01&bis=2026-04-30");
    expect(await screen.findByText("Bestzeiten")).toBeInTheDocument();
    expect(screen.getByText("160")).toBeInTheDocument(); // 4 × 40 km
    expect(screen.getAllByText("Ring cycling+running")).toHaveLength(2); // Distanz und Zeit
    expect(screen.getAllByText(/^Balken/)).toHaveLength(3);
    const cyc = screen.getByTestId("records-cycling");
    expect(within(cyc).getByText("Längste Distanz")).toBeInTheDocument();
    expect(within(cyc).getByText("61,23")).toBeInTheDocument();
    const run = screen.getByTestId("records-running");
    expect(within(run).getByText("Schnellste Pace")).toBeInTheDocument();
    expect(within(run).getByText("5:00")).toBeInTheDocument();
    expect(within(run).getByRole("link", { name: /Lauf am Fluss · 1× eingestellt/ })).toHaveAttribute("href", "/aktivitaeten/r1");
    expect(screen.queryByTestId("records-ebike")).not.toBeInTheDocument();
  });

  it("Neue Saison anlegen, Konflikt am Feld, Ändern und Löschen mit Bestätigung", async () => {
    setup("/statistik?saison=s2");
    fireEvent.click(await screen.findByRole("button", { name: "Neue Saison" }));
    let dialog = screen.getByRole("dialog", { name: "Neue Saison" });
    expect(within(dialog).queryByRole("button", { name: "Löschen" })).not.toBeInTheDocument();
    fireEvent.change(within(dialog).getByPlaceholderText("z. B. Saison 2026"), { target: { value: "Herbst 2026" } });
    fireEvent.change(within(dialog).getByLabelText("Erster Tag"), { target: { value: "2027-03-01" } });
    fireEvent.change(within(dialog).getByLabelText("Letzter Tag"), { target: { value: "2027-10-31" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Speichern" }));
    expect(await within(dialog).findByText("Eine andere Saison hat schon diesen Namen.")).toBeInTheDocument();
    fireEvent.change(within(dialog).getByPlaceholderText("z. B. Saison 2026"), { target: { value: "Saison 2027" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Speichern" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(writes.at(-1)).toEqual({ method: "POST", url: "/api/v1/seasons", body: { name: "Saison 2027", start_date: "2027-03-01", end_date: "2027-10-31" } });

    fireEvent.click(screen.getByRole("button", { name: "Sommer 2026 ändern" }));
    dialog = screen.getByRole("dialog", { name: "Saison ändern" });
    expect(screen.getByTestId("where").textContent).toBe("/statistik?saison=s2");
    fireEvent.click(within(dialog).getByRole("button", { name: "Löschen" }));
    expect(writes.filter((w) => w.method === "DELETE")).toHaveLength(0);
    fireEvent.click(within(dialog).getByRole("button", { name: "Wirklich löschen?" }));
    await waitFor(() => expect(writes.at(-1)).toEqual({ method: "DELETE", url: "/api/v1/seasons/s2", body: null }));
    // Gelöschte Saison war gewählt: zurück zur Vorgabe
    await waitFor(() => expect(screen.getByTestId("where").textContent).toBe("/statistik"));
  });

  it("ohne Saisons: nur „+ Neue Saison“, Zeitraum ist das laufende Jahr", async () => {
    seasons = [];
    defaultId = null;
    setup();
    expect(await screen.findByRole("button", { name: "Neue Saison" })).toBeInTheDocument();
    await waitFor(() => expect(lastStats()).toBe("/api/v1/stats?from=2026-01-01&to=2026-12-31"));
    expect(screen.queryAllByTestId("season-tile")).toHaveLength(0);
    expect(screen.getByTestId("selection-title").textContent).toBe("Jahr 2026");
  });
});
