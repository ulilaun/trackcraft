import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { chooseOption, optionNames } from "../../components/chooseOption.test-helper";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter, Route, Routes, useLocation } from "react-router-dom";
import { FlashProvider } from "../../components/Flash";
import { config, json } from "../activities/testData";
import { SegmentDetailPage } from "./SegmentDetailPage";
import { SegmentListPage } from "./SegmentListPage";
import type { Effort, Season } from "../../api/client";
import { attempts, effort, segment, track } from "./testData";

// Leaflet und Recharts brauchen Layout, das jsdom nicht hat; die Logik testet segmentModel.
vi.mock("../activities/ActivityMap", () => ({ ActivityMap: (p: { points: unknown[] }) => <div>Karte mit {p.points.length} Punkten</div> }));
vi.mock("./SegmentCharts", () => ({
  ElevationProfile: () => <div>Profil</div>,
  ProgressionChart: (p: { data: unknown[]; hoverId?: string | null; onHover?: (id: string | null) => void }) => (
    <div>
      Entwicklung mit {p.data.length} Punkten{p.hoverId ? ` · Hover ${p.hoverId}` : ""}
      <button type="button" onMouseEnter={() => p.onHover?.("e1")}>
        Balken e1
      </button>
    </div>
  ),
}));

const calls: string[] = [];
const writes: { method: string; url: string; body: unknown }[] = [];
let current = segment();
let deleted = false;
let attemptList: Effort[] = attempts();
let seasonList: Season[] = [];
const season = (p: Partial<Season>): Season => ({
  id: "s",
  name: "Saison",
  start_date: "2026-01-01",
  end_date: "2026-12-31",
  state: "current",
  created_at: "2026-11-13T08:00:00.000Z",
  updated_at: "2026-11-13T08:00:00.000Z",
  ...p,
});

function Where() {
  const l = useLocation();
  return <div data-testid="where">{l.pathname + l.search}</div>;
}

function setup(entry: string | { pathname: string; state: unknown }) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter initialEntries={[entry]}>
        <FlashProvider>
          <Routes>
            <Route path="/segmente" element={<SegmentListPage key="segment" kind="segment" />} />
            <Route path="/segmente/:id" element={<SegmentDetailPage />} />
            <Route path="/strecken" element={<SegmentListPage key="route" kind="route" />} />
            <Route path="/strecken/:id" element={<SegmentDetailPage />} />
            <Route path="/aktivitaeten/:id" element={<div>Aktivität</div>} />
            <Route path="/aktivitaeten" element={<div>Aktivitätenliste</div>} />
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
  current = segment();
  deleted = false;
  attemptList = attempts();
  seasonList = [];
  // Bestenliste nach Jahr: fester heutiger Tag
  vi.useFakeTimers({ toFake: ["Date"] });
  vi.setSystemTime(new Date("2026-11-12T10:00:00Z"));
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string, init?: RequestInit) => {
      const method = init?.method ?? "GET";
      if (url.endsWith("/config")) return json(200, config);
      if (method !== "GET") {
        const body: unknown = init?.body ? JSON.parse(String(init.body)) : null;
        writes.push({ method, url, body });
        if (method === "DELETE") {
          deleted = true;
          return new Response(null, { status: 204 });
        }
        current = { ...current, ...(body as object) };
        return json(200, { data: current, meta: { calculation: { status: "current", version_id: "v", published_at: null } } });
      }
      calls.push(url);
      const meta = { calculation: { status: "current", version_id: "v", published_at: null } };
      if (url.includes("/deletion-preview")) {
        return json(200, { data: { object_type: "segment", effort_count: 3, affected_segment_count: 0, original_file_removed: null } });
      }
      if (url.endsWith("/seasons")) {
        const def = seasonList.find((x) => x.state === "current") ?? seasonList.find((x) => x.state === "ended") ?? null;
        return json(200, { data: seasonList, meta: { total: seasonList.length, default_season_id: def?.id ?? null, today: "2026-11-12" } });
      }
      if (url.includes("/track")) return json(200, track);
      if (url.includes("/attempts")) return json(200, { data: attemptList, meta: { total: attemptList.length, limit: 200, offset: 0, ...meta } });
      if (url.includes("/segments?")) {
        const route = url.includes("kind=route");
        const data = deleted ? [] : [route ? segment({ id: "r1", name: "Hausrunde", kind: "route", length_m: 42_000 }) : current];
        return json(200, {
          data,
          meta: { total: data.length, limit: 50, offset: 0, summary: { count: data.length, attempt_count: 3, length_m: route ? 42_000 : 1520, improvement_count: 1 } },
        });
      }
      if (url.endsWith("/segments/weg")) return json(404, { error: { code: "segment_not_found", message: "Segment not found" } });
      return json(200, { data: current, meta });
    }),
  );
});

afterEach(() => {
  vi.useRealTimers();
});

describe("Segmentliste", () => {
  it("zeigt Kennzahlen und Zeile der Segmente", async () => {
    setup("/segmente");
    const row = (await screen.findByText("Anstieg Wald")).closest("tr")!;
    expect(calls[0]).toBe("/api/v1/segments?kind=segment&sort=name&limit=50&offset=0");
    expect(within(row).getByText("1,52 km")).toBeInTheDocument();
    expect(within(row).getByText("5,8 %")).toBeInTheDocument();
    expect(within(row).getByText("Anstieg")).toBeInTheDocument();
    expect(within(row).getByText("4:31,2")).toBeInTheDocument();
    expect(within(row).getByText("01.11.26")).toBeInTheDocument();
    expect(screen.getByText("PR-Verbesserungen")).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Segmente" })).toBeInTheDocument();
  });

  it("Strecken haben eine eigene Seite mit Sportklasse und Sortierung; die Zeile führt zur Strecke", async () => {
    setup("/strecken");
    expect(await screen.findByText("Hausrunde")).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Strecken" })).toBeInTheDocument();
    expect(calls[0]).toBe("/api/v1/segments?kind=route&sort=name&limit=50&offset=0");
    expect(screen.getByText("Hausrunde").getAttribute("href")).toBe("/strecken/r1");
    // Strecken: Ort statt Steigung, Kategorien der Strecken als Filter
    expect(screen.getByText("Ort", { selector: "th" })).toBeInTheDocument();
    expect(screen.queryByText("Steigung", { selector: "th" })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Rundkurs" }));
    await waitFor(() => expect(calls.at(-1)).toBe("/api/v1/segments?kind=route&category=loop&sort=name&limit=50&offset=0"));
    expect(screen.getByTestId("where").textContent).toBe("/strecken?art=loop");
    expect(screen.queryByRole("button", { name: "Anstieg" })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Laufen" }));
    fireEvent.click(await screen.findByText("Länge", { selector: "th" }));
    await waitFor(() => expect(calls.at(-1)).toBe("/api/v1/segments?kind=route&sport_class=running&category=loop&sort=-length_m&limit=50&offset=0"));
  });

  it("Klick auf die Zeile öffnet die Segmentseite mit Rücksprung zur Liste", async () => {
    setup("/segmente");
    fireEvent.click((await screen.findByText("Anstieg Wald")).closest("tr")!);
    expect(await screen.findByText("Zurück zur Segmentliste")).toBeInTheDocument();
    expect(screen.getByTestId("where").textContent).toBe("/segmente/s1");
  });
});

describe("Segmentseite", () => {
  it("zeigt Kopf, Kennzahlen, Karte, Entwicklung und alle Versuche; ohne Herkunft Ø Zeit", async () => {
    setup("/segmente/s1");
    expect(await screen.findByRole("heading", { name: "Anstieg Wald" })).toBeInTheDocument();
    expect(screen.getByText("Anstieg")).toBeInTheDocument();
    expect(screen.getByText("· Radfahren")).toBeInTheDocument();
    expect(screen.getByText("Zur Segmentliste")).toBeInTheDocument();
    expect(await screen.findByText("Karte mit 3 Punkten")).toBeInTheDocument();
    expect(await screen.findByText("Entwicklung mit 2 Punkten")).toBeInTheDocument();
    expect(screen.getByText("Ø Zeit")).toBeInTheDocument();
    expect(screen.getByText("Ø 5,8 %")).toBeInTheDocument();
    expect(screen.getByText("2 gewertet · seit 01.10.2026")).toBeInTheDocument();
    expect(screen.getByText("01.11.26 · Rang 1 von 2")).toBeInTheDocument();
    const rows = screen.getAllByRole("row").slice(1);
    expect(rows).toHaveLength(3);
    expect(within(rows[0]!).getByText("PR")).toBeInTheDocument();
    // Bestzeit-Spalte: bei der absoluten Bestzeit nichts; Rückstand statt Ersparnis
    expect(within(rows[0]!).queryByText("Bestzeit 2026")).not.toBeInTheDocument();
    expect(within(rows[0]!).getByText("20,2 km/h")).toBeInTheDocument();
    expect(within(rows[1]!).getByText("+0:28,7")).toBeInTheDocument();
    expect(within(rows[1]!).queryByText("Erste Bestzeit")).not.toBeInTheDocument();
    expect(screen.getByText("Rückstand")).toBeInTheDocument();
    expect(within(rows[2]!).getByText("nicht gewertet: mehrdeutige Passage")).toBeInTheDocument();
    expect(screen.queryByText("Aktivitäten vergleichen")).not.toBeInTheDocument();
  });

  it("aus einer Aktivität: deren Versuch ist markiert, Klick auf einen Versuch öffnet die Aktivität", async () => {
    setup({ pathname: "/segmente/s1", state: { from: "activity", activityId: "a1" } });
    expect(await screen.findByText("Zurück zur Aktivität")).toBeInTheDocument();
    expect(await screen.findByText("Diese Aktivität")).toBeInTheDocument();
    expect(screen.getByText("Rang 2 von 2")).toBeInTheDocument();
    const mine = screen.getByText("diese Aktivität").closest("tr")!;
    expect(within(mine).getByText("Runde Wald")).toBeInTheDocument();
    // oranger Rahmen um die Zeile
    expect(mine.getAttribute("data-own")).toBe("true");
    expect(mine.className).toContain("outline-warn");
    fireEvent.click(screen.getByText("Feierabendrunde"));
    expect(await screen.findByText("Aktivität")).toBeInTheDocument();
    expect(screen.getByTestId("where").textContent).toBe("/aktivitaeten/a2");
  });

  it("Strecke: Vergleich führt in die Aktivitätenliste mit Streckenfilter; laufende Auswertung wird gezeigt", async () => {
    current = segment({
      id: "r1",
      name: "Hausrunde",
      kind: "route",
      category: "loop",
      category_auto: "loop",
      location: "Aischfeld",
      history: { status: "running", processed_items: 40, total_items: 212 },
    });
    setup("/segmente/r1");
    expect(await screen.findByText("Rundkurs")).toBeInTheDocument();
    expect(screen.getByText("· Aischfeld")).toBeInTheDocument();
    // keine Steigung bei Strecken
    expect(screen.queryByText("Ø 5,8 %")).not.toBeInTheDocument();
    // unter /segmente geöffnet: weiter zur Adresse der Strecke
    expect(screen.getByTestId("where").textContent).toBe("/strecken/r1");
    expect(screen.getByText("Zur Streckenliste")).toBeInTheDocument();
    expect(screen.getByRole("status").textContent).toContain("40 von 212");
    current = { ...current, history: { status: "complete", processed_items: 212, total_items: 212 } };
    fireEvent.click(screen.getByText("Aktivitäten vergleichen"));
    await waitFor(() => expect(screen.getByTestId("where").textContent).toBe("/aktivitaeten?zeitraum=alle&strecke=r1&sort=total_time_ms"));
  });

  it("Bearbeiten schickt nur geänderte Felder und meldet die Neuauswertung beim Artwechsel", async () => {
    setup("/segmente/s1");
    fireEvent.click(await screen.findByRole("button", { name: "Bearbeiten" }));
    const dialog = screen.getByRole("dialog");
    const save = within(dialog).getByRole("button", { name: "Speichern" });
    expect(save).toBeDisabled();
    fireEvent.change(within(dialog).getByLabelText("Name"), { target: { value: "" } });
    expect(within(dialog).getByText("Der Name darf nicht leer sein.")).toBeInTheDocument();
    fireEvent.change(within(dialog).getByLabelText("Name"), { target: { value: "Waldanstieg" } });
    chooseOption(dialog, "Art", "Strecke");
    expect(within(dialog).getByText(/alle Aktivitäten neu ausgewertet/)).toBeInTheDocument();
    fireEvent.click(save);
    await waitFor(() => expect(writes).toEqual([{ method: "PATCH", url: "/api/v1/segments/s1", body: { name: "Waldanstieg", kind: "route" } }]));
    expect(await screen.findByText("Gespeichert. Die Historie wird neu ausgewertet.")).toBeInTheDocument();
    expect(await screen.findByRole("heading", { name: "Waldanstieg" })).toBeInTheDocument();
  });

  it("Bearbeiten: Kategorie von Hand und Ort; Automatisch nennt die bestimmte", async () => {
    setup("/segmente/s1");
    fireEvent.click(await screen.findByRole("button", { name: "Bearbeiten" }));
    const dialog = screen.getByRole("dialog");
    // eigene Auswahlliste mit Kategorie-Symbolen
    const names = optionNames(dialog, "Kategorie");
    expect(names).toContain("Automatisch (Anstieg)");
    expect(names).not.toContain("Rundkurs");
    chooseOption(dialog, "Kategorie", "Wellig");
    fireEvent.change(within(dialog).getByLabelText("Ort"), { target: { value: "Aischfeld" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Speichern" }));
    await waitFor(() =>
      expect(writes).toEqual([{ method: "PATCH", url: "/api/v1/segments/s1", body: { location: "Aischfeld", category: "rolling" } }]),
    );
  });

  it("Bestenliste: gesamt oder dieses Jahr, Rückstand und HF-Dreiecke zur Bestzeit, Hover mit dem Diagramm verbunden", async () => {
    attemptList = [
      effort({ id: "e0", activity_id: "a0", activity_title: "Alte Bestzeit", start_time: "2025-07-01T09:00:00.000Z", duration_ms: 250_000, rank: 1, avg_heart_rate_bpm: 160 }),
      effort({ id: "e1", activity_id: "a1", activity_title: "Runde Wald", start_time: "2026-10-01T09:10:00.000Z", duration_ms: 300_000, rank: 3, avg_heart_rate_bpm: 150 }),
      effort({ id: "e2", activity_id: "a2", activity_title: "Feierabendrunde", start_time: "2026-11-01T09:10:00.000Z", duration_ms: 271_250, rank: 2, avg_heart_rate_bpm: 155 }),
    ];
    setup("/segmente/s1");
    await screen.findByText("Alte Bestzeit");
    let rows = screen.getAllByTestId("attempt-row");
    expect(rows.map((x) => within(x).getByRole("link").textContent)).toEqual(["Alte Bestzeit", "Feierabendrunde", "Runde Wald"]);
    // gesamt: Bezug ist die Bestzeit 2025 mit HF 160, beide HF niedriger
    expect(within(rows[1]!).getByText("+0:21,2")).toBeInTheDocument();
    expect(within(rows[1]!).getByTestId("hr-down")).toBeInTheDocument();
    expect(within(rows[1]!).getByText("Bestzeit 2026")).toBeInTheDocument();
    expect(screen.getByText("Entwicklung mit 3 Punkten")).toBeInTheDocument();

    // geteilter Knopf „Jahr | 2026 ▾“
    fireEvent.click(screen.getByRole("button", { name: "Jahr" }));
    rows = screen.getAllByTestId("attempt-row");
    expect(rows.map((x) => within(x).getByRole("link").textContent)).toEqual(["Feierabendrunde", "Runde Wald"]);
    expect(within(rows[0]!).getByText("1")).toBeInTheDocument();
    // dieses Jahr: Bezug Feierabendrunde (HF 155), Runde Wald liegt darunter
    expect(within(rows[1]!).getByText("+0:28,7")).toBeInTheDocument();
    expect(within(rows[1]!).getByTestId("hr-down")).toBeInTheDocument();
    expect(screen.getByText("Entwicklung mit 2 Punkten")).toBeInTheDocument();

    // Hover: Zeile → Diagramm, Diagramm → Zeile
    fireEvent.mouseEnter(rows[0]!);
    expect(screen.getByText("Entwicklung mit 2 Punkten · Hover e2")).toBeInTheDocument();
    expect(rows[0]!.getAttribute("data-hover")).toBe("true");
    fireEvent.mouseEnter(screen.getByRole("button", { name: "Balken e1" }));
    expect(screen.getAllByTestId("attempt-row")[1]!.getAttribute("data-hover")).toBe("true");
  });

  it("Bestenliste: früheres Jahr und Saison, Vorgabe ist die zuletzt beendete Saison", async () => {
    attemptList = [
      effort({ id: "e0", activity_id: "a0", activity_title: "Alte Bestzeit", start_time: "2025-07-01T09:00:00.000Z", duration_ms: 250_000, rank: 1 }),
      effort({ id: "e1", activity_id: "a1", activity_title: "Runde Wald", start_time: "2026-10-01T09:10:00.000Z", duration_ms: 300_000, rank: 3 }),
      effort({ id: "e2", activity_id: "a2", activity_title: "Feierabendrunde", start_time: "2026-11-01T09:10:00.000Z", duration_ms: 271_250, rank: 2 }),
    ];
    seasonList = [
      season({ id: "s26", name: "Saison 2026", start_date: "2026-03-01", end_date: "2026-10-31", state: "ended" }),
      season({ id: "s25", name: "Saison 2025", start_date: "2025-03-01", end_date: "2025-10-31", state: "ended" }),
    ];
    setup("/segmente/s1");
    await screen.findByText("Alte Bestzeit");
    const names = () => screen.getAllByTestId("attempt-row").map((x) => within(x).getByRole("link").textContent);
    // Spalte Bestzeit: Saison vor Jahr, bei der absoluten Bestzeit nichts
    const badge = (title: string) => {
      const row = screen.getAllByTestId("attempt-row").find((x) => within(x).queryByText(title))!;
      return within(row).queryByText(/^Bestzeit /)?.textContent ?? null;
    };
    expect(badge("Runde Wald")).toBe("Bestzeit Saison 2026");
    expect(badge("Feierabendrunde")).toBe("Bestzeit 2026");
    expect(badge("Alte Bestzeit")).toBeNull();

    // Pfeil am geteilten Knopf wählt ein anderes Jahr
    fireEvent.click(screen.getByRole("button", { name: "Jahr wählen" }));
    fireEvent.click(within(screen.getByRole("menu")).getByRole("menuitemradio", { name: "2025" }));
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Jahr" })).toHaveAttribute("aria-pressed", "true");
    expect(names()).toEqual(["Alte Bestzeit"]);

    // Saison: Name und Zustandspunkt rechts vom Trenner
    const seasonButton = screen.getByRole("button", { name: "Saison" });
    expect(seasonButton).toHaveAttribute("title", "Saison 2026 · beendet");
    expect(screen.getByRole("button", { name: "Saison wählen" }).querySelector('[data-state="ended"]')).not.toBeNull();
    fireEvent.click(seasonButton);
    expect(names()).toEqual(["Runde Wald"]);
    expect(seasonButton).toHaveAttribute("aria-pressed", "true");
    fireEvent.click(screen.getByRole("button", { name: "Saison wählen" }));
    expect(screen.getAllByRole("menuitemradio").map((o) => o.textContent)).toEqual(["Saison 2026", "Saison 2025"]);
    fireEvent.click(screen.getByRole("menuitemradio", { name: "Saison 2025" }));
    expect(names()).toEqual(["Alte Bestzeit"]);
    expect(screen.queryByRole("option", { name: "Saisons verwalten …" })).not.toBeInTheDocument();
  });

  it("ohne Saisons nur Gesamt und Jahr", async () => {
    setup("/segmente/s1");
    await screen.findByRole("button", { name: "Jahr" });
    expect(screen.getByRole("button", { name: "Gesamt" })).toBeInTheDocument();
    expect(screen.queryByText(/Saison/)).not.toBeInTheDocument();
  });

  it("Löschen zeigt die Folgen und führt zur Liste", async () => {
    setup("/segmente/s1");
    fireEvent.click(await screen.findByRole("button", { name: "Löschen" }));
    const dialog = screen.getByRole("dialog");
    expect(await within(dialog).findByText("3 Versuche mit Rängen und Bestzeiten entfallen.")).toBeInTheDocument();
    expect(within(dialog).getByText("Aktivitäten und Originaldateien bleiben erhalten.")).toBeInTheDocument();
    fireEvent.click(within(dialog).getByRole("button", { name: "Endgültig löschen" }));
    await waitFor(() => expect(writes).toEqual([{ method: "DELETE", url: "/api/v1/segments/s1", body: null }]));
    expect(await screen.findByText("„Anstieg Wald“ gelöscht.")).toBeInTheDocument();
    await waitFor(() => expect(screen.getByTestId("where").textContent).toBe("/segmente"));
  });

  it("unbekanntes Segment", async () => {
    setup("/segmente/weg");
    expect(await screen.findByText("Dieses Segment gibt es nicht (mehr).")).toBeInTheDocument();
  });
});
