// Testdaten für die Aktivitätenseiten (nachgebildeter Server).
import type { Activity, ActivitySummary } from "../../api/client";

export function activity(over: Partial<Activity> = {}): Activity {
  return {
    id: "a1",
    title: "Runde Wald",
    description: null,
    location: null,
    sport_class: "cycling",
    source_sport: "cycling",
    sub_sport: "mtb",
    start_time: "2026-11-01T09:00:00.000Z",
    start_time_origin: "file",
    start_time_original: null,
    total_time_ms: 3_600_000,
    total_time_origin: "file",
    total_time_original_ms: null,
    distance_m: 25_000,
    distance_origin: "file",
    elevation_gain_m: 312,
    elevation_origin: "hysteresis",
    avg_speed_kmh: 25,
    avg_heart_rate_bpm: 128,
    filtered_outliers_count: 0,
    section_count: 1,
    point_count: 3600,
    has_position: true,
    segment_success_count: 0,
    revision: 1,
    ...over,
  };
}

export function summary(over: Partial<ActivitySummary> = {}): ActivitySummary {
  return {
    count: 1,
    distance_m: 25_000,
    total_time_ms: 3_600_000,
    elevation_gain_m: 312,
    activities_without_distance: 0,
    activities_without_elevation: 0,
    mixed_sport_classes: false,
    avg_speed_kmh: 25,
    avg_heart_rate_bpm: 128,
    segment_success_count: 0,
    ...over,
  };
}

export const config = {
  data: {
    timezone: "Europe/Berlin",
    user: { display_name: "Uli" },
    map: { tile_url: "https://tile.example/{z}/{x}/{y}.png", attribution: "OSM", track_max_points: 2000 },
    import_limits: { max_file_size_mb: 50, max_points_per_session: 100000 },
    segments: { minimum_length_m: 20, route_edge_m: 1000 },
    sub_sport_catalog: {
      cycling: [
        { key: "road", name: null, builtin: true },
        { key: "mtb", name: null, builtin: true },
      ],
    },
  },
};

export function json(status: number, body: unknown) {
  return new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });
}
