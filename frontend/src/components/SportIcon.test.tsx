import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { SubSportCatalog } from "../api/client";
import { ActivitySportIcon } from "./SportIcon";

const catalog: SubSportCatalog = {
  cycling: [
    { key: "road", name: null, icon: null, builtin: true },
    { key: "custom_1", name: "Gravel", icon: "mtb", builtin: false },
  ],
};

describe("ActivitySportIcon", () => {
  it("zeigt ohne Untertyp das Klassensymbol", () => {
    render(<ActivitySportIcon sport="cycling" subKey={null} catalog={catalog} />);
    expect(screen.getByRole("img", { name: "Radfahren" })).toHaveAttribute("data-glyph", "bike");
  });

  it("zeigt mit Untertyp dessen Symbol und beide Namen als Tooltip", () => {
    render(<ActivitySportIcon sport="cycling" subKey="road" catalog={catalog} />);
    expect(screen.getByRole("img", { name: "Radfahren · Rennrad" })).toHaveAttribute("data-glyph", "road_bike");
  });

  it("nimmt bei eigenen Untertypen das gewählte Symbol", () => {
    render(<ActivitySportIcon sport="cycling" subKey="custom_1" catalog={catalog} />);
    expect(screen.getByRole("img", { name: "Radfahren · Gravel" })).toHaveAttribute("data-glyph", "mtb");
  });
});
