import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";
import type { Profile } from "../../api/client";
import { FlashProvider } from "../../components/Flash";
import { json } from "../activities/testData";
import { AccountPage } from "./AccountPage";
import { age, avatarText, formOf, formProblem, inputOf } from "./profileModel";

const base: Profile = {
  id: "u1",
  nickname: "Uli",
  first_name: null,
  last_name: null,
  birthday: null,
  avatar_color: null,
  updated_at: "2026-11-16T10:00:00.000Z",
};

describe("Profil-Modell", () => {
  it("Symboltext: bis 3 Zeichen ganz, sonst erstes Zeichen groß", () => {
    expect(avatarText("Uli")).toBe("Uli");
    expect(avatarText(" Jo ")).toBe("Jo");
    expect(avatarText("ulrich")).toBe("U");
    expect(avatarText("Ölf")).toBe("Ölf");
    expect(avatarText("ärger")).toBe("Ä");
    expect(avatarText("")).toBe("?");
  });

  it("Alter in vollen Jahren", () => {
    expect(age("1965-11-17", "2026-11-16")).toBe(60);
    expect(age("1965-11-16", "2026-11-16")).toBe(61);
    expect(age(null, "2026-11-16")).toBeNull();
  });

  it("schickt nur Änderungen, leere Felder als null", () => {
    const p = { ...base, first_name: "Ulrich", birthday: "1965-04-03" };
    const f = formOf(p);
    expect(inputOf(f, p)).toBeNull();
    expect(inputOf({ ...f, first_name: " ", nickname: " Uli2 ", avatar_color: "teal" }, p)).toEqual({
      nickname: "Uli2",
      first_name: null,
      avatar_color: "teal",
    });
    expect(formProblem({ ...f, nickname: "  " }, "2026-11-16")).toBe("nicknameRequired");
    expect(formProblem({ ...f, birthday: "2026-11-17" }, "2026-11-16")).toBe("birthdayFuture");
  });
});

let writes: { url: string; body: unknown }[];

beforeEach(() => {
  writes = [];
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string, init?: RequestInit) => {
      if ((init?.method ?? "GET") === "PATCH") {
        const body = JSON.parse(String(init!.body)) as Partial<Profile>;
        writes.push({ url, body });
        return json(200, { data: { ...base, ...body, updated_at: "2026-11-16T11:00:00.000Z" } });
      }
      if (url.endsWith("/profile")) return json(200, { data: base });
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
        <FlashProvider>
          <AccountPage />
        </FlashProvider>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe("Kontoseite", () => {
  it("bearbeitet Spitzname, Namen, Geburtstag und Farbe", async () => {
    setup();
    const nick = await screen.findByLabelText(/Spitzname/);
    const save = screen.getByRole("button", { name: "Speichern" });
    expect(save).toBeDisabled();

    fireEvent.change(nick, { target: { value: "  " } });
    expect(screen.getByRole("alert")).toHaveTextContent("Der Spitzname darf nicht leer sein.");
    expect(save).toBeDisabled();

    fireEvent.change(nick, { target: { value: "Ulrich" } });
    fireEvent.change(screen.getByLabelText("Vorname"), { target: { value: "Ulrich" } });
    fireEvent.change(screen.getByLabelText("Geburtstag"), { target: { value: "1965-04-03" } });
    fireEvent.click(within(screen.getByRole("radiogroup", { name: "Farbe des Symbols" })).getByRole("radio", { name: "Petrol" }));
    expect(screen.getByTestId("account-nickname")).toHaveTextContent("Ulrich");
    expect(screen.getByText(/Ulrich · \d+ Jahre/)).toBeInTheDocument();

    fireEvent.click(save);
    await waitFor(() =>
      expect(writes).toEqual([
        { url: "/api/v1/profile", body: { nickname: "Ulrich", first_name: "Ulrich", birthday: "1965-04-03", avatar_color: "teal" } },
      ]),
    );
    expect(await screen.findByText("Profil gespeichert.")).toBeInTheDocument();
  });
});
