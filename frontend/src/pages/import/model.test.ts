import type { DuplicateCandidate, ImportJob, ImportSession } from "../../api/client";
import {
  autoConfirmable,
  autoDecision,
  classify,
  countOutcome,
  decisionFor,
  fieldSession,
  initialChoice,
  isOpen,
  lockedOf,
  lockedRows,
  precheck,
  rowsOf,
  savedIndices,
  savedRows,
  summarize,
  type Choice,
  type FileEntry,
  type Row,
} from "./model";

function at(rows: Row[], i: number): Row {
  const r = rows[i];
  if (!r) throw new Error(`Zeile ${i} fehlt`);
  return r;
}

function session(over: Partial<ImportSession> = {}): ImportSession {
  return {
    session_index: 0,
    state: "selectable",
    lock_reason: null,
    preselected: true,
    title: "Radfahren 22.10.2026 10:15",
    source_sport: "cycling",
    sport_class: "cycling",
    sub_sport: null,
    start_time: "2026-10-22T08:15:03.000Z",
    total_time_ms: 3_600_000,
    distance_m: 30_000,
    avg_heart_rate_bpm: 120,
    point_count: 3600,
    has_position: true,
    time_input_required: false,
    existing_activity_id: null,
    duplicate_candidates: [],
    ...over,
  };
}

const candidate: DuplicateCandidate = {
  activity_id: "old-1",
  title: "Alt",
  sport_class: "cycling",
  start_time: "2026-10-22T08:15:30.000Z",
  total_time_ms: 3_590_000,
  distance_m: 29_900,
  matched_criteria: ["start_time", "total_time", "distance"],
};

function job(id: string, sessions: ImportSession[]): ImportJob {
  return {
    id,
    status: "awaiting_decision",
    original_filename: `${id}.fit`,
    format: "fit",
    size_bytes: 1000,
    state_token: `st_${id}`,
    error: null,
    sessions,
    imported_activity_ids: [],
    created_at: "2026-11-08T10:00:00.000Z",
    finished_at: null,
  };
}

function entry(j: ImportJob): FileEntry {
  return { key: j.id, name: j.original_filename, size: 1000, status: "checked", progress: 1, job: j };
}

describe("Importdialog: Einordnung", () => {
  it("ordnet Sessions ein", () => {
    expect(classify(session())).toBe("ready");
    expect(classify(session({ duplicate_candidates: [candidate] }))).toBe("duplicate");
    expect(classify(session({ time_input_required: true, start_time: null }))).toBe("time");
    expect(classify(session({ state: "locked_vehicle" }))).toBe("locked");
    expect(classify(session({ state: "locked_existing", duplicate_candidates: [candidate] }))).toBe("locked");
  });

  it("FIT mit mehreren Sessions ergibt eine Zeile je Session", () => {
    const j = job("a", [session(), session({ session_index: 1, duplicate_candidates: [candidate] })]);
    const rows = rowsOf([entry(j), { key: "x", name: "x.zip", size: 5, status: "rejected", progress: 0 }]);
    expect(rows.map((r) => [r.key, r.kind])).toEqual([
      ["a:0", "ready"],
      ["a:1", "duplicate"],
    ]);
  });
});

describe("Importdialog: Entscheidungen", () => {
  it("Duplikat ist offen, bis eine Option gewählt ist", () => {
    const j = job("a", [session(), session({ session_index: 1, duplicate_candidates: [candidate] })]);
    const entries = [entry(j), { key: "r", name: "route.gpx", size: 10, status: "rejected" as const, progress: 1, error: { code: "gpx_without_time", message: "" } }];
    const rows = rowsOf(entries);
    let s = summarize(entries, rows, {});
    expect(s).toMatchObject({ files: 2, bytes: 1010, imported: 0, needDecision: 1, open: 1, notImportable: 1, toSave: 1 });

    const choices: Record<string, Choice> = { "a:1": { ...initialChoice(at(rows, 1)), action: "replace" } };
    s = summarize(entries, rows, choices);
    expect(s.open).toBe(0);
    expect(s.toSave).toBe(2);
    expect(decisionFor(j, rows, choices)).toEqual({
      state_token: "st_a",
      sessions: [
        { session_index: 0, action: "import" },
        {
          session_index: 1,
          action: "replace",
          replace_activity_id: "old-1",
          carry_over: { title: true, description: true, location: true, sport_class: true, sub_sport: true },
        },
      ],
    });
  });

  it("abgewählte und gesperrte Sessions werden übersprungen", () => {
    const j = job("a", [session(), session({ session_index: 1 }), session({ session_index: 2, state: "locked_limit" })]);
    const rows = rowsOf([entry(j)]);
    const choices: Record<string, Choice> = { "a:1": { ...initialChoice(at(rows, 1)), include: false } };
    expect(decisionFor(j, rows, choices).sessions).toEqual([
      { session_index: 0, action: "import" },
      { session_index: 1, action: "skip" },
      { session_index: 2, action: "skip" },
    ]);
  });

  it("Datei ohne Duplikat und Zeiteingabe wird ohne Rückfrage gespeichert", () => {
    const plain = job("a", [session(), session({ session_index: 1, state: "locked_vehicle" })]);
    expect(autoConfirmable(plain)).toBe(true);
    expect(autoDecision(plain).sessions).toEqual([
      { session_index: 0, action: "import" },
      { session_index: 1, action: "skip" },
    ]);
    // Nur gesperrt: wird ebenfalls abgeschlossen (nichts zu entscheiden).
    expect(autoConfirmable(job("b", [session({ state: "locked_existing" })]))).toBe(true);
    // Ein Duplikat oder eine Zeiteingabe hält die ganze Datei an.
    expect(autoConfirmable(job("c", [session(), session({ session_index: 1, duplicate_candidates: [candidate] })]))).toBe(false);
    expect(autoConfirmable(job("d", [session({ time_input_required: true })]))).toBe(false);
    expect(autoConfirmable({ ...plain, status: "done" })).toBe(false);
  });

  it("listet gespeicherte und gesperrte Sessions je Datei", () => {
    const j = job("a", [session(), session({ session_index: 1, state: "locked_existing" })]);
    const e: FileEntry = { ...entry({ ...j, status: "done" }), status: "saved", saved: [0], lockedSessions: lockedOf(j) };
    expect(savedRows([e]).map((r) => r.session.session_index)).toEqual([0]);
    expect(lockedRows([e]).map((r) => r.session.state)).toEqual(["locked_existing"]);
    expect(summarize([e], [], {})).toMatchObject({ imported: 1, notImportable: 1 });
    expect(savedIndices({ state_token: "x", sessions: [{ session_index: 0, action: "import" }, { session_index: 1, action: "skip" }] })).toEqual([0]);
  });

  it("Zeiteingabe braucht Start und Dauer, bei doppelter Herbststunde auch das Auftreten", () => {
    const j = job("a", [session({ time_input_required: true, start_time: null, total_time_ms: null, title: "Radfahren" })]);
    const rows = rowsOf([entry(j)]);
    const base = initialChoice(at(rows, 0));
    expect(isOpen(at(rows, 0), base)).toBe(true);
    const filled: Choice = { ...base, startLocal: "2026-10-25T02:30", durationText: "1:00:00" };
    expect(isOpen(at(rows, 0), filled)).toBe(false);
    expect(isOpen(at(rows, 0), { ...filled, ambiguous: true })).toBe(true);
    const chosen: Choice = { ...filled, ambiguous: true, occurrence: "second" };
    expect(isOpen(at(rows, 0), chosen)).toBe(false);
    expect(decisionFor(j, rows, { "a:0": chosen }).sessions[0]).toEqual({
      session_index: 0,
      action: "import",
      start_time: { local: "2026-10-25T02:30:00", occurrence: "second" },
      total_time_ms: 3_600_000,
    });
    // Abgewählt ist nichts offen.
    expect(isOpen(at(rows, 0), { ...base, include: false })).toBe(false);
  });
});

describe("Importdialog: Ergebnis und Hilfen", () => {
  it("zählt nur abgeschlossene Vorgänge als gespeichert", () => {
    const d = { state_token: "x", sessions: [{ session_index: 0, action: "import" as const }, { session_index: 1, action: "skip" as const }] };
    const zero = { imported: 0, replaced: 0, skipped: 0, failed: 0 };
    expect(countOutcome(d, { ...job("a", []), status: "done" }, zero)).toEqual({ imported: 1, replaced: 0, skipped: 1, failed: 0 });
    // Gesperrte Sessions zählen nicht als übersprungen.
    expect(countOutcome(d, { ...job("a", []), status: "done" }, zero, 1).skipped).toBe(0);
    expect(countOutcome(d, { ...job("a", []), status: "failed" }, zero)).toEqual({ imported: 0, replaced: 0, skipped: 0, failed: 1 });
  });

  it("findet die Session zu einem Feldfehler", () => {
    const d = { state_token: "x", sessions: [{ session_index: 3, action: "import" as const }] };
    expect(fieldSession("sessions[0].start_time", d)).toBe(3);
    expect(fieldSession("body", d)).toBeUndefined();
  });

  it("weist ZIP und zu große Dateien vor dem Upload ab", () => {
    expect(precheck({ name: "export.ZIP", size: 1 }, 50)?.code).toBe("unsupported_format");
    expect(precheck({ name: "a.fit", size: 50 * 1024 * 1024 + 1 }, 50)?.code).toBe("file_too_large");
    expect(precheck({ name: "a.fit", size: 50 * 1024 * 1024 }, 50)).toBeNull();
  });
});
