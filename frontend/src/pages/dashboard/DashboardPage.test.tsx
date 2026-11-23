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
let units: unknown[] = [];
const calls: string[] = [];

const unit = (id: string, date: string, title: string, duration_min: number, extra: Record<string, unknown> = {}) => ({
  id,
  date,
  sport_class: "cycling",
  sub_sport: null,
  title,
  note: null,
  mode: "target",
  duration_min,
  target_load: null,
  intensive_pct: 0,
  zone_minutes: null,
  status: "planned",
  activity_id: null,
  load: { total: 0, aerobic: 0, anaerobic: 0, minutes: duration_min, method: "target" },
  ...extra,
});

beforeEach(() => {
  calls.length = 0;
  units = [];
  feed = [
    activity({ id: "r1", title: "Lauf am Main", sport_class: "running", sub_sport: null, location: "Frankfurt", avg_speed_kmh: 12, has_position: false }),
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
            year: block("2026", "2026-01-01", "2026-12-31", 40, 1_234_000),
          },
        });
      }
      if (url.includes("/plan?")) return json(200, { data: { today: "2026-11-04", units, goal: null, zones_configured: true, assistant: null } });
      return json(200, { data: feed, meta: { total: 57, limit: 5, offset: 0 } });
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
  it("Feed der letzten 5 als Kacheln mit Kennzahlen, Mini-Karte nur mit GPS, Link zur Liste", async () => {
    setup();
    expect(await screen.findByText("Lauf am Main")).toBeInTheDocument();
    expect(calls).toContain("/api/v1/activities?sort=-start_time&limit=5");
    const run = screen.getByText("Lauf am Main").closest("li")!;
    expect(within(run).getByText(/01\.11\.2026 10:00 · Frankfurt/)).toBeInTheDocument();
    expect(within(run).getByText("Pace")).toBeInTheDocument();
    expect(within(run).getByText("5:00")).toBeInTheDocument();
    expect(within(run).queryByTestId("mini-map")).not.toBeInTheDocument();
    const ride = screen.getByText("Runde Wald").closest("li")!;
    expect(within(ride).getByTestId("mini-map")).toBeInTheDocument();
    expect(within(ride).getByText("25,0")).toBeInTheDocument();
    expect(within(ride).getByText("312")).toBeInTheDocument();
    // ohne IntersectionObserver (jsdom) lädt keine Spur
    expect(calls.some((c) => c.includes("/track"))).toBe(false);
    expect(screen.getByRole("link", { name: "Alle Aktivitäten (57)" })).toHaveAttribute("href", "/aktivitaeten");
  });

  it("Diese Woche: Ist von Plan, je Tag Balken mit geplanter Zeit, ausgelassene zählen nicht", async () => {
    units = [
      unit("u1", "2026-11-02", "Rad GA1", 60, { status: "done", activity_id: "x" }),
      unit("u2", "2026-11-05", "Intervalle", 45),
      unit("u3", "2026-11-07", "Lang", 180, { status: "skipped" }),
    ];
    setup();
    const week = (await screen.findByRole("list", { name: "Diese Woche" })).closest("section")!;
    expect(calls).toContain("/api/v1/plan?from=2026-10-30&to=2026-11-08");
    expect(within(week).getByText("02.–08.11.")).toBeInTheDocument();
    expect(await within(week).findByText("von 2")).toBeInTheDocument();
    expect(within(week).getByText("von 1:45 h")).toBeInTheDocument();
    expect(within(week).getByText("3:00")).toBeInTheDocument();
    expect(within(week).getAllByTestId("planned-bar")).toHaveLength(2);
    expect(within(week).getByRole("listitem", { name: "Mo 2: 3:00 h erledigt, 1:00 h geplant" })).toBeInTheDocument();
    expect(within(week).getByRole("listitem", { name: "Heute: 0:00 h erledigt, 0:00 h geplant" })).toBeInTheDocument();
  });

  it("Plan-Marke an Aktivitäten, die mit einer Einheit verknüpft sind", async () => {
    units = [unit("u1", "2026-10-30", "Rad GA1", 60, { status: "done", activity_id: "a1" })];
    setup();
    const ride = (await screen.findByText("Runde Wald")).closest("li")!;
    expect(await within(ride).findByText("✓ Plan: Rad GA1")).toBeInTheDocument();
    expect(within(screen.getByText("Lauf am Main").closest("li")!).queryByText(/Plan:/)).not.toBeInTheDocument();
  });

  it("rechts Monat mit Balken und Jahr nur mit Summen", async () => {
    setup();
    const month = (await screen.findByText("Diesen Monat · November 2026")).closest("section")!;
    expect(within(month).getByText("75,50")).toBeInTheDocument();
    expect(within(month).getByText("Balken 30 Tage, 1 mit Aktivität, cycling")).toBeInTheDocument();
    const year = screen.getByText("Jahr 2026").closest("section")!;
    expect(within(year).getByText("1.234,00")).toBeInTheDocument();
    expect(within(year).getByText("40")).toBeInTheDocument();
    expect(within(year).queryByText(/Balken/)).not.toBeInTheDocument();
    expect(screen.queryByText(/Saison/)).not.toBeInTheDocument();
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
