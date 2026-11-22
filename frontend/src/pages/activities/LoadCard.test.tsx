import { render, screen } from "@testing-library/react";
import type { ActivityLoad } from "../../api/client";
import { LoadCard } from "./LoadCard";

const load: ActivityLoad = {
  trimp: 187.4,
  aerobic: 161.2,
  anaerobic: 26.2,
  method: "samples",
  zone_seconds: [840, 4260, 3120, 2340, 960],
  zone_set: {
    id: "c1",
    sport_class: "cycling",
    valid_from: "2026-03-01",
    max_hr: 174,
    resting_hr: 52,
    threshold_hr: 158,
    model: "threshold",
    bounds: [68, 85, 90, 95, 100],
    zones_bpm: [
      [107, 133],
      [134, 141],
      [142, 149],
      [150, 157],
      [158, null],
    ],
    anaerobic_from_bpm: 158,
  },
};

describe("Block Belastung", () => {
  it("zeigt TRIMP, aerob/anaerob und Zeit in Zonen", () => {
    render(<LoadCard load={load} />);
    expect(screen.getByTestId("load-trimp")).toHaveTextContent("187");
    expect(screen.getByText("unter 158 Schlägen/min")).toBeInTheDocument();
    expect(screen.getByText("Zonensatz „Radfahren“ ab 01.03.2026")).toBeInTheDocument();
    const zones = screen.getByTestId("load-zones");
    expect(zones).toHaveTextContent("Z2 Grundlage134+");
    expect(zones).toHaveTextContent("1:11:00");
    expect(zones).toHaveTextContent("37 %");
  });

  it("geschätzt ohne Zonen, noch nicht berechnet", () => {
    const { rerender } = render(<LoadCard load={{ ...load, method: "estimated", zone_seconds: null, zone_set: null, trimp: 25 }} />);
    expect(screen.getByText(/aus der Dauer geschätzt/)).toBeInTheDocument();
    expect(screen.queryByText("aerob")).not.toBeInTheDocument();
    rerender(<LoadCard load={null} />);
    expect(screen.getByText("Die Belastung wird gerade berechnet.")).toBeInTheDocument();
  });
});
