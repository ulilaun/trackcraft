import { render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { json } from "../pages/activities/testData";
import { DemoBanner } from "./DemoBanner";

function setup(demo: boolean | undefined) {
  vi.stubGlobal(
    "fetch",
    vi.fn(async () => json(200, { data: { timezone: "Europe/Berlin", sub_sport_catalog: {}, demo } })),
  );
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <DemoBanner />
      <span>geladen</span>
    </QueryClientProvider>,
  );
}

describe("DemoBanner", () => {
  it("zeigt den Hinweis nur im Demo-Modus", async () => {
    setup(true);
    expect(await screen.findByRole("note")).toHaveTextContent("Demo mit erfundenen Daten");
  });

  it("ohne Demo kein Hinweis", async () => {
    const { container } = setup(undefined);
    await screen.findByText("geladen");
    await new Promise((r) => setTimeout(r, 20));
    expect(container.querySelector("[role=note]")).toBeNull();
  });
});
