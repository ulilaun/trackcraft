import { readChoice, resolveDark, storeChoice, THEME_KEY } from "./theme";

function memory(initial: Record<string, string> = {}) {
  const data = new Map(Object.entries(initial));
  return {
    getItem: (k: string) => data.get(k) ?? null,
    setItem: (k: string, v: string) => void data.set(k, v),
    removeItem: (k: string) => void data.delete(k),
    data,
  };
}

describe("Theme (RFC-0006)", () => {
  it("folgt dem System, ohne Präferenz Dark", () => {
    expect(resolveDark("system", false)).toBe(true);
    expect(resolveDark("system", true)).toBe(false);
  });

  it("Wahl überschreibt das System", () => {
    expect(resolveDark("dark", true)).toBe(true);
    expect(resolveDark("light", false)).toBe(false);
  });

  it("speichert je Browser, System entfernt den Eintrag", () => {
    const s = memory();
    storeChoice(s, "dark");
    expect(s.data.get(THEME_KEY)).toBe("dark");
    expect(readChoice(s)).toBe("dark");
    storeChoice(s, "system");
    expect(s.data.has(THEME_KEY)).toBe(false);
    expect(readChoice(s)).toBe("system");
  });

  it("ignoriert unbekannte Werte und fehlenden Speicher", () => {
    expect(readChoice(memory({ [THEME_KEY]: "lila" }))).toBe("system");
    expect(readChoice(undefined)).toBe("system");
    const broken = {
      getItem: () => {
        throw new Error("gesperrt");
      },
    };
    expect(readChoice(broken)).toBe("system");
    expect(() =>
      storeChoice(
        {
          setItem: () => {
            throw new Error("voll");
          },
          removeItem: () => {},
        },
        "dark",
      ),
    ).not.toThrow();
  });
});
