import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";
import type { GarminActivity, GarminStatus } from "../../api/client";
import { json } from "../activities/testData";
import { GarminSettings } from "./GarminSettings";
import { finishedRun, garminState, intervalOptions, pollInterval, unsettledCount } from "./garminModel";

const base = (p: Partial<GarminStatus> = {}): GarminStatus => ({
  configured: true,
  key_set_at: "2026-11-14T18:00:00.000Z",
  enabled: false,
  start_date: null,
  interval_minutes: 30,
  backfill_done: false,
  running: false,
  progress: { phase: "", total: 0, processed: 0, imported: 0, linked: 0, locked: 0, rejected: 0, failed: 0 },
  last_run_started_at: null,
  last_run_finished_at: null,
  last_success_at: null,
  last_error: null,
  counts: { imported: 0, linked: 0, locked: 0, rejected: 0, failed: 0 },
  ...p,
});

describe("Garmin-Modell", () => {
  it("Zustand, Intervalle, Zähler", () => {
    expect(garminState(base({ configured: false }))).toBe("unconfigured");
    expect(garminState(base())).toBe("never");
    expect(garminState(base({ running: true, last_error: { code: "x", message: "y" } }))).toBe("running");
    expect(garminState(base({ last_error: { code: "x", message: "y" }, last_success_at: "2026-11-14T18:00:00Z" }))).toBe("error");
    expect(garminState(base({ last_success_at: "2026-11-14T18:00:00Z" }))).toBe("ok");
    expect(intervalOptions(30)).toEqual([15, 30, 60, 120, 360, 1440]);
    expect(intervalOptions(45)).toEqual([15, 30, 45, 60, 120, 360, 1440]);
    expect(unsettledCount(base({ counts: { imported: 9, linked: 2, locked: 1, rejected: 2, failed: 3 } }))).toBe(6);
  });

  it("fragt während eines Laufs und kurz nach dem Anstoßen schnell nach", () => {
    expect(pollInterval(base({ running: true }), null, 0)).toBe(2_000);
    expect(pollInterval(base(), 1_000, 5_000)).toBe(1_000);
    expect(pollInterval(base(), 1_000, 20_000)).toBe(30_000);
    expect(pollInterval(undefined, null, 0)).toBe(30_000);
    expect(finishedRun(base({ running: true }), base())).toBe(true);
    expect(finishedRun(base(), base())).toBe(false);
  });
});

let status: GarminStatus;
let unsettled: GarminActivity[];
let writes: { method: string; url: string; body: unknown }[];

beforeEach(() => {
  status = base({ configured: false, key_set_at: null });
  unsettled = [];
  writes = [];
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string, init?: RequestInit) => {
      const method = init?.method ?? "GET";
      const body = init?.body ? JSON.parse(String(init.body)) : undefined;
      if (method !== "GET") writes.push({ method, url, body });
      if (url.endsWith("/garmin") && method === "PATCH") {
        if (body.api_key === "falscher-schluessel") {
          return json(400, { error: { code: "garmin_key_invalid", message: "rejected" } });
        }
        status = { ...status, ...body, configured: body.api_key === undefined ? status.configured : body.api_key !== null };
        if (body.api_key) status.key_set_at = "2026-11-14T19:00:00.000Z";
        return json(200, { data: status });
      }
      if (url.endsWith("/garmin/sync")) {
        status = { ...status, running: true, progress: { ...status.progress, phase: "list" } };
        return json(202, { data: status });
      }
      if (url.endsWith("/garmin/activities/retry")) return json(200, { data: { reset: unsettled.length } });
      if (url.includes("/garmin/activities")) return json(200, { data: unsettled, meta: { total: unsettled.length, limit: 200, offset: 0 } });
      if (url.endsWith("/garmin")) return json(200, { data: status });
      if (url.endsWith("/config")) return json(200, { data: { timezone: "Europe/Berlin", sub_sport_catalog: {} } });
      return json(404, { error: { code: "not_found", message: "nope" } });
    }),
  );
});

function setup() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter>
        <GarminSettings />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe("Einstellungen → Garmin Connect", () => {
  it("prüft den Schlüssel, zeigt Ablehnung und startet nichts von selbst", async () => {
    setup();
    expect(await screen.findByText("Nicht verbunden")).toBeInTheDocument();
    const field = screen.getByLabelText("Persönlicher API-Schlüssel");
    fireEvent.change(field, { target: { value: "falscher-schluessel" } });
    fireEvent.click(screen.getByRole("button", { name: "Prüfen und speichern" }));
    expect(await screen.findByText("Garmin lehnt den Schlüssel ab (abgelaufen oder falsch).")).toBeInTheDocument();

    fireEvent.change(screen.getByLabelText("Persönlicher API-Schlüssel"), { target: { value: "richtiger-schluessel" } });
    fireEvent.click(screen.getByRole("button", { name: "Prüfen und speichern" }));
    expect(await screen.findByText("Verbunden · Schlüssel seit 14.11.2026")).toBeInTheDocument();
    expect(writes.map((w) => w.url)).toEqual(["/api/v1/garmin", "/api/v1/garmin"]);
    expect(screen.getByRole("checkbox")).not.toBeChecked();
    expect(screen.queryByLabelText("Persönlicher API-Schlüssel")).not.toBeInTheDocument();
  });

  it("schaltet Automatik, Intervall, Anfangsdatum und stößt einen Abruf an", async () => {
    status = base();
    setup();
    fireEvent.click(await screen.findByRole("checkbox"));
    await waitFor(() => expect(writes).toHaveLength(1));
    fireEvent.change(screen.getByLabelText("Abrufintervall"), { target: { value: "60" } });
    await waitFor(() => expect(writes).toHaveLength(2));
    fireEvent.change(screen.getByLabelText(/Ab Datum/), { target: { value: "2020-01-01" } });
    await waitFor(() => expect(writes).toHaveLength(3));
    expect(writes.map((w) => w.body)).toEqual([{ enabled: true }, { interval_minutes: 60 }, { start_date: "2020-01-01" }]);

    fireEvent.click(screen.getByRole("button", { name: "Jetzt abrufen" }));
    expect(await screen.findByText(/Gesamtimport läuft/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Abruf läuft …" })).toBeDisabled();
    // Während die Liste gelesen wird, ist die Gesamtzahl unbekannt: Balken ohne Anteil.
    expect(screen.getByRole("progressbar")).not.toHaveAttribute("aria-valuenow");
  });

  it("zeigt den Fortschritt als Anteil der Gesamtzahl", async () => {
    status = base({
      running: true,
      progress: { phase: "download", total: 2428, processed: 1214, imported: 1100, linked: 114, locked: 0, rejected: 0, failed: 0 },
    });
    setup();
    expect(await screen.findByText(/1\.214 von 2\.428 bearbeitet, 1\.100 neu, 114 verknüpft/)).toBeInTheDocument();
    expect(screen.getByRole("progressbar")).toHaveAttribute("aria-valuenow", "50");
  });

  it("listet nicht übernommene Aktivitäten und setzt sie zurück", async () => {
    status = base({ counts: { imported: 10, linked: 1, locked: 1, rejected: 0, failed: 0 }, backfill_done: true });
    unsettled = [
      {
        garmin_id: "123",
        start_time: "2024-01-07T09:00:00.000Z",
        type_key: "running",
        status: "locked",
        activity_id: null,
        activity_title: null,
        reason_code: "locked_vehicle",
        reason: "Fahrzeug erkannt",
        attempts: 1,
        updated_at: "2026-11-14T18:00:00.000Z",
      },
    ];
    setup();
    const summary = await screen.findByText("Nicht übernommen (1)");
    fireEvent.click(summary);
    expect(await screen.findByText("Fahrzeug erkannt")).toBeInTheDocument();
    expect(screen.getByText("gesperrt")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Erneut versuchen" }));
    await waitFor(() => expect(writes).toEqual([{ method: "POST", url: "/api/v1/garmin/activities/retry", body: undefined }]));
  });

  it("entfernt den Schlüssel erst nach Rückfrage", async () => {
    status = base({ enabled: true });
    setup();
    fireEvent.click(await screen.findByRole("button", { name: "Entfernen" }));
    expect(writes).toHaveLength(0);
    expect(screen.getByText("Schlüssel entfernen und automatischen Abruf ausschalten?")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Entfernen" }));
    await waitFor(() => expect(writes).toEqual([{ method: "PATCH", url: "/api/v1/garmin", body: { api_key: null, enabled: false } }]));
    expect(await screen.findByText("Nicht verbunden")).toBeInTheDocument();
  });
});
