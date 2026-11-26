import { afterEach, describe, expect, it } from "vitest";
import type { ActivityListResponse } from "../../api/client";
import { resetLocaleForTests, setLocale } from "../../i18n";
import { csvDelimiter, fetchAll, toCsv } from "./csvExport";
import { readFilter } from "./listModel";
import { activity, summary } from "./testData";

const catalog = { cycling: [{ key: "mtb", builtin: true, name: null, icon: null }] } as never;

describe("CSV der Aktivitätenliste", () => {
  afterEach(() => resetLocaleForTests());

  it("deutsches Format: Semikolon, Dezimalkomma, Ortszeit, Anführungszeichen", () => {
    setLocale({ language: "de", format: "de-DE" });
    expect(csvDelimiter()).toBe(";");
    const csv = toCsv(
      [
        activity({ title: 'Runde "Wald"; kurz', location: "Kelheim" }),
        activity({ id: "a2", sub_sport: null, distance_m: null, avg_speed_kmh: null, avg_heart_rate_bpm: null, load: null, total_time_ms: 59_000 }),
      ],
      "Europe/Berlin",
      catalog,
    );
    const lines = csv.split("\r\n");
    expect(lines[0]).toBe("Start;Titel;Sportart;Untertyp;Ort;Distanz (km);Zeit (h:mm:ss);Schnitt (km/h);Aufstieg (m);Ø HF (bpm);Belastung");
    expect(lines[1]).toBe('2026-11-01 10:00;"Runde ""Wald""; kurz";Radfahren;MTB;Kelheim;25,00;1:00:00;25,0;312;128;72,9');
    expect(lines[2]).toBe("2026-11-01 10:00;Runde Wald;Radfahren;;;;0:59;;312;;");
    expect(lines[3]).toBe("");
  });

  it("englisches Format: Komma und Punkt", () => {
    setLocale({ language: "en", format: "en-GB" });
    expect(csvDelimiter()).toBe(",");
    const lines = toCsv([activity({ title: "Loop, short" })], "Europe/Berlin", catalog).split("\r\n");
    expect(lines[0]).toBe("Start,Title,Sport,Sub-type,Location,Distance (km),Time (h:mm:ss),Average (km/h),Elevation gain (m),Avg HR (bpm),Load");
    expect(lines[1]).toBe('2026-11-01 10:00,"Loop, short",Cycling,MTB,,25.00,1:00:00,25.0,312,128,72.9');
  });

  it("holt alle Seiten mit dem Filter der Liste", async () => {
    const calls: string[] = [];
    const page = (n: number, offset: number): ActivityListResponse => ({
      data: Array.from({ length: n }, (_, i) => activity({ id: `a${offset + i}` })),
      meta: { total: 250, limit: 200, offset, summary: summary() },
    });
    const all = await fetchAll(readFilter(new URLSearchParams("sport=cycling")), "2026-11-26", null, async (path) => {
      calls.push(path);
      return calls.length === 1 ? page(200, 0) : page(50, 200);
    });
    expect(all).toHaveLength(250);
    expect(calls).toHaveLength(2);
    expect(calls[1]).toContain("limit=200&offset=200");
  });
});
