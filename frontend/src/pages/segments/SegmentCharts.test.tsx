import { fireEvent, render, screen } from "@testing-library/react";
import { ProgressionChart, barLayout, outlierCap, rankColor } from "./SegmentCharts";
import { progression } from "./segmentModel";
import { effort } from "./testData";

/** Vier gültige Versuche: erste Bestzeit (heute Platz 3), langsamer (Platz 4), Verbesserung (Platz 2), heutige Bestzeit. */
function four() {
  return [
    effort({ id: "e1", activity_id: "a1", start_time: "2026-08-01T09:00:00.000Z", duration_ms: 300_000, rank: 3, record_kind: "first_best" }),
    effort({ id: "e2", activity_id: "a2", start_time: "2026-08-10T09:00:00.000Z", duration_ms: 320_000, rank: 4, record_kind: null }),
    effort({ id: "e3", activity_id: "a3", start_time: "2026-09-01T09:00:00.000Z", duration_ms: 290_000, rank: 2, record_kind: "improvement" }),
    effort({ id: "e4", activity_id: "a4", activity_title: "Schnelle Runde", start_time: "2026-10-01T09:00:00.000Z", duration_ms: 280_000, rank: 1, record_kind: "improvement" }),
  ];
}

describe("Bestzeiten-Entwicklung", () => {
  it("Gold für die Bestzeit, Akzent für die Jahresbestzeit, sonst gedämpft; keine Markierung damaliger Bestzeiten", () => {
    render(<ProgressionChart data={progression(four())} timeZone="Europe/Berlin" yearBestId="e3" year={2026} />);
    const bars = screen.getAllByTestId("progress-bar");
    // Kein Silber und Bronze mehr (Uli 12.11.2026)
    expect(bars.map((b) => b.getAttribute("fill"))).toEqual([rankColor(3), rankColor(4), rankColor(2, true), rankColor(1)]);
    expect(rankColor(2)).toBe(rankColor(3));
    expect(new Set(bars.map((b) => b.getAttribute("fill"))).size).toBe(3);
    // damalige Bestzeit e1 trägt keinen Punkt mehr
    expect(screen.queryByTestId("progress-record")).not.toBeInTheDocument();
    expect(screen.getAllByTestId("progress-pr")).toHaveLength(1);
    expect(screen.getByText("Bestzeit 2026")).toBeInTheDocument();
    expect(bars.some((b) => b.getAttribute("data-own"))).toBe(false);
    // schneller = kürzerer Balken
    const heights = bars.map((b) => Number(b.getAttribute("height")));
    expect(heights[3]).toBeLessThan(heights[2]!);
    expect(heights[1]).toBeGreaterThan(heights[0]!);
    expect(screen.queryByText("damalige Bestzeit")).not.toBeInTheDocument();
  });

  it("oranger Rahmen für die Herkunftsaktivität, Hover zeigt Datum, Aktivität, Zeit und Rang", () => {
    render(<ProgressionChart data={progression(four(), "a4")} timeZone="Europe/Berlin" />);
    const own = screen.getAllByTestId("progress-bar").filter((b) => b.getAttribute("data-own"));
    expect(own).toHaveLength(1);
    expect(own[0]!.getAttribute("stroke")).toBe("#f59e0b");
    expect(screen.getByText("Diese Aktivität")).toBeInTheDocument();
    fireEvent.mouseEnter(screen.getAllByTestId("progress-bar")[3]!.parentElement!);
    expect(screen.getByText("01.10.2026 · Schnelle Runde")).toBeInTheDocument();
    expect(screen.getByText("4:40,0")).toBeInTheDocument();
    expect(screen.getByText(/Rang 1 von 4/)).toBeInTheDocument();
  });

  it("Balken stehen direkt nebeneinander, werden bei vielen schmaler und bei wenigen nicht beliebig breit", () => {
    render(<ProgressionChart data={progression(four())} timeZone="Europe/Berlin" />);
    const bars = screen.getAllByTestId("progress-bar");
    const xs = bars.map((b) => Number(b.getAttribute("x")));
    expect(xs[1]! - xs[0]!).toBeCloseTo(34);
    expect(xs[3]! - xs[2]!).toBeCloseTo(34);
    expect(bars[0]!.getAttribute("width")).toBe("30");
    expect(barLayout(1, 700)).toEqual({ slot: 34, bar: 30, labelEvery: 2 });
    const many = barLayout(350, 700);
    expect(many.slot).toBe(2);
    expect(many.bar).toBeLessThan(2);
    expect(many.labelEvery).toBe(27);
    // unter jedem zweiten Balken steht das Datum
    expect(screen.getByText("01.08.26")).toBeInTheDocument();
    expect(screen.queryByText("10.08.26")).not.toBeInTheDocument();
    expect(screen.getByText("01.09.26")).toBeInTheDocument();
  });

  it("Hover von außen hebt den Balken hervor und meldet eigenen Hover nach außen", () => {
    const onHover = vi.fn();
    render(<ProgressionChart data={progression(four())} timeZone="Europe/Berlin" hoverId="e2" onHover={onHover} />);
    const bars = screen.getAllByTestId("progress-bar");
    expect(bars.map((b) => b.getAttribute("data-hover"))).toEqual([null, "true", null, null]);
    expect(bars[0]!.getAttribute("opacity")).toBe("0.55");
    expect(screen.getByText(/Rang 4 von 4/)).toBeInTheDocument();
    fireEvent.mouseEnter(bars[3]!.parentElement!);
    expect(onHover).toHaveBeenLastCalledWith("e4");
    fireEvent.mouseLeave(bars[3]!.parentElement!);
    expect(onHover).toHaveBeenLastCalledWith(null);
  });

  it("Ausreißer: nur der brutal langsame Versuch wird gekappt, nicht normale Streuung", () => {
    expect(outlierCap([60, 62, 58, 70, 1048, 65])).toBe(70);
    // langsam, aber unter dem Doppelten des Medians: kein Ausreißer
    expect(outlierCap([60, 62, 58, 70, 110, 65])).toBeNull();
    // zu wenige Versuche
    expect(outlierCap([60, 1000])).toBeNull();
    // mehr als ein Viertel so langsam: keine Ausreißer, sondern so ist das Segment
    expect(outlierCap([60, 61, 300, 310])).toBeNull();
    expect(outlierCap([60, 61, 62, 63, 64, 65, 66, 600])).toBe(66);
  });

  it("gekappter Balken ragt nur knapp über die Achse, zeigt Bruchzeichen und die echte Zeit", () => {
    const list = [
      ...four(),
      effort({ id: "e5", activity_id: "a5", start_time: "2026-10-05T09:00:00.000Z", duration_ms: 1_048_000, rank: 5, record_kind: null }),
    ];
    render(<ProgressionChart data={progression(list)} timeZone="Europe/Berlin" />);
    const bars = screen.getAllByTestId("progress-bar");
    expect(bars.map((b) => b.getAttribute("data-capped"))).toEqual([null, null, null, null, "true"]);
    expect(screen.getAllByTestId("progress-break")).toHaveLength(1);
    expect(screen.getByTestId("progress-capped-value")).toHaveTextContent("17:28");
    const heights = bars.map((b) => Number(b.getAttribute("height")));
    // Die normalen Versuche bleiben unterscheidbar: der langsamste normale ist deutlich höher als die Bestzeit.
    expect(heights[1]! / heights[3]!).toBeGreaterThan(1.5);
    // Der gekappte ist nur wenig höher als der langsamste normale.
    expect(heights[4]! - heights[1]!).toBeLessThanOrEqual(20);
  });

  it("ohne gewertete Versuche ein Hinweis", () => {
    render(<ProgressionChart data={[]} />);
    expect(screen.getByText("Noch keine gewerteten Versuche.")).toBeInTheDocument();
  });
});
