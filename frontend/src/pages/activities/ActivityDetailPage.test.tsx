import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { chooseOption, optionNames } from "../../components/chooseOption.test-helper";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { FlashProvider } from "../../components/Flash";
import { ActivityDetailPage } from "./ActivityDetailPage";
import type { Effort } from "../../api/client";
import { activity, config, json } from "./testData";

// Leaflet und Recharts brauchen Layout, das jsdom nicht hat; die Logik testet trackModel.
vi.mock("./ActivityMap", () => ({ ActivityMap: (p: { points: unknown[] }) => <div>Karte mit {p.points.length} Punkten</div> }));
vi.mock("./ActivityChart", () => ({ ActivityChart: () => <div>Diagramm</div> }));
vi.mock("./CreateSegmentDialog", () => ({ CreateSegmentDialog: (p: { kind: string }) => <div>Erstellen-Dialog {p.kind}</div> }));

let all: ReturnType<typeof activity>[] = [];
const initial = () => [
  activity({ id: "old", title: "Ältere", start_time: "2026-10-30T09:00:00.000Z" }),
  {
    ...activity({ id: "a1", title: "Runde Wald", description: "Mit Regen", location: "Taunus" }),
    training_load: { trimp: 187.4, aerobic: 161.2, anaerobic: 26.2, method: "samples" as const, zone_seconds: [60, 1800, 1200, 400, 140], zone_set: null },
  },
  activity({ id: "run", title: "Lauf", sport_class: "running", avg_speed_kmh: 12, start_time: "2026-11-02T09:00:00.000Z" }),
];
const calls: string[] = [];
const writes: { method: string; url: string; body: unknown }[] = [];
let patchError: unknown = null;
let efforts: Effort[] = [];
let suggestion: unknown = null;
let planMatch: unknown = { linked: null, candidates: [] };

function effort(over: Partial<Effort> = {}): Effort {
  return {
    id: "e1",
    segment_id: "s1",
    segment_name: "Anstieg Wald",
    activity_id: "a1",
    activity_title: "Runde Wald",
    attempt_position: 0,
    status: "valid",
    not_ranked_reason: null,
    start_time: "2026-11-01T09:10:00.000Z",
    end_time: "2026-11-01T09:14:31.200Z",
    duration_ms: 271_250,
    rank: 1,
    record_kind: "improvement",
    improvement_ms: 12_300,
    coverage_ratio: 1,
    avg_heart_rate_bpm: 150,
    max_heart_rate_bpm: 171,
    segment: { kind: "segment", category: "climb", sport_class: "cycling", length_m: 1520, elevation_gain_m: 88, average_grade_pct: 5.8, ranked_count: 7 },
    ...over,
  };
}

const track = {
  data: {
    sections: [
      {
        section_index: 0,
        point_count: 2,
        returned_point_count: 2,
        simplified: false,
        index: [0, 1],
        t_ms: [0, 1000],
        lat: [50, 50.0001],
        lon: [8, 8],
        ele_m: [100, 101],
        hr: [120, 121],
        dist_m: [0, 11],
      },
    ],
  },
};

beforeEach(() => {
  all = initial();
  calls.length = 0;
  writes.length = 0;
  patchError = null;
  efforts = [];
  suggestion = null;
  planMatch = { linked: null, candidates: [] };
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string, init?: RequestInit) => {
      calls.push(url);
      const method = init?.method ?? "GET";
      if (method !== "GET") {
        writes.push({ method, url, body: init?.body ? JSON.parse(String(init.body)) : undefined });
        const id = url.split("/").pop();
        if (method === "DELETE") {
          all = all.filter((x) => x.id !== id);
          return new Response(null, { status: 204 });
        }
        if (patchError) return json(400, patchError);
        if (method === "POST" && url.endsWith("/sub-sports")) {
          const b = JSON.parse(String(init?.body)) as { sport_class: string; name: string };
          return json(201, { data: { sport_class: b.sport_class, key: "custom_9", name: b.name, builtin: false, activity_count: 0, mapping_count: 0 } });
        }
        // Startkorrektur rechnet der echte Server um; hier bleibt der Start, die übrigen Felder werden übernommen.
        const patch = JSON.parse(String(init?.body)) as Record<string, unknown>;
        delete patch.start_time;
        all = all.map((x) => (x.id === id ? { ...x, ...patch, revision: x.revision + 1 } : x));
        return json(200, { data: all.find((x) => x.id === id) });
      }
      if (url.endsWith("/deletion-preview")) {
        return json(200, { data: { object_type: "activity", effort_count: 0, affected_segment_count: 0, original_file_removed: true } });
      }
      if (url.endsWith("/config")) return json(200, config);
      const u = new URL(url, "http://x");
      if (u.pathname.endsWith("/track")) return json(200, track);
      if (u.pathname.endsWith("/location-suggestion")) return json(200, { data: suggestion });
      if (u.pathname.endsWith("/plan")) return json(200, { data: planMatch });
      if (u.pathname.endsWith("/efforts"))
        return json(200, { data: efforts, meta: { total: efforts.length, calculation: { status: "current", version_id: "v", published_at: null } } });
      const one = /\/activities\/(\w+)$/.exec(u.pathname);
      if (one) {
        const a = all.find((x) => x.id === one[1]);
        return a ? json(200, { data: a }) : json(404, { error: { code: "activity_not_found", message: "nope" } });
      }
      const asc = u.searchParams.get("sort") === "start_time";
      const data = asc ? all : [...all].reverse();
      return json(200, { data, meta: { total: 3, limit: 50, offset: 0 } });
    }),
  );
});

function setup(id: string, state?: unknown) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter initialEntries={["/aktivitaeten", { pathname: `/aktivitaeten/${id}`, state }]} initialIndex={1}>
        <FlashProvider>
        <Routes>
          <Route path="/aktivitaeten" element={<div>Liste</div>} />
          <Route path="/aktivitaeten/:id" element={<ActivityDetailPage />} />
        </Routes>
        </FlashProvider>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe("Aktivitätsdetail", () => {
  it("Kopfkarte, Kennzahlen, Karte, Diagramm und Beschreibung", async () => {
    setup("a1", { from: "list" });
    expect(await screen.findByRole("heading", { name: "Runde Wald" })).toBeInTheDocument();
    // Untertyp als Text, sein Symbol groß im Kopf statt des Klassensymbols
    const meta = screen.getByTestId("detail-meta");
    expect(meta.textContent).toBe("01.11.2026 10:00·Radfahren·MTB·Taunus");
    expect(meta.querySelector("[data-glyph]")).toBeNull();
    expect(screen.getByRole("img", { name: "Radfahren · MTB" })).toHaveAttribute("data-glyph", "mtb");
    expect(screen.getByText("25,00")).toBeInTheDocument();
    expect(screen.getByText("aus den Messpunkten berechnet")).toBeInTheDocument();
    expect(await screen.findByText("Karte mit 2 Punkten")).toBeInTheDocument();
    expect(screen.getByText("Diagramm")).toBeInTheDocument();
    expect(screen.getByText("Mit Regen")).toBeInTheDocument();
    // Kachel Belastung neben der Herzfrequenz
    expect(screen.getByText("aerob 161 · anaerob 26").parentElement).toHaveTextContent(/^Belastung187/);
    expect(calls).toContain("/api/v1/activities/a1/track?max_points=2000");
  });

  it("blättert zu älteren und neueren Aktivitäten, Zurück führt zur Liste", async () => {
    setup("a1", { from: "list" });
    await screen.findByRole("heading", { name: "Runde Wald" });
    const newer = screen.getByRole("button", { name: "Nächste Aktivität" });
    await vi.waitFor(() => expect(newer).toBeEnabled());
    expect(screen.getByRole("button", { name: "Vorherige Aktivität" })).toBeEnabled();
    fireEvent.click(newer);
    expect(await screen.findByRole("heading", { name: "Lauf" })).toBeInTheDocument();
    expect(screen.getByText("Pace")).toBeInTheDocument();
    expect(screen.getByText("5:00")).toBeInTheDocument();
    await vi.waitFor(() => expect(screen.getByRole("button", { name: "Nächste Aktivität" })).toBeDisabled());
    fireEvent.click(screen.getByRole("button", { name: "Zurück zur Aktivitätenliste" }));
    expect(await screen.findByText("Liste")).toBeInTheDocument();
  });

  it("vom Dashboard geöffnet führt Zurück zum Dashboard", async () => {
    setup("a1", { from: "dashboard" });
    await screen.findByRole("heading", { name: "Runde Wald" });
    fireEvent.click(screen.getByRole("button", { name: "Zurück zum Dashboard" }));
    expect(await screen.findByText("Liste")).toBeInTheDocument();
  });

  it("ohne Herkunft ein Link zur Liste, unbekannte Aktivität", async () => {
    setup("gibtsnicht");
    expect(await screen.findByText("Diese Aktivität gibt es nicht (mehr).")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Zur Aktivitätenliste" })).toHaveAttribute("href", "/aktivitaeten");
  });

  it("bearbeitet Titel, Ort und Sportklasse, schickt nur Änderungen", async () => {
    setup("a1", { from: "list" });
    await screen.findByRole("heading", { name: "Runde Wald" });
    fireEvent.click(screen.getByRole("button", { name: "Bearbeiten" }));
    const dialog = screen.getByRole("dialog", { name: "Aktivität bearbeiten" });
    const save = within(dialog).getByRole("button", { name: "Speichern" });
    expect(save).toBeDisabled();
    fireEvent.change(within(dialog).getByLabelText("Titel"), { target: { value: "Runde Feldberg" } });
    fireEvent.change(within(dialog).getByLabelText("Ort"), { target: { value: "" } });
    chooseOption(dialog, "Sportklasse", "E-Bike");
    expect(within(dialog).getByText("Kennzahlen und Bestenlisten werden neu berechnet.")).toBeInTheDocument();
    fireEvent.click(save);
    expect(await screen.findByRole("heading", { name: "Runde Feldberg" })).toBeInTheDocument();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent("Änderungen gespeichert.");
    // E-Bike kennt den Untertyp mtb nicht: er entfällt (wie im Backend).
    expect(writes).toEqual([
      { method: "PATCH", url: "/api/v1/activities/a1", body: { title: "Runde Feldberg", location: null, sport_class: "ebike", sub_sport: null } },
    ]);
  });

  it("leerer Ort: Vorschlag ist vorbelegt und wird gespeichert (#22)", async () => {
    suggestion = { location: "Musterdorf", source: "own", distance_m: 56.4 };
    setup("run");
    await screen.findByRole("heading", { name: "Lauf" });
    fireEvent.click(screen.getByRole("button", { name: "Bearbeiten" }));
    const dialog = screen.getByRole("dialog");
    // Der Hinweis steht im selben <label> wie bei den übrigen Feldern
    await waitFor(() => expect(within(dialog).getByLabelText(/^Ort/)).toHaveValue("Musterdorf"));
    expect(within(dialog).getByText("Vorschlag aus deinen Aktivitäten und Strecken (56 m vom Start).")).toBeInTheDocument();
    fireEvent.click(within(dialog).getByRole("button", { name: "Speichern" }));
    await waitFor(() => expect(writes).toEqual([{ method: "PATCH", url: "/api/v1/activities/run", body: { location: "Musterdorf" } }]));
  });

  it("Ortsvorschlag von OpenStreetMap lässt sich überschreiben oder leeren", async () => {
    suggestion = { location: "Beispielstadt", source: "osm", distance_m: null };
    setup("run");
    await screen.findByRole("heading", { name: "Lauf" });
    fireEvent.click(screen.getByRole("button", { name: "Bearbeiten" }));
    const dialog = screen.getByRole("dialog");
    const field = within(dialog).getByLabelText("Ort");
    await waitFor(() => expect(field).toHaveValue("Beispielstadt"));
    expect(within(dialog).getByText("Vorschlag von OpenStreetMap.")).toBeInTheDocument();
    // Geleert bleibt leer, der Hinweis verschwindet, es gibt nichts zu speichern
    fireEvent.change(field, { target: { value: "" } });
    expect(field).toHaveValue("");
    expect(within(dialog).queryByText("Vorschlag von OpenStreetMap.")).not.toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "Speichern" })).toBeDisabled();
  });

  it("vorhandener Ort: kein Vorschlag abgefragt", async () => {
    setup("a1");
    await screen.findByRole("heading", { name: "Runde Wald" });
    fireEvent.click(screen.getByRole("button", { name: "Bearbeiten" }));
    expect(within(screen.getByRole("dialog")).getByLabelText("Ort")).toHaveValue("Taunus");
    expect(calls.some((c) => c.includes("location-suggestion"))).toBe(false);
  });

  it("Untertyp aus dem Katalog wählen oder neu anlegen", async () => {
    setup("a1");
    await screen.findByRole("heading", { name: "Runde Wald" });
    expect(screen.getByTestId("detail-meta")).toHaveTextContent("MTB");
    fireEvent.click(screen.getByRole("button", { name: "Bearbeiten" }));
    const dialog = screen.getByRole("dialog");
    // Eigene Auswahlliste mit Symbolen
    expect(optionNames(dialog, "Untertyp")).toContain("Rennrad");
    chooseOption(dialog, "Untertyp", "Neuer Untertyp …");
    const save = within(dialog).getByRole("button", { name: "Speichern" });
    expect(save).toBeDisabled();
    fireEvent.change(within(dialog).getByLabelText("Name des neuen Untertyps"), { target: { value: " Gravel " } });
    fireEvent.click(save);
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(writes).toEqual([
      { method: "POST", url: "/api/v1/sub-sports", body: { sport_class: "cycling", name: "Gravel" } },
      { method: "PATCH", url: "/api/v1/activities/a1", body: { sub_sport: "custom_9" } },
    ]);
  });

  it("doppelte Herbststunde fragt nach dem Auftreten", async () => {
    setup("a1");
    await screen.findByRole("heading", { name: "Runde Wald" });
    fireEvent.click(screen.getByRole("button", { name: "Bearbeiten" }));
    const dialog = screen.getByRole("dialog");
    fireEvent.click(within(dialog).getByRole("button", { name: "Start und Gesamtzeit korrigieren" }));
    fireEvent.change(within(dialog).getByLabelText("Start"), { target: { value: "2026-10-25T02:30:00" } });
    patchError = { error: { code: "local_time_ambiguous", message: "ambiguous" } };
    fireEvent.click(within(dialog).getByRole("button", { name: "Speichern" }));
    const occurrence = await within(dialog).findByLabelText("Doppelte Stunde der Zeitumstellung");
    expect(within(dialog).getByText(/gibt es wegen der Zeitumstellung zweimal/)).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "Speichern" })).toBeDisabled();
    patchError = null;
    fireEvent.change(occurrence, { target: { value: "first" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Speichern" }));
    await vi.waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(writes[1]?.body).toEqual({ start_time: { local: "2026-10-25T02:30:00", occurrence: "first" } });
  });

  it("zeigt korrigierte Zeiten und die Originalwerte", async () => {
    all = [activity({ start_time_origin: "manual_corrected", start_time_original: "2026-11-01T08:00:00.000Z", total_time_origin: "manual_corrected", total_time_original_ms: 3_000_000 })];
    setup("a1");
    expect(await screen.findByText(/01\.11\.2026 10:00 \(korrigiert\)/)).toBeInTheDocument();
    expect(screen.getByText("manuell korrigiert")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Bearbeiten" }));
    expect(screen.getByText("Ursprünglich: Start 01.11.2026 09:00, Gesamtzeit 50:00")).toBeInTheDocument();
    fireEvent.keyDown(document, { key: "Escape" });
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("löscht nach Vorschau und kehrt zur Liste zurück", async () => {
    setup("a1", { from: "list" });
    await screen.findByRole("heading", { name: "Runde Wald" });
    fireEvent.click(screen.getByRole("button", { name: "Löschen" }));
    const dialog = screen.getByRole("dialog", { name: "Aktivität löschen?" });
    expect(await within(dialog).findByText("Die Originaldatei wird ebenfalls entfernt.")).toBeInTheDocument();
    expect(within(dialog).queryByText(/Segmentversuche/)).not.toBeInTheDocument();
    fireEvent.click(within(dialog).getByRole("button", { name: "Endgültig löschen" }));
    expect(await screen.findByText("Liste")).toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent("„Runde Wald“ gelöscht.");
    expect(writes).toEqual([{ method: "DELETE", url: "/api/v1/activities/a1", body: undefined }]);
  });

  it("Löschen abbrechen ändert nichts", async () => {
    setup("a1");
    await screen.findByRole("heading", { name: "Runde Wald" });
    fireEvent.click(screen.getByRole("button", { name: "Löschen" }));
    fireEvent.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Abbrechen" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(writes).toEqual([]);
  });

  it("zeigt Segmentversuche, Top-Ergebnisse und öffnet die Erstellen-Dialoge", async () => {
    efforts = [
      effort(),
      effort({ id: "e2", segment_id: "s2", segment_name: "Ortsdurchfahrt", rank: 4, record_kind: null, improvement_ms: null }),
      effort({ id: "e3", segment_id: "s3", segment_name: "Kehre", status: "not_ranked", not_ranked_reason: "time_invalid", rank: null, record_kind: null, improvement_ms: null, duration_ms: null }),
    ];
    setup("a1");
    expect(await screen.findByText("Top-Ergebnisse")).toBeInTheDocument();
    expect(screen.getByText("Neue Bestzeit, 0:12,3 schneller")).toBeInTheDocument();
    expect(screen.getAllByText("4:31,2")).toHaveLength(3); // Top-Ergebnis und zwei Zeilen
    expect(screen.getByText("1 von 7")).toBeInTheDocument();
    expect(screen.getByText("4 von 7")).toBeInTheDocument();
    expect(screen.getByText("nicht gewertet: keine belastbare Zeit")).toBeInTheDocument();
    expect(screen.getByText("−0:12,3")).toBeInTheDocument();
    expect(screen.getAllByText("1,52 km · 88 m ↑ · 5,8 %")).toHaveLength(3);
    // Kennzahl Segmente: Zahl der Versuche
    expect(screen.getByTestId("kpi-segments")).toHaveTextContent(/Segmente\s*3/);

    fireEvent.click(screen.getByRole("button", { name: "Segment erstellen" }));
    expect(screen.getByText("Erstellen-Dialog segment")).toBeInTheDocument();
  });

  it("bietet „Strecke erstellen“ an, solange die Aktivität zu keiner Strecke passt", async () => {
    setup("a1");
    const route = await screen.findByRole("button", { name: "Strecke erstellen" });
    await vi.waitFor(() => expect(route).toBeEnabled());
    expect(screen.getByText("Passt zu keiner Strecke.")).toBeInTheDocument();
    fireEvent.click(route);
    expect(screen.getByText("Erstellen-Dialog route")).toBeInTheDocument();
  });

  it("zeigt die passende Strecke oben statt in der Segmentliste", async () => {
    const routeInfo = { kind: "route" as const, category: "loop" as const, sport_class: "cycling" as const, length_m: 25_000, elevation_gain_m: 300, average_grade_pct: 0, ranked_count: 3, best_ms: 3_000_000 };
    efforts = [
      effort({ id: "r", segment_id: "r1", segment_name: "Hausrunde", rank: 2, record_kind: null, improvement_ms: null, duration_ms: 3_100_000, segment: routeInfo }),
      effort(),
    ];
    setup("a1");
    const top = within(await screen.findByRole("region", { name: "Strecke" }));
    expect(await top.findByRole("link", { name: /Hausrunde/ })).toHaveAttribute("href", "/strecken/r1");
    expect(top.getByText("Rang 2 von 3 · Bestzeit 50:00,0")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Strecke erstellen" })).not.toBeInTheDocument();
    expect(within(screen.getByRole("region", { name: "Segmente" })).getByRole("button", { name: "Segment erstellen" })).toBeEnabled();
    // Segmenttabelle und Kennzahl ohne Strecke
    expect(screen.getAllByText("Hausrunde")).toHaveLength(1);
    expect(screen.getByTestId("kpi-segments")).toHaveTextContent(/Segmente\s*1\s*davon 1 PRs/);
  });

  it("zeigt höchstens drei Top-Ergebnisse, Bestzeiten zuerst", async () => {
    efforts = [
      effort({ id: "a", segment_id: "sa", segment_name: "Platz drei", rank: 3, record_kind: null, improvement_ms: null }),
      effort({ id: "b", segment_id: "sb", segment_name: "Platz zwei", rank: 2, record_kind: null, improvement_ms: null }),
      effort({ id: "c", segment_id: "sc", segment_name: "Zweiter zwei", rank: 2, record_kind: null, improvement_ms: null }),
      effort({ id: "d", segment_id: "sd", segment_name: "Bestzeit", record_kind: "first_best" }),
    ];
    setup("a1");
    const region = within(await screen.findByRole("region", { name: "Segmente" }));
    await vi.waitFor(() => expect(region.getAllByRole("link")).toHaveLength(3));
    expect(region.getAllByRole("link").map((l) => l.textContent)).toEqual([
      expect.stringContaining("Bestzeit"),
      expect.stringContaining("Platz zwei"),
      expect.stringContaining("Zweiter zwei"),
    ]);
  });
});
