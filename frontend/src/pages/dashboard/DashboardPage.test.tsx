import { fireEvent, render, screen, within } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter, Route, Routes, useLocation } from "react-router-dom";
import { DashboardPage } from "./DashboardPage";
import { activity, config, json } from "../activities/testData";

// Recharts braucht Layout, das jsdom nicht hat; die Balken prüft dashboardModel.
vi.mock("./DayBars", () => ({
  DayBars: (p: { bars: { label: string; count: number }[]; classes: string[] }) => (
    <div>
      Balken {p.bars.length} Tage, {p.bars.filter((b) => b.count > 0).length} mit Aktivität, {p.classes.join("+")}
    </div>
  ),
}));

const block = (period: string, from: string, to: string, count: number, distance_m: number) => ({
  period,
  from,
  to,
  summaries: [
    {
      period,
      sport_class: "all",
      count,
      distance_m,
      total_time_ms: count * 5_400_000,
      elevation_gain_m: 0,
      activities_without_distance: 0,
      activities_without_elevation: 0,
    },
  ],
  by_day: count ? [{ date: from, sport_class: "cycling", count, distance_m, total_time_ms: count * 5_400_000 }] : [],
});

let feed: ReturnType<typeof activity>[] = [];
let season: unknown = null;
const calls: string[] = [];

beforeEach(() => {
  calls.length = 0;
  season = null;
  feed = [
    activity({ id: "r1", title: "Lauf am Main", sport_class: "running", sub_sport: null, location: "Frankfurt", avg_speed_kmh: 12 }),
    activity({ id: "a1", title: "Runde Wald", start_time: "2026-10-30T09:00:00.000Z" }),
  ];
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string) => {
      calls.push(url);
      if (url.endsWith("/config")) return json(200, config);
      if (url.endsWith("/stats/dashboard")) {
        return json(200, {
          data: {
            week: block("2026-W45", "2026-11-02", "2026-11-08", 2, 50_000),
            month: block("2026-11", "2026-11-01", "2026-11-30", 3, 75_500),
            season,
          },
        });
      }
      return json(200, { data: feed, meta: { total: 57, limit: 10, offset: 0 } });
    }),
  );
});

afterEach(() => vi.unstubAllGlobals());

function Where() {
  const loc = useLocation();
  const files = (loc.state as { files?: File[] } | null)?.files;
  return (
    <div>
      Import mit {files?.map((f) => f.name).join(", ")}
    </div>
  );
}

function setup() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter initialEntries={["/"]}>
        <Routes>
          <Route path="/" element={<DashboardPage />} />
          <Route path="/aktivitaeten/:id" element={<div>Detail</div>} />
          <Route path="/import" element={<Where />} />
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe("Dashboard", () => {
  it("Feed der letzten 10 mit KPI-Strip und Link zur Liste", async () => {
    setup();
    expect(await screen.findByText("Lauf am Main")).toBeInTheDocument();
    expect(calls).toContain("/api/v1/activities?sort=-start_time&limit=10");
    const run = screen.getByText("Lauf am Main").closest("li")!;
    expect(within(run).getByText(/01\.11\.2026 10:00 · Frankfurt/)).toBeInTheDocument();
    expect(within(run).getByText("Pace")).toBeInTheDocument();
    expect(within(run).getByText("5:00")).toBeInTheDocument();
    const ride = screen.getByText("Runde Wald").closest("li")!;
    expect(within(ride).getByText("25,0")).toBeInTheDocument();
    expect(within(ride).getByText("312")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Alle Aktivitäten (57)" })).toHaveAttribute("href", "/aktivitaeten");
  });

  it("Diesen Monat mit Summen und Balken, kein Block „Diese Woche“", async () => {
    setup();
    const month = (await screen.findByText("Diesen Monat · November 2026")).closest("section")!;
    expect(screen.queryByText(/Diese Woche/)).not.toBeInTheDocument();
    expect(screen.queryByText("Balken 7 Tage, 1 mit Aktivität, cycling")).not.toBeInTheDocument();
    expect(within(month).getByText("75,50")).toBeInTheDocument();
    expect(within(month).getByText("Balken 30 Tage, 1 mit Aktivität, cycling")).toBeInTheDocument();
  });

  it("Saison: zuletzt beendete mit Hinweis, Balken je Woche, Link zur gefilterten Liste; ohne Saison kein Block", async () => {
    const view = setup();
    await screen.findByText("Diesen Monat · November 2026");
    expect(screen.queryByText(/^Saison ·/)).not.toBeInTheDocument();
    view.unmount();

    season = {
      ...block("Saison 2026", "2026-03-01", "2026-10-31", 40, 1_234_000),
      season: {
        id: "s26",
        name: "Saison 2026",
        start_date: "2026-03-01",
        end_date: "2026-10-31",
        state: "ended",
        created_at: "2026-11-13T08:00:00.000Z",
        updated_at: "2026-11-13T08:00:00.000Z",
      },
    };
    setup();
    const link = await screen.findByRole("link", { name: "Saison · Saison 2026" });
    expect(link).toHaveAttribute("href", "/aktivitaeten?saison=s26");
    const card = link.closest("section")!;
    expect(within(card).getByText("beendet")).toBeInTheDocument();
    expect(within(card).getByText("01.03.2026 – 31.10.2026")).toBeInTheDocument();
    expect(within(card).getByText("1.234,00")).toBeInTheDocument();
    // KW 9 (Mo 23.02., nur So 01.03. in der Saison) bis KW 44 (26.10.–31.10.)
    expect(within(card).getByText("Balken 36 Tage, 1 mit Aktivität, cycling")).toBeInTheDocument();
  });

  it("Klick auf eine Aktivität öffnet die Detailseite", async () => {
    setup();
    fireEvent.click(await screen.findByText("Runde Wald"));
    expect(await screen.findByText("Detail")).toBeInTheDocument();
  });

  it("abgelegte Dateien gehen an den Importdialog", async () => {
    setup();
    await screen.findByText("Lauf am Main");
    fireEvent.change(screen.getByTestId("file-input"), { target: { files: [new File(["x"], "fahrt.fit")] } });
    expect(await screen.findByText("Import mit fahrt.fit")).toBeInTheDocument();
  });

  it("ohne Aktivitäten ein Hinweis", async () => {
    feed = [];
    setup();
    expect(await screen.findByText(/Noch keine Aktivitäten/)).toBeInTheDocument();
  });
});
