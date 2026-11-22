import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { chooseOption } from "../../components/chooseOption.test-helper";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";
import type { TrackSection } from "../../api/client";
import { CreateSegmentDialog } from "./CreateSegmentDialog";
import { activity, json } from "./testData";

// Leaflet braucht Layout, das jsdom nicht hat; Einrasten auf der Karte testet createModel.
vi.mock("./SelectionMap", () => ({ SelectionMap: () => <div>Karte</div> }));

const N = 60; // gut 33,4 m Abstand: knapp 2 km
const STEP_M = 33.358;
function section(): TrackSection {
  const r = Array.from({ length: N }, (_, i) => i);
  return {
    section_index: 0,
    point_count: N + 1,
    returned_point_count: N + 1,
    simplified: false,
    // Punkt 0 ohne Position: die Indizes der API verschieben sich um eins
    index: [0, ...r.map((i) => i + 1)],
    t_ms: [Date.parse("2026-11-01T09:00:00Z"), ...r.map((i) => Date.parse("2026-11-01T09:00:00Z") + (i + 1) * 10_000)],
    lat: [null, ...r.map((i) => 47 + i * 0.0003)],
    lon: [null, ...r.map(() => 11)],
    ele_m: [null, ...r.map((i) => 500 + i)],
    hr: [null, ...r.map(() => 120)],
    dist_m: [null, ...r.map((i) => i * STEP_M)],
  };
}

const posts: { url: string; body: Record<string, unknown> }[] = [];
const calls: string[] = [];
let createError: unknown = null;
let polls = 0;
let existing: unknown[] = [];
let similarRoutes: unknown[] = [];
const similarBodies: Record<string, unknown>[] = [];

beforeEach(() => {
  posts.length = 0;
  calls.length = 0;
  createError = null;
  polls = 0;
  existing = [];
  similarRoutes = [];
  similarBodies.length = 0;
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string, init?: RequestInit) => {
      calls.push(url);
      if (url.endsWith("/segments/similar")) {
        similarBodies.push(JSON.parse(String(init?.body)) as Record<string, unknown>);
        return json(200, { data: similarRoutes, meta: {} });
      }
      if (init?.method === "POST") {
        posts.push({ url, body: JSON.parse(String(init.body)) as Record<string, unknown> });
        if (createError) return json(409, createError);
        return json(201, { data: { id: "s9", name: "Anstieg", history: { status: "running", processed_items: 0, total_items: 3 } }, meta: {} });
      }
      if (url.includes("/track")) return json(200, { data: { sections: [section()] } });
      if (url.includes("/segments?")) return json(200, { data: existing, meta: { total: existing.length, limit: 200, offset: 0 } });
      if (url.endsWith("/segments/s9")) {
        polls++;
        const done = polls > 1;
        return json(200, {
          data: {
            id: "s9",
            name: "Anstieg",
            history: { status: done ? "complete" : "running", processed_items: done ? 3 : 1, total_items: 3 },
            summary: { attempt_count: 3, valid_attempt_count: 2, best: { duration_ms: 271_250 } },
          },
          meta: {},
        });
      }
      return json(404, { error: { code: "not_found", message: "?" } });
    }),
  );
});

function setup(kind: "segment" | "route", sport: "cycling" | "unknown" = "cycling", minLengthM = 20, location: string | null = null) {
  const onClose = vi.fn();
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={qc}>
      <MemoryRouter>
        <CreateSegmentDialog
        activity={activity({ sport_class: sport, location })}
        kind={kind}
        tileUrl="x"
        attribution="y"
        minLengthM={minLengthM}
        routeEdgeM={1000}
        onClose={onClose}
        />
      </MemoryRouter>
    </QueryClientProvider>,
  );
  return onClose;
}

/** Dialog, sobald der Abschnitt geladen ist. */
async function ready(title: string) {
  await screen.findByTestId("point-s");
  return screen.getByRole("dialog", { name: title });
}

describe("Segment erstellen", () => {
  it("lädt den ganzen Abschnitt, übernimmt die Sportklasse und legt das Segment an", async () => {
    setup("segment");
    const dialog = await ready("Segment erstellen");
    expect(calls).toContain("/api/v1/activities/a1/track?section=0");
    expect(within(dialog).getByText("Aus der Aktivität übernommen (Radfahren), änderbar.")).toBeInTheDocument();
    const create = within(dialog).getByRole("button", { name: "Segment erstellen" });
    expect(create).toBeDisabled();
    expect(screen.getByTestId("create-message")).toHaveTextContent("Bitte einen Namen eingeben.");

    // Vorgabe: 40 % bis 48 % der Aufzeichnung; ein Punkt vor, bei Dauerdruck weiter (Uli 12.11.2026)
    const slider = within(dialog).getByRole("slider", { name: "Start" });
    const at = () => Number(slider.getAttribute("aria-valuenow"));
    const start = at();
    const forward = within(dialog).getAllByRole("button", { name: "Einen Punkt vor" })[0]!;
    // Finger: langes Drücken öffnet kein Kontextmenü, auch nicht auf den grünen/roten Reglern
    expect(fireEvent.contextMenu(forward)).toBe(false);
    expect(fireEvent.contextMenu(slider)).toBe(false);
    expect(fireEvent.contextMenu(within(dialog).getByRole("slider", { name: "Ziel" }))).toBe(false);
    expect(within(dialog).getByTestId("selection-controls")).toHaveClass("tc-no-callout");
    // Eingabefelder behalten ihr Menü (Einfügen)
    expect(fireEvent.contextMenu(within(dialog).getByRole("textbox", { name: "Name" }))).toBe(true);
    fireEvent.pointerDown(forward, { button: 0 });
    fireEvent.pointerUp(forward);
    expect(at()).toBe(start + 1);
    fireEvent.pointerDown(forward, { button: 0 });
    await act(() => new Promise((done) => setTimeout(done, 650)));
    fireEvent.pointerUp(forward);
    // 400 ms bis zur Wiederholung, dann alle 70 ms: weitere Punkte, nach dem Loslassen Stillstand
    const held = at();
    expect(held - start).toBeGreaterThanOrEqual(3);
    await act(() => new Promise((done) => setTimeout(done, 200)));
    expect(at()).toBe(held);
    // Kurze Auswahl auf langer Fahrt: vergrößertes Profil um die Auswahl
    expect(within(dialog).getByTestId("detail-profile")).toBeInTheDocument();

    fireEvent.change(within(dialog).getByPlaceholderText("z. B. Anstieg Wald"), { target: { value: " Anstieg " } });
    expect(create).toBeEnabled();
    fireEvent.click(create);
    expect(await screen.findByText(/Auswertung läuft … 0 von 3|Auswertung läuft … 1 von 3/)).toBeInTheDocument();
    const body = posts[0]!.body;
    expect(posts[0]!.url).toBe("/api/v1/segments/from-activity");
    expect(body).toMatchObject({ activity_id: "a1", section_index: 0, name: "Anstieg", sport_class: "cycling", kind: "segment", description: null });
    // Indizes im Abschnitt, nicht in der Liste der Punkte mit Position
    expect(body.start_index).toBe(held + 1);
    expect(body.end_index).toBeGreaterThan(body.start_index as number);
    expect(await screen.findByText(/Auswertung fertig: 3 Versuche, davon 2 gewertet\. Bestzeit 4:31,2\./, {}, { timeout: 3000 })).toBeInTheDocument();
  });

  it("verlangt eine Sportklasse, wenn die Aktivität keine hat", async () => {
    setup("segment", "unknown");
    const dialog = await ready("Segment erstellen");
    fireEvent.change(within(dialog).getByPlaceholderText("z. B. Anstieg Wald"), { target: { value: "A" } });
    expect(screen.getByTestId("create-message")).toHaveTextContent("Bitte eine Sportklasse wählen.");
    chooseOption(dialog, "Sportklasse", "Laufen");
    expect(within(dialog).getByRole("button", { name: "Segment erstellen" })).toBeEnabled();
  });

  it("meldet zu kurze Auswahl", async () => {
    setup("segment", "cycling", 50);
    const dialog = await ready("Segment erstellen");
    const end = within(dialog).getByRole("slider", { name: "Ziel" });
    for (let i = 0; i < 6; i++) fireEvent.keyDown(end, { key: "ArrowLeft" });
    expect(screen.getByTestId("create-message")).toHaveTextContent("Zu kurz: 33 m, mindestens 50 m.");
  });
});

describe("Strecke erstellen", () => {
  it("beginnt 500 m vom Rand, begrenzt auf den Rand und übernimmt den Titel", async () => {
    setup("route");
    const dialog = await ready("Strecke erstellen");
    expect(within(dialog).getByDisplayValue("Runde Wald")).toBeInTheDocument();
    // 500 m / 33,4 m ≈ Punkt 15
    expect(screen.getByTestId("point-s")).toHaveTextContent("500 m");
    // Start weit nach hinten schieben: bleibt höchstens 965 m vom Anfang (halbe Länge minus Mindestlänge)
    fireEvent.keyDown(within(dialog).getByRole("slider", { name: "Start" }), { key: "PageUp" });
    expect(screen.getByTestId("point-s")).toHaveTextContent("934 m");
    // Strecken haben einen Ort, aber keine Steigung
    expect(within(dialog).queryByText("Steigung Ø")).not.toBeInTheDocument();
    fireEvent.change(within(dialog).getByLabelText("Ort"), { target: { value: " Aischfeld " } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Strecke erstellen" }));
    await screen.findByText(/Auswertung/);
    expect(posts[0]!.body).toMatchObject({ kind: "route", name: "Runde Wald", start_index: 29, end_index: 45, location: "Aischfeld" });
  });

  it("belegt den Ort aus der Aktivität vor", async () => {
    setup("route", "cycling", 20, "Seebach");
    const dialog = await ready("Strecke erstellen");
    expect(within(dialog).getByLabelText("Ort")).toHaveValue("Seebach");
    fireEvent.click(within(dialog).getByRole("button", { name: "Strecke erstellen" }));
    await screen.findByText(/Auswertung/);
    expect(posts[0]!.body).toMatchObject({ kind: "route", location: "Seebach" });
  });

  it("weist auf eine Strecke mit fast gleichem Verlauf hin (Backend)", async () => {
    similarRoutes = [{ id: "r7", name: "Alte Runde", length_m: 980, overlap: 0.96 }];
    setup("route");
    const dialog = await ready("Strecke erstellen");
    const hint = await within(dialog).findByTestId("create-similar", {}, { timeout: 2000 });
    expect(hint).toHaveTextContent("Eine ähnliche Strecke gibt es schon: Alte Runde");
    expect(within(hint).getByRole("link", { name: "Alte Runde" })).toHaveAttribute("href", "/strecken/r7");
    // Vorgabe 500 m vom Rand als Indizes im Abschnitt; keine Endpunktprüfung im Browser
    expect(similarBodies[0]).toEqual({ activity_id: "a1", section_index: 0, start_index: 16, end_index: 45, sport_class: "cycling" });
    expect(calls.some((c) => c.includes("/segments?kind=route"))).toBe(false);
  });

  it("zeigt die Ablehnung, wenn die Aktivität schon zu einer Strecke passt", async () => {
    createError = { error: { code: "route_already_matched", message: "x" } };
    setup("route");
    const dialog = await ready("Strecke erstellen");
    fireEvent.click(within(dialog).getByRole("button", { name: "Strecke erstellen" }));
    expect(await screen.findByText("Diese Aktivität passt schon zu einer Strecke.")).toBeInTheDocument();
  });
});
