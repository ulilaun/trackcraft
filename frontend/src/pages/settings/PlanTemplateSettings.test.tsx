import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";
import type { PlanTemplate } from "../../api/client";
import { FlashProvider } from "../../components/Flash";
import { config, json } from "../activities/testData";
import { PlanTemplateSettings } from "./PlanTemplateSettings";

const tpl = (p: Partial<PlanTemplate> & { id: string; name: string }): PlanTemplate => ({
  sport_class: "cycling",
  sub_sport: null,
  title: "Intervalle 4×8",
  note: null,
  mode: "zones",
  duration_min: null,
  target_load: null,
  intensive_pct: 0,
  zone_minutes: [15, 20, 0, 32, 0],
  load: { total: 120, aerobic: 50, anaerobic: 70, minutes: 67, method: "zones" },
  ...p,
});

let list: PlanTemplate[];
let writes: { method: string; url: string; body: unknown }[];

beforeEach(() => {
  list = [tpl({ id: "t1", name: "4×8 Z4" }), tpl({ id: "t2", name: "Lange Einheit", title: "Lange Einheit" })];
  writes = [];
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string, init?: RequestInit) => {
      const method = init?.method ?? "GET";
      if (method !== "GET") {
        const body = init?.body ? JSON.parse(String(init.body)) : undefined;
        writes.push({ method, url, body });
        if (method === "DELETE") return new Response(null, { status: 204 });
        if (url.endsWith("/plan/templates/t2") && (body as { name?: string }).name === "4×8 z4")
          return json(409, { error: { code: "plan_template_exists", message: "exists" } });
        return json(method === "POST" ? 201 : 200, { data: { ...list[0], ...(body as object) } });
      }
      if (url.endsWith("/config")) return json(200, config);
      if (url.endsWith("/plan/templates")) return json(200, { data: list });
      if (url.endsWith("/hr-zones")) return json(200, { data: [], meta: { suggestion: { age: null, max_hr: null } } });
      if (url.startsWith("/api/v1/form")) return json(404, { error: { code: "not_found", message: "nope" } });
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
          <PlanTemplateSettings />
        </FlashProvider>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe("Einstellungen → Plan-Vorlagen", () => {
  it("Liste, umbenennen und ändern, vergebener Name, löschen", async () => {
    setup();
    const ul = await screen.findByTestId("plan-templates");
    expect(within(ul).getAllByRole("button")).toHaveLength(2);
    // Titel nur, wenn er vom Namen abweicht
    expect(within(ul).getByText("Intervalle 4×8")).toBeInTheDocument();

    fireEvent.click(within(ul).getByRole("button", { name: "Vorlage „Lange Einheit“ bearbeiten" }));
    let dialog = screen.getByRole("dialog", { name: "Vorlage bearbeiten" });
    expect(within(dialog).queryByLabelText("Datum")).toBeNull();
    expect(within(dialog).queryByRole("radio", { name: "Jede Woche" })).toBeNull();
    fireEvent.change(within(dialog).getByLabelText("Name der Vorlage"), { target: { value: "4×8 z4" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Speichern" }));
    expect(await within(dialog).findByText("Eine Vorlage mit diesem Namen gibt es schon.")).toBeInTheDocument();
    fireEvent.change(within(dialog).getByLabelText("Name der Vorlage"), { target: { value: "Lang 3 h" } });
    fireEvent.change(within(dialog).getByLabelText("Minuten in Zone 2"), { target: { value: "150" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Speichern" }));
    await waitFor(() =>
      expect(writes[1]).toEqual({
        method: "PATCH",
        url: "/api/v1/plan/templates/t2",
        body: { name: "Lang 3 h", sport_class: "cycling", sub_sport: null, title: "Lange Einheit", note: null, mode: "zones", zone_minutes: [15, 150, 0, 32, 0] },
      }),
    );

    fireEvent.click(within(ul).getByRole("button", { name: "Vorlage „4×8 Z4“ bearbeiten" }));
    dialog = screen.getByRole("dialog", { name: "Vorlage bearbeiten" });
    fireEvent.click(within(dialog).getByRole("button", { name: "Löschen" }));
    await waitFor(() => expect(writes[2]).toEqual({ method: "DELETE", url: "/api/v1/plan/templates/t1", body: undefined }));
    expect(await screen.findByText("Vorlage „4×8 Z4“ gelöscht")).toBeInTheDocument();
  });

  it("Neue Vorlage anlegen", async () => {
    list = [];
    setup();
    expect(await screen.findByText(/Noch keine Vorlage/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Vorlage" }));
    const dialog = screen.getByRole("dialog", { name: "Neue Vorlage" });
    fireEvent.change(within(dialog).getByLabelText("Titel"), { target: { value: "Grundlage" } });
    expect(within(dialog).getByLabelText("Name der Vorlage")).toHaveValue("Grundlage");
    fireEvent.click(within(dialog).getByRole("button", { name: "Speichern" }));
    await waitFor(() => expect(writes[0]).toMatchObject({ method: "POST", url: "/api/v1/plan/templates", body: { name: "Grundlage", title: "Grundlage" } }));
  });
});
