import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";
import type { PlannedUnit } from "../../api/client";
import { json } from "./testData";
import { PlannedUnitCard } from "./PlannedUnitCard";

const unit = (over: Partial<PlannedUnit> = {}): PlannedUnit => ({
  id: "u1",
  date: "2026-11-23",
  sport_class: "cycling",
  sub_sport: null,
  title: "Intervalle 4×8",
  note: null,
  mode: "zones",
  duration_min: null,
  target_load: null,
  intensive_pct: 0,
  zone_minutes: [22, 15, 6, 32, 0],
  status: "planned",
  activity_id: null,
  load: { total: 145, aerobic: 56, anaerobic: 89, minutes: 75, method: "zones" },
  actual: null,
  ...over,
});

let match: { linked: PlannedUnit | null; candidates: PlannedUnit[] };
let writes: { method: string; url: string; body: unknown }[];

beforeEach(() => {
  writes = [];
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string, init?: RequestInit) => {
      const method = init?.method ?? "GET";
      if (method !== "GET") {
        writes.push({ method, url, body: init?.body ? JSON.parse(String(init.body)) : undefined });
        return method === "POST" ? new Response(null, { status: 204 }) : json(200, { data: unit() });
      }
      return json(200, { data: match });
    }),
  );
});

function setup() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter>
        <PlannedUnitCard activityId="a1" />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe("Geplante Einheit auf der Aktivitätsseite", () => {
  it("ohne Plan an diesem Tag erscheint nichts", async () => {
    match = { linked: null, candidates: [] };
    const { container } = setup();
    await waitFor(() => expect(fetch).toHaveBeenCalled());
    expect(container).toBeEmptyDOMElement();
  });

  it("Vorschlag verknüpfen oder ablehnen", async () => {
    match = { linked: null, candidates: [unit()] };
    setup();
    const card = await screen.findByTestId("plan-match");
    expect(within(card).getByText("Geplant war: „Intervalle 4×8“")).toBeInTheDocument();
    fireEvent.click(within(card).getByRole("button", { name: "Verknüpfen" }));
    await waitFor(() => expect(writes[0]).toEqual({ method: "PUT", url: "/api/v1/plan/units/u1/activity", body: { activity_id: "a1" } }));
    fireEvent.click(within(card).getByRole("button", { name: "Nicht verknüpfen" }));
    await waitFor(() => expect(writes[1]).toEqual({ method: "POST", url: "/api/v1/plan/units/u1/decline", body: { activity_id: "a1" } }));
  });

  it("verknüpft: Vergleich Plan ↔ Ist und Lösen", async () => {
    match = {
      linked: unit({
        status: "done",
        activity_id: "a1",
        actual: {
          activity_id: "a1",
          title: "Abendrunde",
          start_time: "2026-11-23T16:42:00.000Z",
          total_time_ms: 4_920_000,
          load: 118,
          anaerobic: 41,
          load_method: "samples",
          zone_seconds: [1380, 1620, 600, 1080, 240],
        },
      }),
      candidates: [],
    };
    setup();
    expect(await screen.findByText("Plan ↔ Ist")).toBeInTheDocument();
    const row = (name: string) => screen.getByText(name).closest("tr")!;
    expect(row("Dauer")).toHaveTextContent("1:151:22+0:07");
    expect(row("Belastung")).toHaveTextContent("145118−27");
    expect(row("davon anaerob")).toHaveTextContent("8941−48");
    fireEvent.click(screen.getByRole("button", { name: "Lösen" }));
    await waitFor(() => expect(writes).toEqual([{ method: "DELETE", url: "/api/v1/plan/units/u1/activity", body: undefined }]));
  });
});
