import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";
import type { ImportJob, ImportSession } from "../../api/client";
import { ImportPage } from "./ImportPage";

function session(over: Partial<ImportSession> = {}): ImportSession {
  return {
    session_index: 0,
    state: "selectable",
    lock_reason: null,
    preselected: true,
    title: "Morgenrunde",
    source_sport: "cycling",
    sport_class: "cycling",
    sub_sport: "road",
    start_time: "2026-07-01T08:00:00.000Z",
    total_time_ms: 590_000,
    distance_m: 2000,
    avg_heart_rate_bpm: 131,
    point_count: 60,
    has_position: true,
    time_input_required: false,
    existing_activity_id: null,
    duplicate_candidates: [],
    ...over,
  };
}

function job(id: string, name: string, sessions: ImportSession[]): ImportJob {
  return {
    id,
    status: "awaiting_decision",
    original_filename: name,
    format: "gpx",
    size_bytes: 100,
    state_token: `st_${id}`,
    error: null,
    sessions,
    imported_activity_ids: [],
    created_at: "2026-11-08T10:00:00.000Z",
    finished_at: null,
  };
}

const uploads: Record<string, { status: number; body: unknown }> = {
  "a.gpx": { status: 201, body: { data: job("a", "a.gpx", [session()]) } },
  "b.gpx": {
    status: 201,
    body: {
      data: job("b", "b.gpx", [
        session({
          title: "Strava",
          duplicate_candidates: [
            {
              activity_id: "old",
              title: "Edge-Fahrt",
              sport_class: "cycling",
              start_time: "2026-07-01T08:00:20.000Z",
              total_time_ms: 600_000,
              distance_m: 2010,
              matched_criteria: ["start_time", "total_time", "distance"],
            },
          ],
        }),
      ]),
    },
  },
  "route.gpx": { status: 400, body: { error: { code: "gpx_without_time", message: "no time" } } },
  "fahrt.fit": {
    status: 201,
    body: { data: job("c", "fahrt.fit", [session({ state: "locked_vehicle", lock_reason: "Zwischen 13:05 und 13:08 bis zu 68 km/h." })]) },
  },
};

/** XMLHttpRequest-Ersatz: antwortet je Dateiname. */
class FakeXhr {
  status = 0;
  statusText = "";
  responseText = "";
  upload: { onprogress: ((e: { lengthComputable: boolean; loaded: number; total: number }) => void) | null } = { onprogress: null };
  onload: (() => void) | null = null;
  onerror: (() => void) | null = null;
  open() {}
  setRequestHeader() {}
  send(form: FormData) {
    const file = form.get("file") as File;
    const r = uploads[file.name] ?? { status: 415, body: { error: { code: "unsupported_format", message: "x" } } };
    setTimeout(() => {
      this.upload.onprogress?.({ lengthComputable: true, loaded: 1, total: 1 });
      this.status = r.status;
      this.responseText = JSON.stringify(r.body);
      this.onload?.();
    }, 0);
  }
}

const confirmed: { id: string; body: unknown }[] = [];
const deleted: string[] = [];

function json(status: number, body: unknown) {
  return new Response(body === null ? null : JSON.stringify(body), { status, headers: { "content-type": "application/json" } });
}

beforeEach(() => {
  confirmed.length = 0;
  deleted.length = 0;
  vi.stubGlobal("XMLHttpRequest", FakeXhr);
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string, init?: RequestInit) => {
      if (url.endsWith("/config")) {
        return json(200, {
          data: {
            timezone: "Europe/Berlin",
            user: { display_name: "Uli" },
            map: { tile_url: "", attribution: "" },
            import_limits: { max_file_size_mb: 50, max_points_per_session: 100000 },
            segments: { minimum_length_m: 20, route_edge_m: 1000 },
            sub_sport_catalog: {},
          },
        });
      }
      const m = /\/import\/jobs\/(\w+)(\/confirm)?$/.exec(url);
      if (m && init?.method === "DELETE") {
        deleted.push(m[1]!);
        return new Response(null, { status: 204 });
      }
      if (m && m[2]) {
        const body = JSON.parse(String(init?.body));
        confirmed.push({ id: m[1]!, body });
        const imported = (body.sessions as { action: string }[]).filter((s) => s.action !== "skip").map(() => "new-id");
        const original = Object.values(uploads)
          .map((u) => (u.body as { data?: ImportJob }).data)
          .find((j) => j?.id === m[1]);
        const sessions = original?.sessions ?? [];
        return json(202, { data: { ...job(m[1]!, "x", sessions), status: "done", state_token: null, imported_activity_ids: imported } });
      }
      return json(404, { error: { code: "not_found", message: "?" } });
    }),
  );
});

afterEach(() => vi.unstubAllGlobals());

function renderPage(handoff?: File[]) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <MemoryRouter initialEntries={[{ pathname: "/import", state: handoff ? { files: handoff } : null }]}>
        <ImportPage />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

function drop(names: string[]) {
  const files = names.map((n) => new File(["x"], n));
  fireEvent.change(screen.getByTestId("file-input"), { target: { files } });
}

describe("Importdialog", () => {
  it("speichert konfliktfreie Dateien sofort und wartet nur bei Duplikaten", async () => {
    renderPage();
    drop(["a.gpx", "b.gpx", "route.gpx", "fahrt.fit", "export.zip"]);
    await screen.findByText("Vermutetes Duplikat · b.gpx", { exact: false });

    // a.gpx ohne Konflikt und fahrt.fit (nur gesperrt) sind schon bestätigt, ohne Titel.
    expect(confirmed).toEqual([
      { id: "a", body: { state_token: "st_a", sessions: [{ session_index: 0, action: "import" }] } },
      { id: "c", body: { state_token: "st_c", sessions: [{ session_index: 0, action: "skip" }] } },
    ]);
    // Keine Eingabefelder für Titel
    expect(screen.queryByDisplayValue("Morgenrunde")).not.toBeInTheDocument();
    const imported = screen.getByText("Importiert", { selector: "h2", exact: false }).closest("section")!;
    expect(within(imported).getByText("Morgenrunde")).toBeInTheDocument();

    expect(screen.getByText("1 offen")).toBeInTheDocument();
    const locked = screen.getByText("Nicht importierbar", { selector: "h2" }).closest("section")!;
    expect(within(locked).getByText("Fahrzeug erkannt")).toBeInTheDocument();
    expect(within(locked).getByText(/geplante Route/)).toBeInTheDocument();
    expect(within(locked).getByText(/ZIP-Archive/)).toBeInTheDocument();
    expect(within(locked).getByText(/68 km\/h/)).toBeInTheDocument();

    const confirm = screen.getByRole("button", { name: "Entscheidungen bestätigen" });
    expect(confirm).toBeDisabled();
    fireEvent.click(screen.getByLabelText(/Vorhandene ersetzen/));
    fireEvent.click(screen.getByLabelText("Untertyp"));
    expect(confirm).toBeEnabled();

    fireEvent.click(confirm);
    await screen.findByText("Zu den Aktivitäten");
    expect(confirmed[2]).toEqual({
      id: "b",
      body: {
        state_token: "st_b",
        sessions: [
          {
            session_index: 0,
            action: "replace",
            replace_activity_id: "old",
            carry_over: { title: true, description: true, location: true, sport_class: true, sub_sport: false },
          },
        ],
      },
    });
    const kpi = (label: string) => screen.getAllByText(label).map((el) => el.closest("section")!).find((sec) => sec.querySelector(".num"))!;
    expect(within(kpi("Importiert")).getByText("1")).toBeInTheDocument();
    expect(within(kpi("Ersetzt")).getByText("1")).toBeInTheDocument();
    expect(within(kpi("Übersprungen")).getByText("0")).toBeInTheDocument();
    expect(within(kpi("Abgewiesen")).getByText("3")).toBeInTheDocument();
  });

  it("übernimmt auf dem Dashboard abgelegte Dateien genau einmal", async () => {
    renderPage([new File(["x"], "a.gpx")]);
    await screen.findByText("Zu den Aktivitäten");
    expect(confirmed).toHaveLength(1);
  });

  it("ohne Konflikt geht es direkt zum Ergebnis", async () => {
    renderPage();
    drop(["a.gpx"]);
    await screen.findByText("Zu den Aktivitäten");
    expect(confirmed).toHaveLength(1);
  });

  it("Offene verwerfen bricht nur die wartenden Vorgänge ab", async () => {
    renderPage();
    drop(["a.gpx", "b.gpx"]);
    await screen.findByText("Vermutetes Duplikat · b.gpx", { exact: false });
    fireEvent.click(screen.getByRole("button", { name: "Offene verwerfen" }));
    await screen.findByText("Zu den Aktivitäten");
    await waitFor(() => expect(deleted).toEqual(["b"]));
    expect(confirmed.map((c) => c.id)).toEqual(["a"]);
  });
});
