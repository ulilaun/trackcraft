// Zugriff auf /api/v1 (RFC-0003). Keine dauerhafte Ablage der Antworten (RFC-0011).
import type { SportGlyphId } from "../components/glyphs";

export const API_BASE = "/api/v1";

export interface FieldError {
  field: string;
  code: string;
  message?: string;
}

export class ApiError extends Error {
  readonly status: number;
  readonly code: string;
  readonly fields: FieldError[];
  /** Übrige `error.details` außer den Feldern, z. B. `season` bei `season_overlap`. */
  readonly details: Record<string, unknown>;
  constructor(status: number, code: string, message: string, fields: FieldError[] = [], details: Record<string, unknown> = {}) {
    super(message);
    this.status = status;
    this.code = code;
    this.fields = fields;
    this.details = details;
  }
}

/** Netzwerk nicht erreichbar (Rechner aus, VPN getrennt). */
export class OfflineError extends Error {}

type Fetch = typeof fetch;

interface ErrorBody {
  error?: { code?: string; message?: string; details?: { fields?: FieldError[] } & Record<string, unknown> };
}

function toApiError(status: number, statusText: string, body: unknown): ApiError {
  const err = (body as ErrorBody | null)?.error;
  return new ApiError(status, err?.code ?? "internal_error", err?.message ?? statusText, err?.details?.fields ?? [], err?.details ?? {});
}

async function request<T>(method: string, path: string, body: unknown, fetcher: Fetch): Promise<T> {
  let res: Response;
  const headers: Record<string, string> = { Accept: "application/json" };
  if (body !== undefined) headers["Content-Type"] = "application/json";
  try {
    res = await fetcher(`${API_BASE}${path}`, {
      method,
      headers,
      cache: "no-store",
      body: body === undefined ? undefined : JSON.stringify(body),
    });
  } catch (e) {
    throw new OfflineError(String(e));
  }
  if (res.status === 204) return undefined as T;
  const json: unknown = await res.json().catch(() => null);
  if (!res.ok) throw toApiError(res.status, res.statusText, json);
  return json as T;
}

export function apiGet<T>(path: string, fetcher: Fetch = fetch): Promise<T> {
  return request<T>("GET", path, undefined, fetcher);
}

export function apiPost<T>(path: string, body: unknown, fetcher: Fetch = fetch): Promise<T> {
  return request<T>("POST", path, body, fetcher);
}

export function apiPatch<T>(path: string, body: unknown, fetcher: Fetch = fetch): Promise<T> {
  return request<T>("PATCH", path, body, fetcher);
}

export function apiPut<T>(path: string, body: unknown, fetcher: Fetch = fetch): Promise<T> {
  return request<T>("PUT", path, body, fetcher);
}

export function apiDelete(path: string, fetcher: Fetch = fetch): Promise<void> {
  return request<void>("DELETE", path, undefined, fetcher);
}

/**
 * Datei-Upload als `multipart/form-data` mit Fortschritt (fetch kennt keinen Upload-Fortschritt).
 * `onProgress` erhält 0 bis 1.
 */
export function apiUpload<T>(path: string, file: File, onProgress: (fraction: number) => void): Promise<T> {
  return new Promise((resolve, reject) => {
    const xhr = new XMLHttpRequest();
    xhr.open("POST", `${API_BASE}${path}`);
    xhr.setRequestHeader("Accept", "application/json");
    xhr.upload.onprogress = (e) => {
      if (e.lengthComputable && e.total > 0) onProgress(e.loaded / e.total);
    };
    xhr.onerror = () => reject(new OfflineError("upload failed"));
    xhr.onload = () => {
      let json: unknown = null;
      try {
        json = JSON.parse(xhr.responseText);
      } catch {
        /* kein JSON */
      }
      if (xhr.status >= 200 && xhr.status < 300) resolve(json as T);
      else reject(toApiError(xhr.status, xhr.statusText, json));
    };
    const form = new FormData();
    form.append("file", file, file.name);
    xhr.send(form);
  });
}

export interface VersionResponse {
  data: { app_version: string; api_version: "v1"; schema_version: number; algorithm_version: number };
}

export type CheckStatus = "ok" | "warn" | "fail";
export interface HealthResponse {
  data: {
    status: "ok" | "degraded" | "fail";
    checks: { name: "backend" | "database" | "filesystem" | "storage"; status: CheckStatus; message: string | null }[];
  };
}

export type SportClass = "cycling" | "ebike" | "running" | "skiing" | "swimming" | "studio" | "unknown";

/** Untertyp im Katalog: fester Schlüssel; `name` ist ein eigener Name, sonst übersetzt die Oberfläche. */
export interface SubSportEntry {
  key: string;
  name: string | null;
  /** gewähltes Symbol; `null` = Vorgabe nach Klasse und Schlüssel */
  icon?: SportGlyphId | null;
  builtin: boolean;
}

export type SubSportCatalog = Record<string, SubSportEntry[]>;

/** Zuordnung eines Sporttyps aus der Datei (`GET /sport-mappings`). */
export interface SportMapping {
  source_sport: string;
  /** eigener Eintrag vorhanden */
  mapped: boolean;
  sport_class: SportClass | null;
  sub_sport: string | null;
  /** was ein neuer Import ergibt */
  effective_sport_class: SportClass;
  effective_sub_sport: string | null;
  activity_count: number;
}

export interface SportMappingListResponse {
  data: SportMapping[];
  meta: { total: number };
}

/** Katalogeintrag mit Verwendung (`GET /sub-sports`). */
export interface SubSport extends SubSportEntry {
  sport_class: SportClass;
  activity_count: number;
  mapping_count: number;
}

export interface SubSportListResponse {
  data: SubSport[];
  meta: { total: number };
}

export interface ConfigResponse {
  data: {
    timezone: string;
    user: { display_name: string };
    map: { tile_url: string; attribution: string; track_max_points: number };
    import_limits: { max_file_size_mb: number; max_points_per_session: number };
    segments: { minimum_length_m: number; route_edge_m: number };
    /** Untertyp-Katalog je Sportklasse aus der Datenbank */
    sub_sport_catalog: SubSportCatalog;
    /** `[demo] enabled`: Hinweisband „Demo mit erfundenen Daten“ */
    demo?: boolean;
  };
}

// Import (RFC-0003 Abschnitt 8, OpenAPI ImportJob)

export type ImportStatus = "staged" | "awaiting_decision" | "running" | "done" | "failed" | "rolled_back";
export type SessionState = "selectable" | "locked_existing" | "locked_limit" | "locked_vehicle";

export interface DuplicateCandidate {
  activity_id: string;
  title: string;
  sport_class: SportClass;
  start_time: string;
  total_time_ms: number;
  distance_m: number | null;
  matched_criteria: ("start_time" | "total_time" | "distance")[];
}

export interface ImportSession {
  session_index: number;
  state: SessionState;
  lock_reason: string | null;
  preselected: boolean;
  title: string;
  source_sport: string | null;
  sport_class: SportClass;
  sub_sport: string | null;
  start_time: string | null;
  total_time_ms: number | null;
  distance_m: number | null;
  avg_heart_rate_bpm: number | null;
  point_count: number;
  has_position: boolean;
  time_input_required: boolean;
  existing_activity_id: string | null;
  duplicate_candidates: DuplicateCandidate[];
}

export interface ImportJob {
  id: string;
  status: ImportStatus;
  original_filename: string;
  format: "fit" | "gpx" | null;
  size_bytes: number | null;
  state_token: string | null;
  error: { code: string; message: string } | null;
  sessions: ImportSession[];
  imported_activity_ids: string[];
  created_at: string;
  finished_at: string | null;
}

export interface ImportJobResponse {
  data: ImportJob;
  meta?: { notices?: { code: string; message: string }[] };
}

export interface CarryOver {
  title: boolean;
  description: boolean;
  location: boolean;
  sport_class: boolean;
  sub_sport: boolean;
}

export interface SessionDecision {
  session_index: number;
  action: "import" | "replace" | "skip";
  replace_activity_id?: string;
  title?: string;
  carry_over?: CarryOver;
  start_time?: { local: string; occurrence?: "first" | "second" };
  total_time_ms?: number;
}

export interface ImportDecision {
  state_token: string;
  sessions: SessionDecision[];
}

// Aktivitäten (RFC-0003, OpenAPI Activity, ActivityListMeta, TrackResponse)

export interface Activity {
  id: string;
  title: string;
  description: string | null;
  location: string | null;
  sport_class: SportClass;
  source_sport: string | null;
  sub_sport: string | null;
  start_time: string;
  start_time_origin: string;
  start_time_original: string | null;
  total_time_ms: number;
  total_time_origin: string;
  total_time_original_ms: number | null;
  distance_m: number | null;
  distance_origin: string | null;
  elevation_gain_m: number | null;
  elevation_origin: "file" | "hysteresis" | null;
  avg_speed_kmh: number | null;
  avg_heart_rate_bpm: number | null;
  filtered_outliers_count: number;
  section_count: number;
  point_count: number;
  has_position: boolean;
  segment_success_count: number;
  /** Belastung (TRIMP), abgeleitet; `null`, solange nicht berechnet. */
  load: number | null;
  load_method: LoadMethod | null;
  revision: number;
}

/** Detailansicht: Aktivität mit Belastung samt Zonen. */
export interface ActivityDetail extends Activity {
  /** Nur bei `GET /activities/{id}`; die Antwort auf PATCH enthält sie nicht. */
  training_load?: ActivityLoad | null;
}

export interface ActivitySummary {
  count: number;
  distance_m: number;
  total_time_ms: number;
  elevation_gain_m: number;
  activities_without_distance: number;
  activities_without_elevation: number;
  mixed_sport_classes: boolean;
  avg_speed_kmh: number | null;
  avg_heart_rate_bpm: number | null;
  segment_success_count: number;
  /** Summe der berechneten Belastungen (TRIMP), gerundet */
  load: number | null;
}

export interface ActivityListResponse {
  data: Activity[];
  meta: { total: number; limit: number; offset: number; summary: ActivitySummary };
}

export interface ActivityResponse {
  data: ActivityDetail;
  meta?: { notices?: { code: string; message: string }[] };
}

/** PATCH /activities/{id}: nur geänderte Felder, `null` löscht ein Freitextfeld. */
export interface ActivityUpdate {
  title?: string;
  description?: string | null;
  location?: string | null;
  sport_class?: SportClass;
  sub_sport?: string | null;
  start_time?: { local: string; occurrence?: "first" | "second" };
  total_time_ms?: number;
}

/** GET /activities/{id}/location-suggestion (#22): `null` ohne Vorschlag. */
export interface LocationSuggestion {
  location: string;
  source: "own" | "osm";
  /** Abstand zum nächsten Start mit diesem Ort (nur eigene Daten) */
  distance_m: number | null;
}
export interface LocationSuggestionResponse {
  data: LocationSuggestion | null;
}

/** GET/PATCH /places/settings (#22) */
export interface PlaceSettingsResponse {
  data: { osm_enabled: boolean };
}

/** GET/POST /segments/{id}/name-transfer: Name und Ort einer Strecke auf ihre Aktivitäten (#23). */
export interface NameTransferResponse {
  data: { name: string; location: string | null; activity_count: number; changed_count: number };
}

export interface DeletionPreviewResponse {
  data: {
    object_type: "activity" | "segment";
    effort_count: number;
    affected_segment_count: number;
    original_file_removed: boolean | null;
  };
}

export interface TrackSection {
  section_index: number;
  point_count: number;
  returned_point_count: number;
  simplified: boolean;
  /** Positionen der gelieferten Punkte im Abschnitt, Grundlage für start_index/end_index. */
  index: number[];
  t_ms: (number | null)[];
  lat: (number | null)[];
  lon: (number | null)[];
  ele_m: (number | null)[];
  hr: (number | null)[];
  dist_m: (number | null)[];
}

export interface TrackResponse {
  data: { sections: TrackSection[] };
}

// Segmente und Strecken (RFC-0016, OpenAPI Segment, Effort)

/** Sportklassen mit Segmenten und Bestenlisten (Skifahren ja, Schwimmen und Studio nicht). */
export type RankedSportClass = "cycling" | "ebike" | "running" | "skiing";
export type SegmentKind = "segment" | "route";
/** Segment: climb/descent/rolling, Strecke: loop/out_and_back/point_to_point (RFC-0016 „Kategorien“). */
export type SegmentCategory = "climb" | "descent" | "rolling" | "loop" | "out_and_back" | "point_to_point";
export type CalculationStatus = "none" | "current" | "running" | "stale" | "failed";

export interface Calculation {
  status: CalculationStatus;
  version_id: string | null;
  published_at: string | null;
}

export interface Effort {
  id: string;
  segment_id: string;
  segment_name: string;
  activity_id: string;
  activity_title: string;
  /** Sportklasse und Untertyp der Aktivität (Symbol in der Bestenliste) */
  activity_sport_class?: SportClass;
  activity_sub_sport?: string | null;
  attempt_position: number;
  status: "valid" | "not_ranked";
  not_ranked_reason: "ambiguous_geometry" | "time_invalid" | null;
  start_time: string | null;
  end_time: string | null;
  duration_ms: number | null;
  rank: number | null;
  record_kind: "first_best" | "improvement" | "equaled" | null;
  improvement_ms: number | null;
  coverage_ratio: number | null;
  avg_heart_rate_bpm: number | null;
  max_heart_rate_bpm: number | null;
  /** Nur in den Versuchen einer Aktivität. */
  segment?: {
    kind: SegmentKind;
    category: SegmentCategory | null;
    sport_class: RankedSportClass;
    length_m: number;
    elevation_gain_m: number | null;
    average_grade_pct: number | null;
    ranked_count: number;
    /** Heutige Bestzeit des Segments bzw. der Strecke */
    best_ms?: number | null;
  };
}

export interface EffortListResponse {
  data: Effort[];
  meta: { total?: number; calculation: Calculation };
}

export interface Segment {
  id: string;
  name: string;
  description: string | null;
  sport_class: RankedSportClass;
  kind: SegmentKind;
  /** Wirksam: von Hand gewählt, sonst automatisch. */
  category: SegmentCategory | null;
  category_auto: SegmentCategory | null;
  location: string | null;
  start?: { lat: number; lon: number };
  end?: { lat: number; lon: number };
  length_m: number;
  elevation_gain_m: number | null;
  elevation_loss_m: number | null;
  average_grade_pct: number | null;
  history: { status: "running" | "complete" | "failed"; processed_items: number; total_items: number };
  summary: {
    attempt_count: number;
    valid_attempt_count: number;
    best: Effort | null;
    average_duration_ms: number | null;
    first_attempt_time: string | null;
    last_attempt_time: string | null;
  };
}

export interface SegmentListSummary {
  count: number;
  attempt_count: number;
  length_m: number;
  improvement_count: number;
}

export interface SegmentListResponse {
  data: Segment[];
  meta: { total: number; limit: number; offset: number; summary: SegmentListSummary };
}

/** PATCH /segments/{id}: nur geänderte Felder; ein Wechsel der Art wertet die Historie neu aus. */
export interface SegmentUpdate {
  name?: string;
  description?: string | null;
  kind?: SegmentKind;
  location?: string | null;
  /** `null`: zurück zur automatischen Kategorie. */
  category?: SegmentCategory | null;
}

export interface SegmentTrackResponse {
  data: {
    point_count: number;
    returned_point_count: number;
    simplified: boolean;
    index: number[];
    lat: number[];
    lon: number[];
    ele_m: (number | null)[];
    dist_m: number[];
  };
}

export interface AttemptListResponse {
  data: Effort[];
  meta: { total: number; limit: number; offset: number; calculation: Calculation };
}

export interface SegmentResponse {
  data: Segment;
  meta: { calculation: Calculation };
}

export interface SegmentCreate {
  activity_id: string;
  section_index: number;
  start_index: number;
  end_index: number;
  name: string;
  description?: string | null;
  sport_class: RankedSportClass;
  kind: SegmentKind;
  location?: string | null;
}

/** Strecke mit fast gleichem Verlauf (OpenAPI SimilarRoutesResponse) */
export interface SimilarRoute {
  id: string;
  name: string;
  length_m: number;
  overlap: number;
}

// Statistik (OpenAPI DashboardStatsResponse, RFC-0018)

export interface PeriodSummary {
  period: string;
  sport_class: SportClass | "all";
  count: number;
  distance_m: number;
  total_time_ms: number;
  elevation_gain_m: number;
  activities_without_distance: number;
  activities_without_elevation: number;
}

export interface DayBucket {
  date: string;
  sport_class: SportClass;
  count: number;
  distance_m: number;
  total_time_ms: number;
}

export interface PeriodBlock {
  period: string;
  from: string;
  to: string;
  summaries: PeriodSummary[];
  by_day: DayBucket[];
}

export interface DashboardStatsResponse {
  /** `year`: laufendes Kalenderjahr, die Seite zeigt nur die Summen */
  data: { week: PeriodBlock; month: PeriodBlock; year: PeriodBlock };
}

// Benutzerprofil (OpenAPI Profile)

export type AvatarColor = "blue" | "teal" | "green" | "amber" | "orange" | "red" | "pink" | "violet";

export interface Profile {
  id: string;
  nickname: string;
  first_name: string | null;
  last_name: string | null;
  /** YYYY-MM-DD */
  birthday: string | null;
  avatar_color: AvatarColor | null;
  updated_at: string;
}

export interface ProfileResponse {
  data: Profile;
}

export interface ProfileInput {
  nickname?: string;
  first_name?: string | null;
  last_name?: string | null;
  birthday?: string | null;
  avatar_color?: AvatarColor | null;
}

// Saisons (OpenAPI Season, RFC-0009 „Saisons“)

export type SeasonState = "current" | "ended" | "upcoming";

export interface Season {
  id: string;
  name: string;
  start_date: string;
  end_date: string;
  state: SeasonState;
  created_at: string;
  updated_at: string;
}

export interface SeasonListResponse {
  data: Season[];
  meta: { total: number; default_season_id: string | null; today: string };
}

export interface SeasonResponse {
  data: Season;
}

export interface SeasonInput {
  name?: string;
  start_date?: string;
  end_date?: string;
}

// Statistikseite (OpenAPI StatisticsResponse, StatisticsPeriodsResponse, RFC-0009)

export interface StatisticsSummary {
  sport_class: RankedSportClass | "all";
  count: number;
  distance_m: number;
  total_time_ms: number;
  elevation_gain_m: number;
  activities_without_distance: number;
  activities_without_elevation: number;
  segment_success_count: number;
}

export interface MonthBucket {
  month: string;
  sport_class: SportClass;
  count: number;
  distance_m: number;
  total_time_ms: number;
  elevation_gain_m: number;
}

export interface WeekBucket {
  week: string;
  week_start: string;
  sport_class: SportClass;
  count: number;
  distance_m: number;
  total_time_ms: number;
}

export type RecordKind = "distance" | "elevation" | "time" | "speed";

export interface RecordItem {
  sport_class: RankedSportClass;
  kind: RecordKind;
  value: number;
  equaled_count: number;
  activity: { id: string; title: string; start_time: string; sub_sport: string | null };
}

export interface StatisticsResponse {
  data: {
    from: string | null;
    to: string | null;
    first_day: string | null;
    last_day: string | null;
    summaries: StatisticsSummary[];
    by_month: MonthBucket[];
    by_week: WeekBucket[];
    records: RecordItem[];
  };
  meta: { calculation: Calculation };
}

export interface PeriodShortSummary {
  count: number;
  distance_m: number;
  total_time_ms: number;
}

export interface StatisticsPeriodsResponse {
  data: { seasons: (PeriodShortSummary & { season_id: string })[]; years: (PeriodShortSummary & { year: number })[] };
}

// ---------------------------------------------------------------------------------------------------------
// Garmin-Abruf (OpenAPI „Garmin“)

export type GarminActivityStatus = "imported" | "linked" | "locked" | "rejected" | "failed";

export interface GarminProgress {
  phase: "" | "token" | "list" | "download" | "import" | "idle";
  /** Zu bearbeitende Aktivitäten des Laufs; 0, solange die Liste gelesen wird */
  total: number;
  processed: number;
  imported: number;
  linked: number;
  locked: number;
  rejected: number;
  failed: number;
}

export interface GarminStatus {
  /** Schlüssel gesetzt; der Schlüssel selbst kommt nie zurück */
  configured: boolean;
  key_set_at: string | null;
  enabled: boolean;
  start_date: string | null;
  interval_minutes: number;
  backfill_done: boolean;
  running: boolean;
  progress: GarminProgress;
  last_run_started_at: string | null;
  last_run_finished_at: string | null;
  last_success_at: string | null;
  last_error: { code: string; message: string } | null;
  counts: Record<GarminActivityStatus, number>;
}

export interface GarminStatusResponse {
  data: GarminStatus;
}

export interface GarminSettingsUpdate {
  api_key?: string | null;
  enabled?: boolean;
  start_date?: string | null;
  interval_minutes?: number;
}

export interface GarminActivity {
  garmin_id: string;
  start_time: string;
  type_key: string | null;
  status: GarminActivityStatus;
  activity_id: string | null;
  activity_title: string | null;
  reason_code: string | null;
  reason: string | null;
  attempts: number;
  updated_at: string;
}

export interface GarminActivityListResponse {
  data: GarminActivity[];
  meta: { total: number; limit: number; offset: number };
}

// ---------------------------------------------------------------------------------------------------------
// Belastung und Form (E7)

/** samples: aus HF je Messpunkt, average: aus Durchschnitts-HF, estimated: ohne HF aus der Dauer geschätzt */
export type LoadMethod = "samples" | "average" | "estimated";
/** % der max. HF, % der HF-Reserve (Karvonen), % der Schwellen-HF (Friel) */
export type ZoneModel = "max" | "reserve" | "threshold";

export interface HrZoneSet {
  id: string;
  /** `null`: allgemeiner Satz */
  sport_class: SportClass | null;
  valid_from: string;
  max_hr: number;
  resting_hr: number;
  threshold_hr: number | null;
  model: ZoneModel;
  /** Untere Grenzen der fünf Zonen in Prozent */
  bounds: number[];
  /** Je Zone [von, bis] in Schlägen pro Minute; die letzte Zone ist nach oben offen. */
  zones_bpm: [number, number | null][];
  anaerobic_from_bpm: number;
}

export interface HrZoneSetInput {
  sport_class?: SportClass | null;
  valid_from?: string;
  max_hr?: number;
  resting_hr?: number;
  threshold_hr?: number | null;
  model?: ZoneModel;
  bounds?: number[];
}

export interface HrZonesResponse {
  data: HrZoneSet[];
  meta: { suggestion: { age: number | null; max_hr: number | null } };
}

export interface HrZoneSetResponse {
  data: HrZoneSet;
}

export interface ActivityLoad {
  trimp: number;
  aerobic: number;
  anaerobic: number;
  method: LoadMethod;
  zone_seconds: number[] | null;
  zone_set: HrZoneSet | null;
}

export interface LoadSettings {
  sex_factor: number;
  fitness_days: number;
  fatigue_days: number;
  estimate_per_hour: Record<SportClass, number>;
  /** Standard: gleitende Mittel; two_curve: Ermüdung aerob/anaerob getrennt, Ruhetags-Bonus */
  curve_model: CurveModel;
  aerobic_days: number;
  anaerobic_days: number;
  rest_bonus_pct: number;
  /** Foster: Aufschlag auf die Ermüdung bei Monotonie über 2,0 */
  foster_surcharge: boolean;
  foster_surcharge_pct: number;
  /** Form = Fitness − Gewicht × Ermüdung (#24), 0,25 bis 3 */
  fatigue_weight: number;
}

export type CurveModel = "standard" | "two_curve";

export interface LoadSettingsResponse {
  data: LoadSettings;
}

export type ForecastMode = "rest" | "steady" | "plan";

export interface FormDay {
  date: string;
  load: number;
  aerobic: number;
  anaerobic: number;
  estimated: number;
  fitness: number;
  fatigue: number;
  form: number;
  /** Nur im Zwei-Kurven-Modell */
  fatigue_aerobic: number | null;
  fatigue_anaerobic: number | null;
  /** Foster, letzte 7 Tage; `null` ohne Belastung oder ohne Streuung */
  monotony: number | null;
  strain: number | null;
}

export interface FormForecastDay {
  date: string;
  load: number;
  fitness: number;
  fatigue: number;
  form: number;
  fatigue_aerobic: number | null;
  fatigue_anaerobic: number | null;
  monotony: number | null;
  strain: number | null;
}

export interface FormSnapshot {
  date: string;
  fitness: number;
  fatigue: number;
  form: number;
  monotony: number | null;
  strain: number | null;
}

export interface FormResponse {
  data: {
    today: string;
    from: string;
    to: string;
    settings: LoadSettings;
    zones_configured: boolean;
    /** Aktivitäten, deren Belastung noch berechnet wird */
    pending: number;
    first_date: string | null;
    current: {
      fitness: number;
      fatigue: number;
      form: number;
      load_7d: number;
      load_prev_7d: number;
      fitness_week_ago: number;
      fatigue_week_ago: number;
      fatigue_aerobic: number | null;
      fatigue_anaerobic: number | null;
      monotony: number | null;
      strain: number | null;
      strain_week_ago: number | null;
      /** Kachelreihen (#25): vor 30 und 7 Tagen, in 7 Tagen mit der gewählten Vorschau */
      compare: { month_ago: FormSnapshot; week_ago: FormSnapshot; week_ahead: FormSnapshot | null };
    };
    /** Ab dieser Monotonie warnt die Seite (Foster) */
    monotony_warn: number;
    days: FormDay[];
    forecast: { mode: ForecastMode; daily_load: number; days: FormForecastDay[] };
    /** Trainingsplan (E8): alle Ziele und letzter Tag mit offener Einheit (ab morgen) */
    plan: { goals: PlanGoal[]; last_date: string | null };
  };
}

// ---------------------------------------------------------------------------------------------------------
// Trainingsplanung (E8)

/** Ziel (Wettkampf, Tour) als Ereignis im Kalender */
export interface PlanGoal {
  id: string;
  date: string;
  sport_class: SportClass;
  name: string;
  description: string | null;
  url: string | null;
  distance_m: number | null;
  target_time_s: number | null;
}

export interface PlanGoalResponse {
  data: PlanGoal;
}

export type PlannedStatus = "planned" | "skipped" | "done";

export interface PlannedUnit {
  id: string;
  date: string;
  sport_class: SportClass;
  sub_sport: string | null;
  title: string;
  note: string | null;
  mode: "target" | "zones";
  duration_min: number | null;
  target_load: number | null;
  intensive_pct: number;
  zone_minutes: number[] | null;
  /** „done“ = mit einer Aktivität verknüpft */
  status: PlannedStatus;
  activity_id: string | null;
  /** Serie (Wiederholen) */
  series_id?: string | null;
  /** Offene Einheiten derselben Serie nach diesem Tag */
  series_following?: number;
  load: { total: number; aerobic: number; anaerobic: number; minutes: number; method: "target" | "zones" | "estimated" };
  /** Nur bei verknüpften Einheiten: Ist-Werte für den Vergleich */
  actual?: {
    activity_id: string;
    title: string;
    start_time: string;
    total_time_ms: number;
    load: number | null;
    anaerobic: number | null;
    load_method: string | null;
    zone_seconds: number[] | null;
  } | null;
}

export interface PlanResponse {
  data: {
    today: string;
    units: PlannedUnit[];
    /** Alle Ziele nach Datum */
    goals: PlanGoal[];
    zones_configured: boolean;
    /** Zuletzt übernommene Eingaben der Planungshilfe */
    assistant: SuggestionInput | null;
  };
}

/** Eingaben der Planungshilfe (E8 Stufe 2) */
export interface SuggestionInput {
  /** Ziel, auf das geplant wird (wird nicht gemerkt) */
  goal_id?: string;
  sport_class: SportClass;
  /** Höchstens so viele Minuten je Wochentag, Montag zuerst */
  minutes: number[];
  max_gain: number;
  recovery_weeks: boolean;
  intensity: "polarized" | "base";
  /** Sprache der Titel vorgeschlagener Einheiten (wird nicht gemerkt) */
  language?: "de" | "en";
}

export type PlanPhase = "build" | "recovery" | "taper";

export interface SuggestionResponse {
  data: {
    today: string;
    goal_date: string;
    units: (PlannedUnit & { phase: PlanPhase })[];
    fixed_units: number;
    blocks: { phase: PlanPhase; from: string; to: string; factor: number }[];
    weeks: { start: string; phase: PlanPhase | null; load: number; monotony: number | null; low_zone_pct: number | null; units: number }[];
    summary: {
      taper_days: number;
      fitness_today: number;
      fitness_at_goal: number;
      form_at_goal: number;
      form_state: string;
      avg_gain: number | null;
      max_gain: number;
      limited_by_time: boolean;
      planned_load: number;
    };
    forecast: FormForecastDay[];
  };
}

export interface SuggestionApplyResponse {
  data: { created: number };
}

/** Einheiten-Vorlage: Einheit ohne Datum und Status unter einem Namen; Belastung mit dem heutigen Zonensatz. */
export type PlanTemplate = Omit<PlannedUnit, "date" | "status" | "activity_id" | "series_id" | "series_following" | "actual"> & {
  name: string;
};

export interface PlanTemplatesResponse {
  data: PlanTemplate[];
}

export interface PlanTemplateResponse {
  data: PlanTemplate;
}

export interface PlannedUnitResponse {
  data: PlannedUnit;
}

export interface ActivityPlanResponse {
  data: { linked: PlannedUnit | null; candidates: PlannedUnit[] };
}
