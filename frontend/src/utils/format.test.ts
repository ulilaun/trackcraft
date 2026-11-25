import { afterEach } from "vitest";
import { resetLocaleForTests, setLocale } from "../i18n";
import {
  addDays,
  dateOnly,
  dateTime,
  dayDate,
  dayMonth,
  dayRange,
  duration,
  hoursMinutes,
  km,
  localDate,
  fileSize,
  megabytes,
  monthLong,
  monthShort,
  monthYear,
  num,
  pace,
  parseDuration,
  segmentTime,
  speed,
  weekdayShort,
} from "./format";

describe("Formate (RFC-0006)", () => {
  it("Zahlen mit Komma und Tausenderpunkt", () => {
    expect(num(1234.5, 1)).toBe("1.234,5");
    expect(km(136210)).toBe("136,21");
    expect(km(null)).toBe("–");
    expect(megabytes(1.5 * 1024 * 1024)).toBe("1,5");
    expect(fileSize(285 * 1024 * 1024)).toBe("285,0 MB");
    expect(fileSize(2.5 * 1024 ** 3)).toBe("2,5 GB");
  });

  it("Dauer h:mm:ss bzw. m:ss", () => {
    expect(duration(24_122_000)).toBe("6:42:02");
    expect(duration(590_000)).toBe("9:50");
    expect(duration(null)).toBe("–");
  });

  it("liest Dauern", () => {
    expect(parseDuration("1:00:00")).toBe(3_600_000);
    expect(parseDuration("45:30")).toBe(2_730_000);
    expect(parseDuration("0:90")).toBeNull();
    expect(parseDuration("1:60:00")).toBeNull();
    expect(parseDuration("abc")).toBeNull();
    expect(parseDuration("5")).toBeNull();
  });

  it("Zeitpunkte in der Zeitzone der Konfiguration", () => {
    expect(dateTime("2026-10-22T08:15:03.000Z", "Europe/Berlin")).toBe("22.10.26 10:15");
    expect(dateTime("2026-01-05T23:30:00.000Z", "Europe/Berlin", true)).toBe("06.01.2026 00:30");
    expect(dateTime(null)).toBe("–");
  });
});

describe("Schnitt, Pace und Tage", () => {
  it("Pace aus km/h, halbe Sekunden aufgerundet", () => {
    expect(pace(12)).toBe("5:00");
    expect(pace(3600 / 299.5)).toBe("5:00");
    expect(pace(0)).toBe("–");
    expect(pace(null)).toBe("–");
  });

  it("Schnitt je Sportklasse", () => {
    expect(speed(20.34, false)).toEqual({ value: "20,3", unit: "km/h" });
    expect(speed(10, true)).toEqual({ value: "6:00", unit: "/km" });
    expect(speed(null, false)).toEqual({ value: "–", unit: "km/h" });
  });

  it("Stunden und Minuten für Summen", () => {
    expect(hoursMinutes(55_800_000)).toBe("15:30");
    expect(hoursMinutes(3_600_000 * 1234 + 60_000)).toBe("1.234:01");
  });

  it("lokaler Tag in der Zeitzone und Tage verschieben", () => {
    expect(localDate(new Date("2026-11-08T23:30:00Z"), "Europe/Berlin")).toBe("2026-11-09");
    expect(localDate(new Date("2026-11-08T22:30:00Z"), "UTC")).toBe("2026-11-08");
    expect(addDays("2026-03-30", -83)).toBe("2026-01-06");
    expect(addDays("2026-10-25", 1)).toBe("2026-10-26");
  });
});

describe("Segmentzeiten", () => {
  it("zeigt Zehntel und schneidet ab", () => {
    expect(segmentTime(271_250)).toBe("4:31,2");
    expect(segmentTime(3_725_990)).toBe("1:02:05,9");
    expect(segmentTime(59_999)).toBe("0:59,9");
    expect(segmentTime(null)).toBe("–");
  });
});

describe("Formate nach Gebietsschema (#42)", () => {
  afterEach(() => {
    window.localStorage.clear();
    resetLocaleForTests();
  });

  it("de-DE bleibt beim bisherigen Aussehen", () => {
    expect(dateTime("2026-03-04T13:05:00Z", "Europe/Berlin")).toBe("04.03.26 14:05");
    expect(dateTime("2026-03-04T13:05:00Z", "Europe/Berlin", true)).toBe("04.03.2026 14:05");
    expect(dateOnly("2026-03-04T13:05:00Z", "Europe/Berlin", true)).toBe("04.03.2026");
    expect(dayDate("2026-11-24")).toBe("24.11.2026");
    expect(dayMonth("2026-11-24")).toBe("24.11.");
    expect([dayRange("2026-11-23", "2026-11-29"), dayRange("2026-11-30", "2026-12-06")]).toEqual(["23.–29.11.", "30.11.–06.12."]);
    expect([0, 2, 6].map(weekdayShort)).toEqual(["Mo", "Mi", "So"]);
    expect([monthShort(2), monthShort(4), monthLong(2)]).toEqual(["Mär", "Mai", "März"]);
    expect(monthYear("2026-11-15")).toBe("November 2026");
  });

  it("en-US: Zahlen, Datum, 12-Stunden-Uhr, Namen", () => {
    setLocale({ format: "en-US" });
    expect(num(1234.5, 1)).toBe("1,234.5");
    expect(segmentTime(3_725_990)).toBe("1:02:05.9");
    expect(dateTime("2026-03-04T13:05:00Z", "Europe/Berlin")).toBe("03/04/26 02:05 PM");
    expect(dayDate("2026-11-24")).toBe("11/24/2026");
    expect([0, 6].map(weekdayShort)).toEqual(["Mon", "Sun"]);
    expect(monthShort(2)).toBe("Mar");
  });

  it("Sprache und Format getrennt: englische Texte mit deutschem Format", () => {
    setLocale({ language: "en", format: "de-DE" });
    expect(num(1234.5, 1)).toBe("1.234,5");
    expect(dayDate("2026-11-24")).toBe("24.11.2026");
  });
});
