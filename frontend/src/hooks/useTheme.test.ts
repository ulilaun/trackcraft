import { act, renderHook } from "@testing-library/react";
import { resetThemeForTests, useApplyTheme, useTheme } from "./useTheme";
import { THEME_KEY } from "../utils/theme";

function mockMatchMedia(prefersLight: boolean) {
  window.matchMedia = ((query: string) => ({
    matches: query.includes("light") ? prefersLight : false,
    media: query,
    addEventListener: () => {},
    removeEventListener: () => {},
  })) as unknown as typeof window.matchMedia;
}

function useBoth() {
  useApplyTheme();
  return useTheme();
}

describe("useTheme", () => {
  beforeEach(() => {
    window.localStorage.clear();
    resetThemeForTests();
    document.documentElement.classList.remove("dark");
  });

  it("setzt Dark ohne Wahl bei dunklem System und übernimmt eine neue Wahl", () => {
    mockMatchMedia(false);
    const { result } = renderHook(() => useBoth());
    expect(result.current.choice).toBe("system");
    expect(document.documentElement.classList.contains("dark")).toBe(true);

    act(() => result.current.setChoice("light"));
    expect(result.current.choice).toBe("light");
    expect(window.localStorage.getItem(THEME_KEY)).toBe("light");
    expect(document.documentElement.classList.contains("dark")).toBe(false);

    act(() => result.current.setChoice("system"));
    expect(window.localStorage.getItem(THEME_KEY)).toBeNull();
    expect(document.documentElement.classList.contains("dark")).toBe(true);
  });

  it("liest die gespeicherte Wahl", () => {
    mockMatchMedia(false);
    window.localStorage.setItem(THEME_KEY, "light");
    const { result } = renderHook(() => useBoth());
    expect(result.current.choice).toBe("light");
    expect(document.documentElement.classList.contains("dark")).toBe(false);
  });

  it("alle Nutzer sehen dieselbe Wahl", () => {
    mockMatchMedia(false);
    const a = renderHook(() => useTheme());
    const b = renderHook(() => useTheme());
    act(() => a.result.current.setChoice("dark"));
    expect(b.result.current.choice).toBe("dark");
  });
});
