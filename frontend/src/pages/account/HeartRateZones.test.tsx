import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";
import type { HrZoneSet } from "../../api/client";
import { FlashProvider } from "../../components/Flash";
import { json } from "../activities/testData";
import { HeartRateZones } from "./HeartRateZones";

const general: HrZoneSet = {
  id: "g1",
  sport_class: null,
  valid_from: "2018-01-01",
  max_hr: 180,
  resting_hr: 50,
  threshold_hr: null,
  model: "reserve",
  bounds: [50, 60, 70, 80, 90],
  zones_bpm: [
    [115, 127],
    [128, 140],
    [141, 153],
    [154, 166],
    [167, null],
  ],
  anaerobic_from_bpm: 154,
};

let sets: HrZoneSet[];
let writes: { method: string; url: string; body: unknown }[];

beforeEach(() => {
  sets = [general];
  writes = [];
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string, init?: RequestInit) => {
      const method = init?.method ?? "GET";
      if (method !== "GET") {
        const body = init?.body ? JSON.parse(String(init.body)) : null;
        writes.push({ method, url, body });
        if (url.endsWith("/form/settings")) return json(200, { data: { sex_factor: body.sex_factor, fitness_days: 42, fatigue_days: 7, estimate_per_hour: {} } });
        if (method === "DELETE") return new Response(null, { status: 204 });
        return json(method === "POST" ? 201 : 200, { data: { ...general, ...body, id: "new" } });
      }
      if (url.endsWith("/hr-zones")) return json(200, { data: sets, meta: { suggestion: { age: 58, max_hr: 167 } } });
      if (url.endsWith("/form/settings")) return json(200, { data: { sex_factor: 1.92, fitness_days: 42, fatigue_days: 7, estimate_per_hour: {} } });
      return json(404, { error: { code: "not_found", message: "nope" } });
    }),
  );
});

function setup() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter>
        <FlashProvider>
          <HeartRateZones today="2026-11-22" />
        </FlashProvider>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe("HF-Zonen auf der Kontoseite", () => {
  it("zeigt den neuesten Stand und rechnet Grenzen beim Tippen", async () => {
    setup();
    const max = await screen.findByLabelText("Max. HF");
    expect(max).toHaveValue("180");
    expect(screen.getByTestId("zone-bpm-1")).toHaveTextContent("115–127");
    expect(screen.getByText(/Anaerob ab 154/)).toBeInTheDocument();
    expect(screen.getByText("nach Alter 167 (Tanaka)")).toBeInTheDocument();
    fireEvent.change(max, { target: { value: "190" } });
    // 50 + 140 × 0,5 = 120
    expect(screen.getByTestId("zone-bpm-1")).toHaveTextContent("120–133");
    fireEvent.click(screen.getByRole("button", { name: "Speichern" }));
    await waitFor(() => expect(writes).toHaveLength(1));
    expect(writes[0]).toMatchObject({ method: "PATCH", url: "/api/v1/hr-zones/g1", body: { max_hr: 190, resting_hr: 50, model: "reserve" } });
  });

  it("Friel braucht die Schwellen-HF; neuer Stand ab heute wird angelegt", async () => {
    setup();
    await screen.findByLabelText("Max. HF");
    fireEvent.click(screen.getByRole("button", { name: /Neuer Stand ab heute/ }));
    expect(screen.getByLabelText("Gültig ab")).toHaveValue("2026-11-22");
    fireEvent.change(screen.getByLabelText("Zonenmodell"), { target: { value: "threshold" } });
    expect(screen.getByRole("alert")).toHaveTextContent("braucht die Schwellen-HF");
    expect(screen.getByRole("button", { name: "Speichern" })).toBeDisabled();
    fireEvent.change(screen.getByLabelText("Schwellen-HF (optional)"), { target: { value: "160" } });
    expect(screen.getByTestId("zone-bpm-5")).toHaveTextContent("160–max");
    fireEvent.click(screen.getByRole("button", { name: "Speichern" }));
    await waitFor(() => expect(writes).toHaveLength(1));
    expect(writes[0]).toMatchObject({
      method: "POST",
      url: "/api/v1/hr-zones",
      body: { sport_class: null, valid_from: "2026-11-22", threshold_hr: 160, model: "threshold", bounds: [68, 85, 90, 95, 100] },
    });
  });

  it("eigene Zonen für eine Sportklasse und Faktor der Formel", async () => {
    setup();
    await screen.findByLabelText("Max. HF");
    fireEvent.change(screen.getByLabelText("Eigene Zonen für …"), { target: { value: "running" } });
    const tabs = screen.getByRole("tablist");
    expect(within(tabs).getByRole("tab", { name: /Laufen/ })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByText(/Gilt für Laufen ab dem jeweiligen Datum/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Speichern" }));
    await waitFor(() => expect(writes).toHaveLength(1));
    expect(writes[0]).toMatchObject({ method: "POST", body: { sport_class: "running", max_hr: 180 } });

    fireEvent.change(screen.getByLabelText("Faktor der Belastungsformel"), { target: { value: "1.67" } });
    await waitFor(() => expect(writes.at(-1)).toMatchObject({ method: "PATCH", url: "/api/v1/form/settings", body: { sex_factor: 1.67 } }));
  });

  it("ohne Stand: Vorschlag aus dem Alter", async () => {
    sets = [];
    setup();
    expect(await screen.findByText(/Noch kein Stand eingetragen/)).toBeInTheDocument();
    expect(screen.getByLabelText("Max. HF")).toHaveValue("167");
    expect(screen.queryByLabelText("Eigene Zonen für …")).not.toBeInTheDocument();
  });
});
