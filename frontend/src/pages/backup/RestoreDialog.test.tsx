import { fireEvent, render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { BackupEntry, Operation, RestorePreview } from "../../api/client";
import { json } from "../activities/testData";
import { RestoreDialog } from "./RestoreDialog";

const entry: BackupEntry = {
  id: "trackcraft-20261125-030000-s21-c10",
  kind: "regular",
  trigger: "daily",
  created_at: "2026-11-25T02:00:00.000Z",
  revision_id: "r10",
  format_version: 1,
  size_bytes: 1024,
  check_status: "ok",
  problem: null,
  activity_count: 2900,
  segment_count: 35,
  original_count: 2900,
  missing_original_count: 0,
  damaged_original_count: 0,
  change_counter: 10,
  schema_version: 21,
  app_version: "1.0.0",
};

const preview = (p: Partial<RestorePreview> = {}): RestorePreview => ({
  preview_token: `${entry.id}.abc`,
  backup: entry,
  current: { revision_id: "r12", activity_count: 2916, segment_count: 36, is_empty: false },
  comparison: "older",
  requires_acknowledgement: true,
  warnings: [{ code: "restore_older", message: "älter" }],
  ...p,
});

const op = (p: Partial<Operation> = {}): Operation => ({
  id: "op1",
  kind: "restore",
  trigger: "manual",
  status: "running",
  phase: "staging",
  outcome: null,
  error: null,
  backup_id: entry.id,
  started_at: "2026-11-26T10:00:00.000Z",
  finished_at: null,
  ...p,
});

let previewBody: unknown;
let previewStatus: number;
let operations: (Operation | "offline")[];
let posts: { url: string; body: unknown }[];

beforeEach(() => {
  previewBody = { data: preview() };
  previewStatus = 200;
  operations = [];
  posts = [];
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string, init?: RequestInit) => {
      const body = init?.body ? JSON.parse(String(init.body)) : undefined;
      if (init?.method === "POST") posts.push({ url, body });
      if (url.endsWith("/restores/preview")) return json(previewStatus, previewBody);
      if (url.endsWith("/restores")) return json(202, { data: op({ phase: "checking" }) });
      if (url.endsWith("/operations/op1")) {
        const next = operations.length > 1 ? operations.shift()! : operations[0]!;
        if (next === "offline") throw new TypeError("Failed to fetch");
        return json(200, { data: next });
      }
      return json(404, { error: { code: "not_found", message: "nope" } });
    }),
  );
});

function setup() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <RestoreDialog entry={entry} tz="Europe/Berlin" onClose={() => {}} />
    </QueryClientProvider>,
  );
}

describe("Wiederherstellen-Dialog", () => {
  it("verlangt bei älterem Stand die Zustimmung und zeigt den Ablauf bis zum Ende", async () => {
    operations = [op(), "offline", op({ status: "done", phase: null, outcome: "restored", finished_at: "2026-11-26T10:01:00.000Z" })];
    setup();
    expect(await screen.findByText("Dieser Stand ist älter als die aktuellen Daten.")).toBeInTheDocument();
    expect(screen.getByText("Änderungen seit diesem Stand gehen verloren.")).toBeInTheDocument();
    expect(screen.getByText("2.916")).toBeInTheDocument();
    const confirm = screen.getByRole("button", { name: "Wiederherstellen" });
    expect(confirm).toBeDisabled();
    fireEvent.click(screen.getByRole("checkbox"));
    expect(confirm).toBeEnabled();
    fireEvent.click(confirm);

    expect(await screen.findByText("Stand wird bereitgestellt …", undefined, { timeout: 3000 })).toBeInTheDocument();
    expect(posts.at(-1)).toEqual({ url: "/api/v1/restores", body: { preview_token: `${entry.id}.abc`, acknowledge_warnings: true } });
    expect(await screen.findByText("TrackCraft startet neu …", undefined, { timeout: 3000 })).toBeInTheDocument();
    expect(await screen.findByText("Wiederhergestellt. Bitte die Seite neu laden.", undefined, { timeout: 3000 })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Neu laden" })).toBeInTheDocument();
  });

  it("braucht bei neuerem Stand keine Zustimmung und meldet eine Rücknahme", async () => {
    previewBody = { data: preview({ comparison: "newer", requires_acknowledgement: false, warnings: [] }) };
    operations = [
      op({
        status: "rolled_back",
        phase: null,
        outcome: "failed",
        error: { code: "restore_failed", message: "Funktionsprüfung gescheitert" },
        finished_at: "2026-11-26T10:01:00.000Z",
      }),
    ];
    setup();
    expect(await screen.findByText("Dieser Stand ist neuer als die aktuellen Daten.")).toBeInTheDocument();
    expect(screen.queryByRole("checkbox")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Wiederherstellen" }));
    expect(await screen.findByText(/zurückgenommen/, undefined, { timeout: 3000 })).toBeInTheDocument();
    expect(screen.getByText("Funktionsprüfung gescheitert")).toBeInTheDocument();
  });

  it("zeigt einen beschädigten Stand mit Grund", async () => {
    previewStatus = 400;
    previewBody = { error: { code: "backup_invalid", message: "damaged", details: { reason: "originals/a.fit weicht vom Manifest ab" } } };
    setup();
    expect(await screen.findByText("Der Stand ist beschädigt oder unvollständig.")).toBeInTheDocument();
    expect(screen.getByText("originals/a.fit weicht vom Manifest ab")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Wiederherstellen" })).toBeDisabled();
  });
});
