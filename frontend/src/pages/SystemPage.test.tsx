import { render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { SystemInfo } from "../api/client";
import { json } from "./activities/testData";
import { SystemPage, uptime } from "./SystemPage";

const info = (p: Partial<SystemInfo> = {}): SystemInfo => ({
  uptime_s: 3 * 86_400 + 4 * 3600 + 120,
  started_at: "2026-11-23T08:00:00.000Z",
  platform: "linux_service",
  tzdata_version: "2025b",
  tls_certificate_expires_at: "2027-11-20T00:00:00.000Z",
  counts: { activities: 2916, segments: 36, efforts: 12_345 },
  storage: { data_free_bytes: 20 * 1024 ** 3, backup_target_free_bytes: null },
  calculation: { status: "current", version_id: "v1", published_at: "2026-11-20T10:00:00.000Z" },
  jobs: { queued: 1, running: 0, failed: 2 },
  warnings: [],
  ...p,
});

let system: SystemInfo;

beforeEach(() => {
  system = info();
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string) => {
      if (url.endsWith("/system")) return json(200, { data: system });
      if (url.endsWith("/version"))
        return json(200, { data: { app_version: "1.0.0", api_version: "v1", schema_version: 23, algorithm_version: 3 } });
      if (url.endsWith("/health"))
        return json(200, { data: { status: "ok", checks: [{ name: "backend", status: "ok", message: null }] } });
      if (url.endsWith("/config")) return json(200, { data: { timezone: "Europe/Berlin" } });
      return json(404, { error: { code: "not_found", message: url } });
    }),
  );
});

afterEach(() => vi.unstubAllGlobals());

function renderPage() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <SystemPage />
    </QueryClientProvider>,
  );
}

describe("SystemPage", () => {
  it("formatiert die Laufzeit", () => {
    expect(uptime(3 * 86_400 + 4 * 3600 + 120)).toBe("3 T 4 Std");
    expect(uptime(2 * 3600 + 5 * 60 + 59)).toBe("2 Std 5 Min");
  });

  it("zeigt Betrieb, Bestand und keine Warnungen", async () => {
    renderPage();
    expect(await screen.findByText("systemd-Dienst (Linux)")).toBeInTheDocument();
    expect(screen.getByText("2025b")).toBeInTheDocument();
    expect(screen.getByText("2.916")).toBeInTheDocument();
    expect(screen.getByText("12.345")).toBeInTheDocument();
    expect(screen.getByText("nicht erreichbar")).toBeInTheDocument();
    expect(screen.getByText("1 wartend, 0 laufend, 2 fehlgeschlagen")).toBeInTheDocument();
    expect(screen.getByText(/gültig bis 20\.11\.2027/)).toBeInTheDocument();
    expect(screen.queryByText("Warnungen")).not.toBeInTheDocument();
  });

  it("zeigt Warnungen mit Bezeichnung, Meldung und Beginn", async () => {
    system = info({
      tls_certificate_expires_at: null,
      warnings: [
        { code: "certificate_expiring", message: "TLS certificate expires in 10 days", since: "2026-11-16T00:00:00.000Z" },
        { code: "something_new", message: "Neue Warnung", since: "2026-11-26T09:00:00.000Z" },
      ],
    });
    renderPage();
    expect(await screen.findByText("Warnungen")).toBeInTheDocument();
    expect(screen.getByText("Zertifikat läuft ab")).toBeInTheDocument();
    expect(screen.getByText(/TLS certificate expires in 10 days · seit 16\.11\.2026/)).toBeInTheDocument();
    // Unbekannter Code: Meldung des Backends als Titel
    expect(screen.getByText("Neue Warnung")).toBeInTheDocument();
    expect(screen.getByText("kein eigenes TLS")).toBeInTheDocument();
  });
});
