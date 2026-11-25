import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";
import type { BackupEntry, BackupStatus, Operation } from "../../api/client";
import { json } from "../activities/testData";
import { BackupPage } from "./BackupPage";
import { backupState, operationPollInterval, statusPollInterval } from "./backupModel";

const entry = (p: Partial<BackupEntry> = {}): BackupEntry => ({
  id: "trackcraft-20261126-030000-s21-c12",
  kind: "regular",
  trigger: "daily",
  created_at: "2026-11-26T02:00:00.000Z",
  revision_id: "r12",
  format_version: 1,
  size_bytes: 285 * 1024 * 1024,
  check_status: "ok",
  problem: null,
  activity_count: 2916,
  segment_count: 36,
  original_count: 2900,
  missing_original_count: 0,
  damaged_original_count: 0,
  change_counter: 12,
  schema_version: 21,
  app_version: "1.0.0",
  ...p,
});

const base = (p: Partial<BackupStatus> = {}): BackupStatus => ({
  last_attempt: null,
  last_success: null,
  pending_changes: true,
  running_operation_id: null,
  target: {
    state: "available",
    free_bytes: 2 * 1024 ** 3,
    path: "\\\\ds214\\trackcraft",
    problem: null,
  },
  daily_time: "03:00",
  keep: 5,
  current: { revision_id: "r12", change_counter: 12 },
  ...p,
});

const operation = (p: Partial<Operation> = {}): Operation => ({
  id: "op1",
  kind: "backup",
  trigger: "manual",
  status: "running",
  phase: "copying",
  outcome: null,
  error: null,
  backup_id: null,
  started_at: "2026-11-26T10:00:00.000Z",
  finished_at: null,
  ...p,
});

const success = {
  time: "2026-11-26T02:00:00.000Z",
  revision_id: "r12",
  backup_id: entry().id,
  change_counter: 12,
};

describe("Sicherungs-Modell", () => {
  it("Zustand der Statuszeile", () => {
    expect(backupState(base())).toBe("never");
    expect(
      backupState(
        base({
          target: {
            state: "unavailable",
            free_bytes: null,
            path: "x",
            problem: "fehlt",
          },
        }),
      ),
    ).toBe("missing");
    expect(backupState(base({ last_success: success, pending_changes: false }))).toBe("ok");
    expect(backupState(base({ last_success: success }))).toBe("pending");
    const failed = {
      time: "2026-11-26T02:00:00Z",
      trigger: "daily" as const,
      outcome: "failed" as const,
      reason: "kaputt",
      operation_id: "o",
    };
    expect(backupState(base({ last_success: success, last_attempt: failed }))).toBe("failed");
    expect(backupState(base({ running_operation_id: "o", last_attempt: failed }))).toBe("running");
  });

  it("fragt nur während eines Vorgangs schnell nach", () => {
    expect(statusPollInterval(base({ running_operation_id: "o" }))).toBe(2_000);
    expect(statusPollInterval(base())).toBe(60_000);
    expect(operationPollInterval(operation())).toBe(1_000);
    expect(operationPollInterval(operation({ finished_at: "2026-11-26T10:01:00.000Z" }))).toBe(false);
  });
});

let status: BackupStatus;
let entries: BackupEntry[];
let op: Operation;
let writes: string[];

beforeEach(() => {
  status = base();
  entries = [];
  op = operation();
  writes = [];
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string, init?: RequestInit) => {
      const method = init?.method ?? "GET";
      if (method !== "GET") writes.push(url);
      if (url.endsWith("/backups") && method === "POST") {
        op = operation({ status: "queued", phase: null });
        return json(202, { data: op });
      }
      if (url.endsWith("/backups/status")) return json(200, { data: status });
      if (url.endsWith("/backups"))
        return json(200, {
          data: entries,
          meta: { target: { state: status.target.state } },
        });
      if (url.endsWith("/operations/op1")) return json(200, { data: op });
      if (url.endsWith("/config"))
        return json(200, {
          data: { timezone: "Europe/Berlin", sub_sport_catalog: {} },
        });
      return json(404, { error: { code: "not_found", message: "nope" } });
    }),
  );
});

function setup() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter>
        <BackupPage />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe("Seite Sicherung", () => {
  it("zeigt Status und Stände", async () => {
    status = base({
      last_success: success,
      last_attempt: {
        time: "2026-11-26T02:00:00.000Z",
        trigger: "daily",
        outcome: "created",
        reason: null,
        operation_id: "o",
      },
    });
    entries = [
      entry(),
      entry({
        id: "trackcraft-20261125-030000-s21-c10",
        created_at: "2026-11-25T02:00:00.000Z",
        check_status: "failed",
        problem: "fehlt",
      }),
    ];
    setup();
    expect(await screen.findByText("Ungesicherte Änderungen")).toBeInTheDocument();
    expect(screen.getByText("Letzte erfolgreiche Sicherung: 26.11.2026 03:00")).toBeInTheDocument();
    expect(screen.getByText(/täglich ab 03:00/)).toBeInTheDocument();
    expect(screen.getByText(/frei$/)).toBeInTheDocument();
    await waitFor(() => expect(screen.getAllByRole("row")).toHaveLength(3));
    const rows = screen.getAllByRole("row");
    expect(within(rows[1]!).getByText("zuletzt")).toBeInTheDocument();
    expect(within(rows[1]!).getByText("vollständig")).toBeInTheDocument();
    expect(within(rows[2]!).getByText("unvollständig")).toBeInTheDocument();
  });

  it("stößt eine Sicherung an, zeigt die Phase und lädt nach dem Ende neu", async () => {
    setup();
    fireEvent.click(await screen.findByRole("button", { name: "Jetzt sichern" }));
    expect(await screen.findByRole("button", { name: "Sichert …" })).toBeDisabled();
    expect(writes).toEqual(["/api/v1/backups"]);

    op = operation();
    expect(await screen.findByRole("button", { name: "Kopiert und prüft …" }, { timeout: 3000 })).toBeDisabled();

    op = operation({
      status: "done",
      phase: null,
      outcome: "created",
      backup_id: entry().id,
      finished_at: "2026-11-26T10:01:00.000Z",
    });
    status = base({
      last_success: success,
      pending_changes: false,
      last_attempt: {
        time: "2026-11-26T10:01:00.000Z",
        trigger: "manual",
        outcome: "created",
        reason: null,
        operation_id: "op1",
      },
    });
    entries = [entry()];
    expect(await screen.findByText("Alles gesichert", undefined, { timeout: 3000 })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Jetzt sichern" })).toBeEnabled();
    expect(await screen.findByText("vollständig")).toBeInTheDocument();
  });

  it("sperrt „Jetzt sichern“ ohne erreichbares Ziel", async () => {
    status = base({
      target: {
        state: "unavailable",
        free_bytes: null,
        path: "E:\\TrackCraft-Sicherung",
        problem: "fehlt",
      },
      last_attempt: {
        time: "2026-11-26T02:00:00.000Z",
        trigger: "daily",
        outcome: "skipped_target_unavailable",
        reason: "E:\\TrackCraft-Sicherung ist nicht vorhanden",
        operation_id: "o",
      },
    });
    setup();
    expect(await screen.findByText("Sicherungsziel nicht erreichbar")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Jetzt sichern" })).toBeDisabled();
    expect(await screen.findByText("Das Sicherungsziel ist nicht erreichbar.")).toBeInTheDocument();
    expect(screen.getByText(/Ziel nicht erreichbar, übersprungen/)).toBeInTheDocument();
  });
});
