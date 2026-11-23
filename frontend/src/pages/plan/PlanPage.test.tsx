import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";
import type { FormResponse, PlannedUnit, SuggestionResponse } from "../../api/client";
import { FlashProvider } from "../../components/Flash";
import { activity, config, json } from "../activities/testData";
import { PlanPage } from "./PlanPage";

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

function unit(over: Partial<PlannedUnit> = {}): PlannedUnit {
  return {
    id: "u1",
    date: "2026-11-25",
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
  };
}

const noParts = { fatigue_aerobic: null, fatigue_anaerobic: null, monotony: null, strain: null };
const snap = { date: "2026-11-16", fitness: 50, fatigue: 40, form: 10, monotony: null, strain: null };
const form: FormResponse = {
  data: {
    today: "2026-11-23",
    from: "2026-10-26",
    to: "2026-11-23",
    settings,
    zones_configured: true,
    pending: 0,
    first_date: "2026-10-26",
    current: {
      fitness: 55,
      fatigue: 50,
      form: 4,
      load_7d: 300,
      load_prev_7d: 280,
      fitness_week_ago: 54,
      fatigue_week_ago: 48,
      fatigue_aerobic: null,
      fatigue_anaerobic: null,
      monotony: 1.2,
      strain: 400,
      strain_week_ago: 380,
      compare: { month_ago: snap, week_ago: snap, week_ahead: snap },
    },
    monotony_warn: 2,
    days: [
      { date: "2026-11-21", load: 168, aerobic: 168, anaerobic: 0, estimated: 0, fitness: 55, fatigue: 60, form: 0, ...noParts },
      { date: "2026-11-23", load: 0, aerobic: 0, anaerobic: 0, estimated: 0, fitness: 55, fatigue: 50, form: 4, ...noParts },
    ],
    forecast: {
      mode: "plan",
      daily_load: 0,
      days: [
        { date: "2026-11-25", load: 145, fitness: 57, fatigue: 60, form: 2, ...noParts },
        { date: "2026-12-13", load: 0, fitness: 52, fatigue: 30, form: 18.4, ...noParts },
      ],
    },
    plan: { goal: { date: "2026-12-13", name: "Wintertour" }, last_date: "2026-11-25" },
  },
};

const suggestion: SuggestionResponse = {
  data: {
    today: "2026-11-23",
    goal_date: "2026-12-13",
    units: [
      { ...unit({ id: "", date: "2026-11-24", title: "Grundlage", zone_minutes: [10, 50, 0, 0, 0], load: { total: 60, aerobic: 60, anaerobic: 0, minutes: 60, method: "zones" } }), suggested: false, phase: "build" },
      { ...unit({ id: "", date: "2026-12-08", title: "Intervalle 2×6 min" }), suggested: false, phase: "taper" },
    ],
    fixed_units: 1,
    replaces: 3,
    blocks: [
      { phase: "build", from: "2026-11-24", to: "2026-12-02", factor: 1 },
      { phase: "taper", from: "2026-12-03", to: "2026-12-12", factor: 0.5 },
    ],
    weeks: [
      { start: "2026-11-23", phase: "build", load: 380, monotony: 1.4, low_zone_pct: 82, units: 1 },
      { start: "2026-12-07", phase: "taper", load: 145, monotony: 2.3, low_zone_pct: 60, units: 1 },
    ],
    summary: {
      taper_days: 10,
      fitness_today: 55,
      fitness_at_goal: 61.2,
      form_at_goal: 14.3,
      form_state: "fresh",
      avg_gain: 3.9,
      max_gain: 5,
      limited_by_time: true,
      planned_load: 205,
    },
    forecast: form.data.forecast.days,
  },
};

let units: PlannedUnit[];
let writes: { method: string; url: string; body: unknown }[];

beforeEach(() => {
  vi.useFakeTimers({ toFake: ["Date"] });
  vi.setSystemTime(new Date("2026-11-23T10:00:00Z"));
  units = [unit(), unit({ id: "u2", date: "2026-11-21", title: "Lange Runde", status: "done", activity_id: "a2" })];
  writes = [];
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string, init?: RequestInit) => {
      const method = init?.method ?? "GET";
      if (method !== "GET") {
        const body = init?.body ? JSON.parse(String(init.body)) : undefined;
        writes.push({ method, url, body });
        if (method === "DELETE") return new Response(null, { status: 204 });
        if (url.endsWith("/plan/suggestion")) return json(200, suggestion);
        if (url.endsWith("/plan/suggestion/apply")) return json(201, { data: { created: 2, replaced: 3, suggestion_id: "s1" } });
        if (url.endsWith("/plan/goal")) return json(200, { data: body });
        return json(method === "POST" ? 201 : 200, { data: unit({ ...(body as object) }) });
      }
      if (url.endsWith("/config")) return json(200, config);
      if (url.startsWith("/api/v1/plan?"))
        return json(200, {
          data: { today: "2026-11-23", units, goal: form.data.plan.goal, zones_configured: true, assistant: { sport_class: "running", minutes: [0, 45, 0, 45, 0, 90, 60], max_gain: 4, recovery_weeks: false, intensity: "polarized" } },
        });
      if (url.startsWith("/api/v1/activities?"))
        return json(200, {
          data: [
            activity({ id: "a1", title: "Abendrunde", start_time: "2026-11-19T17:00:00.000Z", total_time_ms: 4_920_000, load: 118 }),
            activity({ id: "a2", title: "Lange Runde", start_time: "2026-11-21T09:00:00.000Z" }),
          ],
          meta: { total: 2, limit: 200, offset: 0 },
        });
      if (url.startsWith("/api/v1/form?")) return json(200, form);
      if (url.endsWith("/hr-zones")) return json(200, { data: [], meta: { suggestion: { age: null, max_hr: null } } });
      return json(404, { error: { code: "not_found", message: "nope" } });
    }),
  );
});

afterEach(() => {
  vi.useRealTimers();
});

function setup() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter initialEntries={["/plan"]}>
        <FlashProvider>
          <PlanPage />
        </FlashProvider>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe("Seite Plan", () => {
  it("Kalender mit geplanten Einheiten, Ist-Aktivitäten ohne Plan, Ziel und Vorschau", async () => {
    setup();
    const cal = await screen.findByTestId("plan-calendar");
    expect(await within(cal).findByText("Intervalle 4×8")).toBeInTheDocument();
    // Die verknüpfte Aktivität erscheint nur als erledigte Einheit, die andere als Ist
    expect(await within(cal).findByText("Abendrunde")).toBeInTheDocument();
    expect(within(cal).getAllByText("Lange Runde")).toHaveLength(1);
    expect(within(cal).getByText("✓")).toBeInTheDocument();
    expect(within(cal).getByText("🏁 Wintertour")).toBeInTheDocument();
    expect(within(cal).getByText("KW 47")).toBeInTheDocument();
    // Vorschau: Form am Ziel mit Zustand
    expect(screen.getByText("Form am Ziel (13.12.2026)")).toBeInTheDocument();
    expect(screen.getByText("+18")).toBeInTheDocument();
    expect(screen.getByText("Geplante Belastung")).toBeInTheDocument();
  });

  it("Klick auf einen Tag legt eine Einheit an, mit Belastung je Zone", async () => {
    setup();
    const cal = await screen.findByTestId("plan-calendar");
    fireEvent.click(within(cal).getByRole("button", { name: "Einheit am 27.11.2026 anlegen" }));
    const dialog = screen.getByRole("dialog", { name: "Neue Einheit" });
    expect(within(dialog).getByLabelText("Datum")).toHaveValue("2026-11-27");
    const save = within(dialog).getByRole("button", { name: "Speichern" });
    expect(save).toBeDisabled();
    fireEvent.change(within(dialog).getByLabelText("Titel"), { target: { value: "Grundlage" } });
    fireEvent.change(within(dialog).getByLabelText("Minuten in Zone 1"), { target: { value: "30" } });
    // Ohne Zonensatz geschätzt: 90 min × 50 je Stunde
    expect(within(dialog).getByTestId("plan-preview")).toHaveTextContent("Belastung 75");
    fireEvent.click(save);
    await waitFor(() =>
      expect(writes).toEqual([
        {
          method: "POST",
          url: "/api/v1/plan/units",
          body: { date: "2026-11-27", sport_class: "cycling", sub_sport: null, title: "Grundlage", note: null, mode: "zones", zone_minutes: [30, 60, 0, 0, 0] },
        },
      ]),
    );
  });

  it("Einheit öffnen: als ausgelassen markieren, duplizieren und löschen", async () => {
    setup();
    fireEvent.click(await screen.findByRole("button", { name: /Intervalle 4×8/ }));
    let dialog = screen.getByRole("dialog", { name: "Geplante Einheit" });
    fireEvent.click(within(dialog).getByRole("radio", { name: "ausgelassen" }));
    fireEvent.click(within(dialog).getByRole("button", { name: "Speichern" }));
    await waitFor(() => expect(writes[0]).toMatchObject({ method: "PATCH", url: "/api/v1/plan/units/u1", body: { status: "skipped", zone_minutes: [22, 15, 6, 32, 0] } }));

    fireEvent.click(await screen.findByRole("button", { name: /Intervalle 4×8/ }));
    dialog = screen.getByRole("dialog", { name: "Geplante Einheit" });
    fireEvent.click(within(dialog).getByRole("button", { name: "Duplizieren …" }));
    fireEvent.change(within(dialog).getByLabelText("Kopieren auf"), { target: { value: "2026-12-02" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Kopieren" }));
    await waitFor(() => expect(writes[1]).toMatchObject({ method: "POST", url: "/api/v1/plan/units", body: { date: "2026-12-02", title: "Intervalle 4×8" } }));

    fireEvent.click(await screen.findByRole("button", { name: /Intervalle 4×8/ }));
    fireEvent.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Löschen" }));
    await waitFor(() => expect(writes[2]).toEqual({ method: "DELETE", url: "/api/v1/plan/units/u1", body: undefined }));
  });

  it("Zieltermin ändern", async () => {
    setup();
    fireEvent.click(await screen.findByRole("button", { name: /Wintertour · 13\.12\.2026/ }));
    const dialog = screen.getByRole("dialog", { name: "Zieltermin" });
    fireEvent.change(within(dialog).getByLabelText("Name"), { target: { value: "Rundfahrt" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Speichern" }));
    await waitFor(() => expect(writes).toEqual([{ method: "PUT", url: "/api/v1/plan/goal", body: { date: "2026-12-13", name: "Rundfahrt" } }]));
  });

  it("Planungshilfe: Vorschau mit gemerkten Eingaben, dann übernehmen", async () => {
    setup();
    await within(await screen.findByTestId("plan-calendar")).findByText("Intervalle 4×8");
    fireEvent.click(screen.getByRole("button", { name: /Plan vorschlagen/ }));
    const dialog = screen.getByRole("dialog", { name: "Plan vorschlagen" });
    // Gemerkte Eingaben vorbelegt
    expect(within(dialog).getByLabelText("Sportklasse")).toHaveValue("running");
    expect(within(dialog).getByLabelText("Minuten am Sa")).toHaveValue("90");
    expect(within(dialog).getByText(/^zusammen 4(,0)? h je Woche$/)).toBeInTheDocument();
    const preview = within(dialog).getByTestId("suggest-preview");
    expect(await within(preview).findByText("+14")).toBeInTheDocument();
    expect(within(preview).getByText("55 → 61")).toBeInTheDocument();
    expect(within(preview).getByText("10 Tage")).toBeInTheDocument();
    expect(within(preview).getByText("dazu 1 feste")).toBeInTheDocument();
    expect(within(preview).getByText(/durch die verfügbare Zeit begrenzt \(Ø 3,9 je Woche statt höchstens 5\)/)).toBeInTheDocument();
    expect(within(dialog).getByText("Ersetzt 3 offene Einheiten des letzten Vorschlags.")).toBeInTheDocument();
    // Monotonie über 2 rot markiert
    expect(within(within(preview).getByTestId("suggest-weeks")).getByText("50")).toHaveClass("text-danger");
    const firstPreview = writes.find((w) => w.url === "/api/v1/plan/suggestion");
    expect(firstPreview?.body).toEqual({ sport_class: "running", max_gain: 4, recovery_weeks: false, intensity: "polarized", minutes: [0, 45, 0, 45, 0, 90, 60] });

    // Ungültige Minuten: Meldung, kein Übernehmen
    fireEvent.change(within(dialog).getByLabelText("Minuten am Mo"), { target: { value: "abc" } });
    expect(within(dialog).getByText("Minuten je Tag als ganze Zahl von 0 bis 600.")).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "2 Einheiten übernehmen" })).toBeDisabled();
    fireEvent.change(within(dialog).getByLabelText("Minuten am Mo"), { target: { value: "30" } });
    const apply = within(dialog).getByRole("button", { name: "2 Einheiten übernehmen" });
    await waitFor(() => expect(apply).toBeEnabled());
    fireEvent.click(apply);
    await waitFor(() =>
      expect(writes.find((w) => w.url === "/api/v1/plan/suggestion/apply")?.body).toEqual({
        sport_class: "running",
        max_gain: 4,
        recovery_weeks: false,
        intensity: "polarized",
        minutes: [30, 45, 0, 45, 0, 90, 60],
      }),
    );
    expect(await screen.findByText("2 Einheiten übernommen.")).toBeInTheDocument();
  });
});
