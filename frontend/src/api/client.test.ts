import { apiGet, ApiError, OfflineError } from "./client";
import { errorText } from "../i18n";

function fakeFetch(status: number, body: unknown): typeof fetch {
  return (async () => new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } })) as typeof fetch;
}

describe("apiGet", () => {
  it("liefert den Body bei Erfolg und fragt /api/v1 ohne Cache", async () => {
    let seen: { url: string; init?: RequestInit } | undefined;
    const fetcher = (async (url: string, init?: RequestInit) => {
      seen = { url, init };
      return new Response('{"data":{"ready":true}}', { status: 200 });
    }) as unknown as typeof fetch;
    await expect(apiGet("/ready", fetcher)).resolves.toEqual({ data: { ready: true } });
    expect(seen?.url).toBe("/api/v1/ready");
    expect(seen?.init?.cache).toBe("no-store");
  });

  it("wandelt Fehlerantworten in ApiError mit Code", async () => {
    const err = await apiGet<never>("/version", fakeFetch(403, { error: { code: "host_not_allowed", message: "nope" } })).catch(
      (e: unknown) => e as ApiError,
    );
    expect(err).toBeInstanceOf(ApiError);
    expect(err.status).toBe(403);
    expect(err.code).toBe("host_not_allowed");
  });

  it("nutzt internal_error bei Antworten ohne Fehlerobjekt", async () => {
    const fetcher = (async () => new Response("kaputt", { status: 502, statusText: "Bad Gateway" })) as unknown as typeof fetch;
    const err = await apiGet<never>("/version", fetcher).catch((e: unknown) => e as ApiError);
    expect(err.code).toBe("internal_error");
  });

  it("meldet fehlende Verbindung als OfflineError", async () => {
    const fetcher = (async () => {
      throw new TypeError("Failed to fetch");
    }) as unknown as typeof fetch;
    await expect(apiGet("/version", fetcher)).rejects.toBeInstanceOf(OfflineError);
  });
});

describe("Fehlertexte (RFC-0006)", () => {
  it("übersetzt bekannte Codes", () => {
    expect(errorText("not_ready", "starting")).toBe("TrackCraft startet noch.");
  });
  it("zeigt bei unbekanntem Code Meldung und Code", () => {
    expect(errorText("brand_neu", "Something")).toBe("Something (brand_neu)");
  });
});
