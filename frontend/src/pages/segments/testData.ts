// Testdaten für die Segmentseiten (nachgebildeter Server).
import type { Effort, Segment } from "../../api/client";

export function effort(over: Partial<Effort> = {}): Effort {
  return {
    id: "e1",
    segment_id: "s1",
    segment_name: "Anstieg Wald",
    activity_id: "a1",
    activity_title: "Runde Wald",
    attempt_position: 0,
    status: "valid",
    not_ranked_reason: null,
    start_time: "2026-10-01T09:10:00.000Z",
    end_time: "2026-10-01T09:15:00.000Z",
    duration_ms: 300_000,
    rank: 2,
    record_kind: "first_best",
    improvement_ms: null,
    coverage_ratio: 1,
    avg_heart_rate_bpm: 150,
    max_heart_rate_bpm: 171,
    ...over,
  };
}

export function segment(over: Partial<Segment> = {}): Segment {
  return {
    id: "s1",
    name: "Anstieg Wald",
    description: null,
    sport_class: "cycling",
    kind: "segment",
    category: "climb",
    category_auto: "climb",
    location: null,
    length_m: 1520,
    elevation_gain_m: 88,
    elevation_loss_m: 2,
    average_grade_pct: 5.8,
    history: { status: "complete", processed_items: 3, total_items: 3 },
    summary: {
      attempt_count: 3,
      valid_attempt_count: 2,
      best: effort({ id: "e2", activity_id: "a2", duration_ms: 271_250, rank: 1 }),
      average_duration_ms: 285_625,
      first_attempt_time: "2026-10-01T09:10:00.000Z",
      last_attempt_time: "2026-11-01T09:10:00.000Z",
    },
    ...over,
  };
}

/** Drei Versuche nach Zeit: Bestzeit (a2), erste Bestzeit (a1), nicht gewertet (a3). */
export function attempts(): Effort[] {
  return [
    effort({
      id: "e2",
      activity_id: "a2",
      activity_title: "Feierabendrunde",
      start_time: "2026-11-01T09:10:00.000Z",
      duration_ms: 271_250,
      rank: 1,
      record_kind: "improvement",
      improvement_ms: 28_750,
    }),
    effort(),
    effort({
      id: "e3",
      activity_id: "a3",
      activity_title: "Regenfahrt",
      start_time: "2026-10-15T09:10:00.000Z",
      status: "not_ranked",
      not_ranked_reason: "ambiguous_geometry",
      duration_ms: null,
      rank: null,
      record_kind: null,
    }),
  ];
}

export const track = {
  data: {
    point_count: 3,
    returned_point_count: 3,
    simplified: false,
    index: [0, 1, 2],
    lat: [50, 50.001, 50.002],
    lon: [8, 8, 8],
    ele_m: [100, 150, 188],
    dist_m: [0, 760, 1520],
  },
};
