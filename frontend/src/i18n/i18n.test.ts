import { afterEach, describe, expect, it } from "vitest";
import de from "./de.json";
import en from "./en.json";
import { COMPLETE_LANGUAGES, FORMAT_KEY, LANGUAGE_KEY, getLocale, resetLocaleForTests, resolveFormat, resolveLanguage, setLocale, t, tn } from ".";

const placeholders = (text: string) => [...text.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort();

afterEach(() => {
  window.localStorage.clear();
  resetLocaleForTests();
});

describe("Sprachdateien", () => {
  it.each([["en", en as Record<string, string>]])("%s hat nur Schlüssel aus de.json mit denselben Platzhaltern", (_, dict) => {
    const ref = de as Record<string, string>;
    for (const [key, text] of Object.entries(dict)) {
      expect(ref, `unbekannter Schlüssel ${key}`).toHaveProperty([key]);
      // Mehrzahlformen dürfen {count} weglassen („one more session“)
      const want = placeholders(ref[key]!).filter((p) => !(key.match(/_(one|other)$/) && p === "count"));
      const got = placeholders(text).filter((p) => !(key.match(/_(one|other)$/) && p === "count"));
      expect(got, `Platzhalter in ${key}`).toEqual(want);
    }
  });

  it("vollständige Sprachen haben jeden Schlüssel von de.json", () => {
    const dicts: Record<string, Record<string, string>> = { de, en };
    for (const lang of COMPLETE_LANGUAGES) {
      expect(Object.keys(de).filter((k) => !(k in dicts[lang]!)), `fehlt in ${lang}.json`).toEqual([]);
    }
  });

  it("jede Mehrzahl hat eine Form _other", () => {
    for (const key of Object.keys(de)) {
      const m = key.match(/^(.*)_(zero|one|two|few|many)$/);
      if (m) expect(de, `${m[1]}_other fehlt`).toHaveProperty([`${m[1]}_other`]);
    }
  });
});

describe("t und tn", () => {
  it("liest die gewählte Sprache", () => {
    expect(t("detail.edit")).toBe("Bearbeiten");
    setLocale({ language: "en" });
    expect(t("nav.activities")).toBe("Activities");
    expect(t("detail.edit")).toBe("Edit");
  });

  it("wählt die Mehrzahlform nach der Sprache", () => {
    expect(tn("plan.series.info", 1)).toBe("Teil einer Serie, noch eine offene Einheit danach");
    expect(tn("plan.series.info", 3)).toBe("Teil einer Serie, noch 3 offene Einheiten danach");
    setLocale({ language: "en" });
    expect(tn("plan.series.info", 2)).toBe("Part of a series, 2 more open sessions after this");
  });
});

describe("Sprach- und Formatwahl", () => {
  it("„Automatisch“ nimmt die erste vollständige Sprache des Browsers, sonst Deutsch", () => {
    expect(resolveLanguage("auto", ["fr-FR", "de-AT"])).toBe("de");
    expect(resolveLanguage("auto", ["fr-FR", "en-US", "de-DE"])).toBe("en");
    expect(resolveLanguage("auto", ["fr-FR", "it"])).toBe("de");
    expect(resolveLanguage("en", ["de-DE"])).toBe("en");
  });

  it("das Format folgt der Sprache oder der eigenen Wahl", () => {
    expect(resolveFormat("auto", "de", ["en-US"])).toBe("de-DE");
    expect(resolveFormat("auto", "en", ["en-US"])).toBe("en-US");
    expect(resolveFormat("auto", "en", ["de-DE"])).toBe("en-GB");
    expect(resolveFormat("en-US", "de")).toBe("en-US");
  });

  it("speichert die Wahl im Browser und setzt <html lang>", () => {
    setLocale({ language: "en", format: "de-DE" });
    expect(window.localStorage.getItem(LANGUAGE_KEY)).toBe("en");
    expect(window.localStorage.getItem(FORMAT_KEY)).toBe("de-DE");
    expect(document.documentElement.lang).toBe("en");
    resetLocaleForTests();
    expect(getLocale()).toMatchObject({ language: "en", format: "de-DE" });
    setLocale({ language: "auto", format: "auto" });
    expect(window.localStorage.getItem(LANGUAGE_KEY)).toBeNull();
    expect(getLocale()).toMatchObject({ language: "de", format: "de-DE" });
  });
});
