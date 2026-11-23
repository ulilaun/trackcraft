import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";
import type { FormResponse, PlanGoal, PlanTemplate, PlannedUnit, SuggestionResponse } from "../../api/client";
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

const goal: PlanGoal = {
  id: "g1",
  date: "2026-12-13",
  sport_class: "cycling",
  name: "Wintertour",
  description: null,
  url: null,
  distance_m: 120000,
  target_time_s: null,
};

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
    plan: { goals: [goal], last_date: "2026-11-25" },
  },
};

const suggestion: SuggestionResponse = {
  data: {
    today: "2026-11-23",
    goal_date: "2026-12-13",
    units: [
      { ...unit({ id: "", date: "2026-11-24", title: "Grundlage", zone_minutes: [10, 50, 0, 0, 0], load: { total: 60, aerobic: 60, anaerobic: 0, minutes: 60, method: "zones" } }), phase: "build" },
      { ...unit({ id: "", date: "2026-12-08", title: "Intervalle 2×6 min" }), phase: "taper" },
    ],
    fixed_units: 1,
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

const template: PlanTemplate = {
  id: "t1",
  name: "Lange Einheit 3 h",
  sport_class: "cycling",
  sub_sport: null,
  title: "Lange Ausfahrt",
  note: "Verpflegung testen",
  mode: "target",
  duration_min: 180,
  target_load: 150,
  intensive_pct: 0,
  zone_minutes: null,
  load: { total: 150, aerobic: 150, anaerobic: 0, minutes: 180, method: "target" },
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
        if (url.endsWith("/plan/suggestion/apply")) return json(201, { data: { created: 2 } });
        if (url.includes("/plan/templates")) return json(method === "POST" ? 201 : 200, { data: { ...template, id: "t2", ...(body as object) } });
        if (url.includes("/plan/goals")) return json(method === "POST" ? 201 : 200, { data: { ...goal, id: method === "POST" ? "g2" : goal.id, ...(body as object) } });
        return json(method === "POST" ? 201 : 200, { data: unit({ ...(body as object) }) });
      }
      if (url.endsWith("/config")) return json(200, config);
      if (url.endsWith("/plan/templates")) return json(200, { data: [template] });
      if (url.startsWith("/api/v1/plan?"))
        return json(200, {
          data: { today: "2026-11-23", units, goals: form.data.plan.goals, zones_configured: true, assistant: { sport_class: "running", minutes: [0, 45, 0, 45, 0, 90, 60], max_gain: 4, recovery_weeks: false, intensity: "polarized" } },
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
    expect(within(cal).getByRole("button", { name: "Wintertour" })).toHaveAttribute("title", "Wintertour · 120 km");
    expect(within(cal).getByText("KW 47")).toBeInTheDocument();
    // Vorschau: Form am Ziel mit Zustand
    expect(screen.getByText("Form am Ziel (13.12.2026)")).toBeInTheDocument();
    expect(screen.getByText("+18")).toBeInTheDocument();
    expect(screen.getByText("Geplante Belastung")).toBeInTheDocument();
    // Oben nur noch „+ Ziel“ und „+ Einheit“
    expect(screen.getByRole("button", { name: "Ziel" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Einheit" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Vorlage|Plan vorschlagen/ })).toBeNull();
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

  it("Ziel im Kalender öffnen und ändern", async () => {
    setup();
    fireEvent.click(await within(await screen.findByTestId("plan-calendar")).findByRole("button", { name: "Wintertour" }));
    const dialog = screen.getByRole("dialog", { name: "Ziel bearbeiten" });
    expect(within(dialog).getByLabelText("Distanz (km, optional)")).toHaveValue("120");
    fireEvent.change(within(dialog).getByLabelText("Name"), { target: { value: "Rundfahrt" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Speichern" }));
    await waitFor(() =>
      expect(writes).toEqual([
        {
          method: "PATCH",
          url: "/api/v1/plan/goals/g1",
          body: { date: "2026-12-13", sport_class: "cycling", name: "Rundfahrt", description: null, url: null, distance_m: 120000, target_time_s: null },
        },
      ]),
    );
  });

  it("Neues Ziel mit Zielzeit und Webseite", async () => {
    setup();
    await screen.findByTestId("plan-calendar");
    fireEvent.click(screen.getByRole("button", { name: "Ziel" }));
    const dialog = screen.getByRole("dialog", { name: "Ziel erstellen" });
    // Sportklasse aus den gemerkten Eingaben der Planungshilfe
    await waitFor(() => expect(within(dialog).getByLabelText("Sportklasse")).toHaveValue("running"));
    const save = within(dialog).getByRole("button", { name: "Speichern" });
    expect(save).toBeDisabled();
    fireEvent.change(within(dialog).getByLabelText("Name"), { target: { value: "Halbmarathon" } });
    fireEvent.change(within(dialog).getByLabelText("Datum"), { target: { value: "2027-04-11" } });
    fireEvent.change(within(dialog).getByLabelText("Zielzeit (optional)"), { target: { value: "1:45" } });
    expect(within(dialog).getByText("Zielzeit als h:mm:ss, z. B. 4:30:00.")).toBeInTheDocument();
    expect(save).toBeDisabled();
    fireEvent.change(within(dialog).getByLabelText("Zielzeit (optional)"), { target: { value: "1:45:00" } });
    fireEvent.change(within(dialog).getByLabelText("Distanz (km, optional)"), { target: { value: "21,1" } });
    fireEvent.change(within(dialog).getByLabelText(/Webseite/), { target: { value: "https://example.org/hm" } });
    fireEvent.click(save);
    await waitFor(() =>
      expect(writes).toEqual([
        {
          method: "POST",
          url: "/api/v1/plan/goals",
          body: { date: "2027-04-11", sport_class: "running", name: "Halbmarathon", description: null, url: "https://example.org/hm", distance_m: 21100, target_time_s: 6300 },
        },
      ]),
    );
    expect(await screen.findByText("Ziel gespeichert")).toBeInTheDocument();
  });

  it("Planungshilfe aus dem Ziel: Vorschau mit gemerkten Eingaben, dann übernehmen", async () => {
    setup();
    fireEvent.click(await within(await screen.findByTestId("plan-calendar")).findByRole("button", { name: "Wintertour" }));
    fireEvent.click(within(screen.getByRole("dialog", { name: "Ziel bearbeiten" })).getByRole("button", { name: /Plan vorschlagen/ }));
    const dialog = await screen.findByRole("dialog", { name: "Plan vorschlagen" });
    expect(screen.queryByRole("dialog", { name: "Ziel bearbeiten" })).toBeNull();
    // Unverändertes Ziel wird nicht gespeichert; Sportklasse vom Ziel, übrige Eingaben gemerkt
    expect(writes.filter((w) => w.url.includes("/plan/goals"))).toEqual([]);
    expect(within(dialog).getByLabelText("Sportklasse")).toHaveValue("cycling");
    expect(within(dialog).getByLabelText("Minuten am Sa")).toHaveValue("90");
    expect(within(dialog).getByText(/^zusammen 4(,0)? h je Woche$/)).toBeInTheDocument();
    const preview = within(dialog).getByTestId("suggest-preview");
    expect(await within(preview).findByText("+14")).toBeInTheDocument();
    expect(within(preview).getByText("55 → 61")).toBeInTheDocument();
    expect(within(preview).getByText("10 Tage")).toBeInTheDocument();
    expect(within(preview).getByText("dazu 1 feste")).toBeInTheDocument();
    expect(within(preview).getByText(/durch die verfügbare Zeit begrenzt \(Ø 3,9 je Woche statt höchstens 5\)/)).toBeInTheDocument();
    // Monotonie über 2 rot markiert
    expect(within(within(preview).getByTestId("suggest-weeks")).getByText("50")).toHaveClass("text-danger");
    const firstPreview = writes.find((w) => w.url === "/api/v1/plan/suggestion");
    expect(firstPreview?.body).toEqual({ goal_id: "g1", sport_class: "cycling", max_gain: 4, recovery_weeks: false, intensity: "polarized", minutes: [0, 45, 0, 45, 0, 90, 60] });

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
        goal_id: "g1",
        sport_class: "cycling",
        max_gain: 4,
        recovery_weeks: false,
        intensity: "polarized",
        minutes: [30, 45, 0, 45, 0, 90, 60],
      }),
    );
    expect(await screen.findByText("2 Einheiten übernommen.")).toBeInTheDocument();
  });
});

describe("Plan: Einheiten-Vorlagen", () => {
  it("Neue Einheit aus Vorlage füllen, im Kalender und als Vorlage speichern", async () => {
    setup();
    const cal = await screen.findByTestId("plan-calendar");
    fireEvent.click(within(cal).getByRole("button", { name: "Einheit am 27.11.2026 anlegen" }));
    const dialog = screen.getByRole("dialog", { name: "Neue Einheit" });
    fireEvent.change(await within(dialog).findByLabelText("Aus Vorlage"), { target: { value: "t1" } });
    expect(within(dialog).getByLabelText("Titel")).toHaveValue("Lange Ausfahrt");
    expect(within(dialog).getByLabelText("Datum")).toHaveValue("2026-11-27");
    expect(within(dialog).getByRole("radio", { name: "Zielbelastung" })).toHaveAttribute("aria-checked", "true");
    // Danach lässt sich alles ändern
    fireEvent.change(within(dialog).getByLabelText("Titel"), { target: { value: "Lange Ausfahrt Hügel" } });
    fireEvent.click(within(dialog).getByRole("radio", { name: "Im Kalender und als Vorlage" }));
    // Name der Vorlage folgt dem Titel, bis man ihn ändert
    expect(within(dialog).getByLabelText("Name der Vorlage")).toHaveValue("Lange Ausfahrt Hügel");
    fireEvent.change(within(dialog).getByLabelText("Name der Vorlage"), { target: { value: "Hügel 3 h" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Speichern" }));
    const fields = { sport_class: "cycling", sub_sport: null, title: "Lange Ausfahrt Hügel", note: "Verpflegung testen", mode: "target", duration_min: 180, target_load: 150, intensive_pct: 0 };
    await waitFor(() =>
      expect(writes).toEqual([
        { method: "POST", url: "/api/v1/plan/templates", body: { name: "Hügel 3 h", ...fields } },
        { method: "POST", url: "/api/v1/plan/units", body: { date: "2026-11-27", ...fields } },
      ]),
    );
  });

  it("Nur als Vorlage: ohne Datum und Wiederholen", async () => {
    setup();
    await screen.findByTestId("plan-calendar");
    fireEvent.click(screen.getByRole("button", { name: "Einheit" }));
    const dialog = screen.getByRole("dialog", { name: "Neue Einheit" });
    fireEvent.change(within(dialog).getByLabelText("Titel"), { target: { value: "Intervalle 4×8" } });
    fireEvent.click(within(dialog).getByRole("radio", { name: "Nur als Vorlage" }));
    expect(within(dialog).queryByLabelText("Datum")).toBeNull();
    expect(within(dialog).queryByRole("radio", { name: "Jede Woche" })).toBeNull();
    fireEvent.change(within(dialog).getByLabelText("Name der Vorlage"), { target: { value: " " } });
    expect(within(dialog).getByRole("button", { name: "Speichern" })).toBeDisabled();
    fireEvent.change(within(dialog).getByLabelText("Name der Vorlage"), { target: { value: "4×8 Z4" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Speichern" }));
    await waitFor(() =>
      expect(writes).toEqual([
        {
          method: "POST",
          url: "/api/v1/plan/templates",
          body: { name: "4×8 Z4", sport_class: "cycling", sub_sport: null, title: "Intervalle 4×8", note: null, mode: "zones", zone_minutes: [0, 60, 0, 0, 0] },
        },
      ]),
    );
    expect(await screen.findByText("Vorlage „4×8 Z4“ gespeichert")).toBeInTheDocument();
  });

  it("Bestehende Einheit hat keine Vorlagen-Auswahl", async () => {
    setup();
    fireEvent.click(await screen.findByRole("button", { name: /Intervalle 4×8/ }));
    const dialog = screen.getByRole("dialog", { name: "Geplante Einheit" });
    expect(within(dialog).queryByLabelText("Aus Vorlage")).toBeNull();
    expect(within(dialog).queryByTestId("plan-save-target")).toBeNull();
  });
});

/** Zieht ein Element auf einen Tag des Kalenders (HTML5 Drag & Drop wie im Browser). */
function drag(el: HTMLElement, day: string) {
  const target = document.querySelector<HTMLElement>(`[data-day="${day}"]`)!;
  const dataTransfer = { setData: vi.fn(), getData: vi.fn(), effectAllowed: "", dropEffect: "" };
  fireEvent.dragStart(el, { dataTransfer });
  fireEvent.dragOver(target, { dataTransfer });
  fireEvent.drop(target, { dataTransfer });
  fireEvent.dragEnd(el, { dataTransfer });
}

describe("Plan: Verschieben per Drag & Drop", () => {
  it("Einheit auf einen anderen Tag ziehen", async () => {
    setup();
    const el = await screen.findByRole("button", { name: /Intervalle 4×8/ });
    expect(el).toHaveAttribute("draggable", "true");
    // Auf denselben Tag: nichts
    drag(el, "2026-11-25");
    drag(el, "2026-11-27");
    await waitFor(() => expect(writes).toEqual([{ method: "PATCH", url: "/api/v1/plan/units/u1", body: { date: "2026-11-27" } }]));
    expect(await screen.findByText("Auf 27.11.2026 verschoben")).toBeInTheDocument();
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("Erledigte Einheit bleibt an ihrem Tag", async () => {
    setup();
    const cal = await screen.findByTestId("plan-calendar");
    const done = await within(cal).findByRole("button", { name: /Lange Runde/ });
    expect(done).toHaveAttribute("draggable", "false");
    drag(done, "2026-11-22");
    expect(writes).toEqual([]);
  });

  it("Serie: fragt nur diese oder diese und folgende", async () => {
    units = [unit({ series_id: "s1", series_following: 3 })];
    setup();
    drag(await screen.findByRole("button", { name: /Intervalle 4×8/ }), "2026-11-26");
    let dialog = screen.getByRole("dialog", { name: "Einheit verschieben" });
    expect(dialog).toHaveTextContent("„Intervalle 4×8“ vom 25.11.2026 auf den 26.11.2026 verschieben?");
    expect(dialog).toHaveTextContent("noch 3 offene Einheiten danach");
    fireEvent.click(within(dialog).getByRole("button", { name: "Diese und folgende" }));
    await waitFor(() => expect(writes).toEqual([{ method: "PATCH", url: "/api/v1/plan/units/u1?scope=following", body: { date: "2026-11-26" } }]));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());

    drag(await screen.findByRole("button", { name: /Intervalle 4×8/ }), "2026-11-28");
    dialog = screen.getByRole("dialog", { name: "Einheit verschieben" });
    fireEvent.click(within(dialog).getByRole("button", { name: "Nur diese" }));
    await waitFor(() => expect(writes[1]).toEqual({ method: "PATCH", url: "/api/v1/plan/units/u1", body: { date: "2026-11-28" } }));
  });
});

describe("Plan: Serien", () => {
  it("Neue Einheit wöchentlich wiederholen", async () => {
    setup();
    const cal = await screen.findByTestId("plan-calendar");
    fireEvent.click(within(cal).getByRole("button", { name: "Einheit am 27.11.2026 anlegen" }));
    const dialog = screen.getByRole("dialog", { name: "Neue Einheit" });
    fireEvent.change(within(dialog).getByLabelText("Titel"), { target: { value: "Grundlage" } });
    fireEvent.click(within(dialog).getByRole("radio", { name: "Jede Woche" }));
    // Vorgabe: 8 Wochen später
    expect(within(dialog).getByLabelText("bis einschließlich")).toHaveValue("2027-01-22");
    expect(within(dialog).getByTestId("plan-repeat-count")).toHaveTextContent("ergibt 9 Einheiten (27.11.2026 bis 22.01.2027)");
    fireEvent.change(within(dialog).getByLabelText("bis einschließlich"), { target: { value: "2026-11-30" } });
    expect(within(dialog).getByTestId("plan-repeat-count")).toHaveTextContent("keine Wiederholung");
    expect(within(dialog).getByRole("button", { name: "Speichern" })).toBeDisabled();
    fireEvent.change(within(dialog).getByLabelText("bis einschließlich"), { target: { value: "2026-12-11" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Speichern" }));
    await waitFor(() => expect(writes[0]).toMatchObject({ method: "POST", url: "/api/v1/plan/units", body: { title: "Grundlage", repeat: { every_weeks: 1, until: "2026-12-11" } } }));
    expect(await screen.findByText("3 Einheiten gespeichert")).toBeInTheDocument();
  });

  it("Einheit einer Serie: diese und folgende ändern und löschen", async () => {
    units = [unit({ series_id: "s1", series_following: 3 })];
    setup();
    fireEvent.click(await screen.findByRole("button", { name: /Intervalle 4×8/ }));
    expect(screen.getByLabelText("Teil einer Serie")).toBeInTheDocument();
    let dialog = screen.getByRole("dialog", { name: "Geplante Einheit" });
    expect(within(dialog).getByTestId("plan-series-scope")).toHaveTextContent("noch 3 offene Einheiten danach");
    fireEvent.click(within(dialog).getByRole("radio", { name: "Diese und folgende" }));
    // Wiederholen ist dann aus
    expect(within(dialog).queryByRole("radio", { name: "Jede Woche" })).toBeNull();
    fireEvent.change(within(dialog).getByLabelText("Titel"), { target: { value: "Intervalle 5×6" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Speichern" }));
    await waitFor(() => expect(writes[0]).toEqual({ method: "PATCH", url: "/api/v1/plan/units/u1?scope=following", body: { title: "Intervalle 5×6" } }));

    fireEvent.click(await screen.findByRole("button", { name: /Intervalle 4×8/ }));
    dialog = screen.getByRole("dialog", { name: "Geplante Einheit" });
    fireEvent.click(within(dialog).getByRole("radio", { name: "Diese und folgende" }));
    fireEvent.click(within(dialog).getByRole("button", { name: "Löschen" }));
    await waitFor(() => expect(writes[1]).toEqual({ method: "DELETE", url: "/api/v1/plan/units/u1?scope=following", body: undefined }));
  });
});
