import { addDays, dateTime, duration, hoursMinutes, km, localDate, megabytes, num, pace, parseDuration, segmentTime, speed } from "./format";

describe("Formate (RFC-0006)", () => {
  it("Zahlen mit Komma und Tausenderpunkt", () => {
    expect(num(1234.5, 1)).toBe("1.234,5");
    expect(km(136210)).toBe("136,21");
    expect(km(null)).toBe("–");
    expect(megabytes(1.5 * 1024 * 1024)).toBe("1,5");
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
