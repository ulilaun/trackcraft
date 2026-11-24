//! Zentrale Konfiguration nach RFC-0021: eine TOML-Datei, streng geprüft, keine Einzelwert-Overrides.

use std::collections::{BTreeMap, HashSet};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Einzige Umgebungsvariable der Anwendung: wählt nur den Pfad der TOML-Datei.
pub const CONFIG_ENV: &str = "TRACKCRAFT_CONFIG";

/// Sportklassen aus RFC-0008. `unknown` ist kein gültiges Mapping-Ziel.
pub const SPORT_CLASSES: [&str; 6] = ["cycling", "ebike", "running", "skiing", "swimming", "studio"];

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub app: AppSection,
    /// Nur noch Vorgabe des Spitznamens beim ersten Start; danach führt die Datenbank.
    #[serde(default)]
    pub user: Option<UserSection>,
    pub server: ServerSection,
    #[serde(default)]
    pub tls: Option<TlsSection>,
    pub paths: PathsSection,
    pub backup: BackupSection,
    #[serde(default)]
    pub logging: LoggingSection,
    #[serde(default)]
    pub monitoring: MonitoringSection,
    #[serde(default)]
    pub maps: MapsSection,
    #[serde(default)]
    pub import: ImportSection,
    #[serde(default)]
    pub processing: ProcessingSection,
    #[serde(default)]
    pub segments: SegmentsSection,
    #[serde(default)]
    pub garmin: GarminSection,
    #[serde(default)]
    pub places: PlacesSection,
    #[serde(default)]
    pub demo: DemoSection,
}

/// Demo mit erfundenen Daten: Hinweisband in der Oberfläche, beim Start Daten auf heute verschieben (nur
/// Demo-Datenbank, siehe Migration 0020).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DemoSection {
    #[serde(default)]
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppSection {
    #[serde(default = "default_timezone")]
    pub timezone: String,
}

impl Default for AppSection {
    fn default() -> Self {
        Self { timezone: default_timezone() }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UserSection {
    pub display_name: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServerSection {
    pub allowed_origins: Vec<String>,
    #[serde(default = "default_listen")]
    pub listen: String,
    #[serde(default)]
    pub static_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TlsSection {
    pub cert_file: PathBuf,
    pub key_file: PathBuf,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PathsSection {
    pub database_dir: PathBuf,
    pub activities_dir: PathBuf,
    pub logs_dir: PathBuf,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackupSection {
    pub target_dir: PathBuf,
    pub migration_dir: PathBuf,
    #[serde(default = "default_daily_time")]
    pub daily_time: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoggingSection {
    #[serde(default = "default_log_level")]
    pub level: String,
}

impl Default for LoggingSection {
    fn default() -> Self {
        Self { level: default_log_level() }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MonitoringSection {
    #[serde(default = "default_low_disk")]
    pub low_disk_free_gb: f64,
}

impl Default for MonitoringSection {
    fn default() -> Self {
        Self { low_disk_free_gb: default_low_disk() }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapsSection {
    #[serde(default = "default_tile_url")]
    pub tile_url: String,
    #[serde(default = "default_attribution")]
    pub attribution: String,
    /// Messpunkte je Abschnitt, die die Detailseite für Karte und Diagramm lädt (C2-Ausdünnung).
    #[serde(default = "default_track_max_points")]
    pub track_max_points: u64,
}

impl Default for MapsSection {
    fn default() -> Self {
        Self { tile_url: default_tile_url(), attribution: default_attribution(), track_max_points: default_track_max_points() }
    }
}

/// Abruf aus Garmin Connect. Schlüssel, Anfangsdatum und Intervall pflegt die Oberfläche (Datenbank).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GarminSection {
    /// Basis der persönlichen Garmin-API; nur für Tests ändern.
    #[serde(default = "default_garmin_api")]
    pub api_base_url: String,
    /// Pause zwischen zwei Downloads, schont die Garmin-API beim Gesamtimport.
    #[serde(default = "default_garmin_delay")]
    pub request_delay_ms: u64,
}

impl Default for GarminSection {
    fn default() -> Self {
        Self { api_base_url: default_garmin_api(), request_delay_ms: default_garmin_delay() }
    }
}

/// Ortsvorschlag (#22). Ob OpenStreetMap gefragt wird, pflegt die Oberfläche (Datenbank).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlacesSection {
    /// Basis des Nominatim-Dienstes für die Rückwärtssuche; nur für Tests oder eine eigene Instanz ändern.
    #[serde(default = "default_nominatim")]
    pub nominatim_url: String,
}

impl Default for PlacesSection {
    fn default() -> Self {
        Self { nominatim_url: default_nominatim() }
    }
}

fn default_nominatim() -> String {
    "https://nominatim.openstreetmap.org".into()
}

fn default_garmin_api() -> String {
    "https://personal.connectapi.garmin.com".into()
}
fn default_garmin_delay() -> u64 {
    1000
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportSection {
    #[serde(default = "default_max_file_size")]
    pub max_file_size_mb: u64,
    #[serde(default = "default_max_points")]
    pub max_points_per_session: u64,
    #[serde(default = "default_upload_timeout")]
    pub upload_timeout_minutes: u64,
    #[serde(default)]
    pub duplicates: DuplicatesSection,
    /// Veraltet seit Schemaversion 7: wird beim ersten Start einmalig in die Datenbank übernommen, danach ignoriert.
    #[serde(default)]
    pub sport_class_mapping: BTreeMap<String, String>,
    #[serde(default)]
    pub sub_sport_catalog: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub sub_sport_mapping: BTreeMap<String, String>,
}

impl Default for ImportSection {
    fn default() -> Self {
        Self {
            max_file_size_mb: default_max_file_size(),
            max_points_per_session: default_max_points(),
            upload_timeout_minutes: default_upload_timeout(),
            duplicates: DuplicatesSection::default(),
            sport_class_mapping: BTreeMap::new(),
            sub_sport_catalog: BTreeMap::new(),
            sub_sport_mapping: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DuplicatesSection {
    #[serde(default = "default_start_tolerance")]
    pub start_tolerance_s: u64,
    #[serde(default = "default_ratio_005")]
    pub duration_tolerance_ratio: f64,
    #[serde(default = "default_ratio_005")]
    pub distance_tolerance_ratio: f64,
}

impl Default for DuplicatesSection {
    fn default() -> Self {
        Self {
            start_tolerance_s: default_start_tolerance(),
            duration_tolerance_ratio: default_ratio_005(),
            distance_tolerance_ratio: default_ratio_005(),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessingSection {
    #[serde(default)]
    pub elevation: ElevationSection,
    #[serde(default)]
    pub outlier_filter: OutlierFilterSection,
    #[serde(default)]
    pub vehicle_detection: VehicleDetectionSection,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ElevationSection {
    #[serde(default = "default_hysteresis")]
    pub hysteresis_m: f64,
}

impl Default for ElevationSection {
    fn default() -> Self {
        Self { hysteresis_m: default_hysteresis() }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutlierFilterSection {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_max_speed")]
    pub default_max_speed_kmh: f64,
    #[serde(default)]
    pub max_speed_kmh: BTreeMap<String, f64>,
}

impl Default for OutlierFilterSection {
    fn default() -> Self {
        Self { enabled: true, default_max_speed_kmh: default_max_speed(), max_speed_kmh: BTreeMap::new() }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VehicleDetectionSection {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_min_duration")]
    pub min_duration_s: u64,
    #[serde(default = "default_window")]
    pub window_s: u64,
    #[serde(default = "default_max_pause")]
    pub max_pause_s: u64,
    #[serde(default)]
    pub peak_speed_kmh: BTreeMap<String, f64>,
    #[serde(default)]
    pub avg_speed_kmh: BTreeMap<String, f64>,
}

impl Default for VehicleDetectionSection {
    fn default() -> Self {
        Self {
            enabled: true,
            min_duration_s: default_min_duration(),
            window_s: default_window(),
            max_pause_s: default_max_pause(),
            peak_speed_kmh: BTreeMap::new(),
            avg_speed_kmh: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SegmentsSection {
    #[serde(default = "default_min_segment_length")]
    pub minimum_length_m: f64,
    /// Strecke: Start und Ziel höchstens so weit vom Rand des Abschnitts (entlang der Aufzeichnung, RFC-0016)
    #[serde(default = "default_route_edge")]
    pub route_edge_m: f64,
    /// Strecke: Start und Ziel höchstens so weit auseinander gilt als Rundkurs bzw. Hin & zurück (RFC-0016)
    #[serde(default = "default_route_loop_radius")]
    pub route_loop_radius_m: f64,
    #[serde(default)]
    pub matching: MatchingSection,
    /// Strecken: großzügigere Werte; alles andere wie `[segments.matching]` (RFC-0002 Abschnitt 4.8)
    #[serde(default)]
    pub route_matching: RouteMatchingSection,
}

impl Default for SegmentsSection {
    fn default() -> Self {
        Self {
            minimum_length_m: default_min_segment_length(),
            route_edge_m: default_route_edge(),
            route_loop_radius_m: default_route_loop_radius(),
            matching: MatchingSection::default(),
            route_matching: RouteMatchingSection::default(),
        }
    }
}

impl SegmentsSection {
    /// Matching-Parameter je Art (`segment` oder `route`).
    pub fn params_for(&self, kind: &str) -> MatchingSection {
        if kind != "route" {
            return self.matching.clone();
        }
        let r = &self.route_matching;
        MatchingSection {
            matching_radius_meters: r.matching_radius_meters,
            tolerance_m: r.tolerance_m,
            max_inner_gap_m: r.max_inner_gap_m,
            recording_edge_m: r.recording_edge_m,
            // Strecken: feste Lücke, sonst wären es auf 40 km 800 m
            max_inner_gap_ratio: 0.0,
            max_vector_angle_deg: r.max_vector_angle_deg,
            direction_reference_m: r.direction_reference_m,
            ..self.matching.clone()
        }
    }
}

/// Strecken über viele Kilometer: GPS weicht im Wald oder Tal abschnittsweise 15–35 m ab, und Aufzeichnungen
/// beginnen und enden einige Dutzend Meter vom Kontrollpunkt entfernt (Uli 12.11.2026).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RouteMatchingSection {
    #[serde(default = "default_35")]
    pub matching_radius_meters: f64,
    #[serde(default = "default_35")]
    pub tolerance_m: f64,
    #[serde(default = "default_200")]
    pub max_inner_gap_m: f64,
    /// Beginn bzw. Ende der Aufzeichnung höchstens so weit von Start bzw. Ziel zählt als Passage
    #[serde(default = "default_100")]
    pub recording_edge_m: f64,
    /// Am Ziel rollt man oft schräg in die Einfahrt: nur Gegenrichtung ausschließen; die Richtung sichern
    /// Abdeckung und Reihenfolge entlang der Strecke
    #[serde(default = "default_90")]
    pub max_vector_angle_deg: f64,
    /// Richtung an Start und Ziel über mehr als die letzten Meter der Aufzeichnung (Wenden, Einfahrt)
    #[serde(default = "default_50")]
    pub direction_reference_m: f64,
}

impl Default for RouteMatchingSection {
    fn default() -> Self {
        Self {
            matching_radius_meters: 35.0,
            tolerance_m: 35.0,
            max_inner_gap_m: 200.0,
            recording_edge_m: 100.0,
            max_vector_angle_deg: 90.0,
            direction_reference_m: 50.0,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MatchingSection {
    #[serde(default = "default_20")]
    pub matching_radius_meters: f64,
    #[serde(default = "default_45")]
    pub max_vector_angle_deg: f64,
    #[serde(default = "default_15")]
    pub max_offset_m: f64,
    #[serde(default = "default_20")]
    pub tolerance_m: f64,
    #[serde(default = "default_coverage")]
    pub min_coverage_ratio: f64,
    #[serde(default = "default_5")]
    pub direction_reference_m: f64,
    #[serde(default = "default_5")]
    pub max_backstep_m: f64,
    #[serde(default = "default_30")]
    pub max_inner_gap_m: f64,
    /// Lücke wächst mit der Länge: höchstens max(`max_inner_gap_m`, Anteil · Segmentlänge) (Uli 12.11.2026)
    #[serde(default = "default_gap_ratio")]
    pub max_inner_gap_ratio: f64,
    #[serde(default = "default_15_s")]
    pub max_interpolation_gap_s: u64,
    #[serde(default = "default_15_s")]
    pub coverage_connection_s: u64,
    /// Nur Strecken (aus `[segments.route_matching]`): Aufzeichnungsrand als Passage
    #[serde(skip, default = "default_15")]
    pub recording_edge_m: f64,
}

impl Default for MatchingSection {
    fn default() -> Self {
        Self {
            matching_radius_meters: 20.0,
            max_vector_angle_deg: 45.0,
            max_offset_m: 15.0,
            tolerance_m: 20.0,
            min_coverage_ratio: 0.95,
            direction_reference_m: 5.0,
            max_backstep_m: 5.0,
            max_inner_gap_m: 30.0,
            max_inner_gap_ratio: 0.02,
            max_interpolation_gap_s: 15,
            coverage_connection_s: 15,
            recording_edge_m: 15.0,
        }
    }
}

fn default_timezone() -> String {
    "Europe/Berlin".into()
}
fn default_listen() -> String {
    "0.0.0.0:8080".into()
}
fn default_daily_time() -> String {
    "03:00".into()
}
fn default_log_level() -> String {
    "info".into()
}
fn default_low_disk() -> f64 {
    1.0
}
fn default_tile_url() -> String {
    "https://tile.openstreetmap.org/{z}/{x}/{y}.png".into()
}
fn default_attribution() -> String {
    "© OpenStreetMap-Mitwirkende".into()
}
fn default_track_max_points() -> u64 {
    2000
}
fn default_max_file_size() -> u64 {
    50
}
fn default_max_points() -> u64 {
    100_000
}
fn default_upload_timeout() -> u64 {
    60
}
fn default_start_tolerance() -> u64 {
    60
}
fn default_ratio_005() -> f64 {
    0.05
}
fn default_hysteresis() -> f64 {
    3.0
}
fn default_true() -> bool {
    true
}
fn default_max_speed() -> f64 {
    100.0
}
fn default_min_duration() -> u64 {
    30
}
fn default_window() -> u64 {
    300
}
fn default_max_pause() -> u64 {
    60
}
fn default_min_segment_length() -> f64 {
    20.0
}
fn default_route_edge() -> f64 {
    1000.0
}
fn default_route_loop_radius() -> f64 {
    750.0
}
fn default_15() -> f64 {
    15.0
}
fn default_45() -> f64 {
    45.0
}
fn default_coverage() -> f64 {
    0.95
}
fn default_5() -> f64 {
    5.0
}
fn default_20() -> f64 {
    20.0
}
fn default_30() -> f64 {
    30.0
}
fn default_gap_ratio() -> f64 {
    0.02
}
fn default_35() -> f64 {
    35.0
}
fn default_50() -> f64 {
    50.0
}
fn default_90() -> f64 {
    90.0
}
fn default_100() -> f64 {
    100.0
}
fn default_200() -> f64 {
    200.0
}
fn default_15_s() -> u64 {
    15
}

/// Fehler beim Laden: entweder nicht lesbar/parsbar oder inhaltlich ungültig (Liste aller Befunde).
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Konfigurationsdatei {path} nicht lesbar: {source}")]
    Read { path: PathBuf, source: std::io::Error },
    #[error("Konfigurationsdatei {path} ist kein gültiges TOML oder enthält unbekannte Schlüssel: {message}")]
    Parse { path: PathBuf, message: String },
    #[error("Konfigurationsdatei {path} ist ungültig:\n  - {}", .problems.join("\n  - "))]
    Invalid { path: PathBuf, problems: Vec<String> },
}

/// Erlaubte Herkunft, zerlegt für die Host-Prüfung (RFC-0011).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Origin {
    pub origin: String,
    pub host: String,
    pub port: u16,
}

/// Geprüfte Konfiguration mit abgeleiteten Werten.
#[derive(Debug, Clone)]
pub struct Validated {
    pub path: PathBuf,
    pub raw: Config,
    pub listen: SocketAddr,
    pub origins: Vec<Origin>,
    pub timezone: chrono_tz::Tz,
    pub log_level: tracing::Level,
}

impl Validated {
    /// Staging-Ordner nach ADR-0011.
    pub fn staging_dir(&self) -> PathBuf {
        self.raw.paths.database_dir.join("staging")
    }

    pub fn database_file(&self) -> PathBuf {
        self.raw.paths.database_dir.join("trackcraft.db")
    }
}

/// Standardpfad der Konfiguration je Plattform (RFC-0005).
pub fn default_config_path() -> PathBuf {
    if cfg!(windows) {
        let base = std::env::var_os("ProgramData").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"));
        base.join("TrackCraft").join("config").join("trackcraft.toml")
    } else {
        // Nativ unter systemd (RFC-0005 „Linux“) /etc/trackcraft, im Container /config
        let native = PathBuf::from("/etc/trackcraft/trackcraft.toml");
        if native.is_file() { native } else { PathBuf::from("/config/trackcraft.toml") }
    }
}

/// Pfad aus `TRACKCRAFT_CONFIG` oder Standardpfad.
pub fn config_path() -> PathBuf {
    std::env::var_os(CONFIG_ENV).map(PathBuf::from).unwrap_or_else(default_config_path)
}

pub fn load(path: &Path) -> Result<Validated, ConfigError> {
    let text = std::fs::read_to_string(path).map_err(|source| ConfigError::Read { path: path.to_path_buf(), source })?;
    parse(path, &text)
}

pub fn parse(path: &Path, text: &str) -> Result<Validated, ConfigError> {
    let raw: Config = toml::from_str(text).map_err(|e| ConfigError::Parse { path: path.to_path_buf(), message: e.to_string() })?;
    validate(path, raw)
}

fn validate(path: &Path, raw: Config) -> Result<Validated, ConfigError> {
    let mut p: Vec<String> = Vec::new();

    let timezone = match raw.app.timezone.parse::<chrono_tz::Tz>() {
        Ok(tz) => Some(tz),
        Err(_) => {
            p.push(format!("[app] timezone: '{}' ist kein gültiger IANA-Name", raw.app.timezone));
            None
        }
    };
    if raw.user.as_ref().is_some_and(|u| u.display_name.trim().is_empty()) {
        p.push("[user] display_name darf nicht leer sein".into());
    }

    // Erlaubte Herkünfte
    let mut origins = Vec::new();
    if raw.server.allowed_origins.is_empty() {
        p.push("[server] allowed_origins braucht mindestens einen Eintrag".into());
    }
    let mut seen = HashSet::new();
    for o in &raw.server.allowed_origins {
        match parse_origin(o) {
            Ok(origin) => {
                if !seen.insert(o.clone()) {
                    p.push(format!("[server] allowed_origins: '{o}' kommt mehrfach vor"));
                }
                origins.push(origin);
            }
            Err(msg) => p.push(format!("[server] allowed_origins: '{o}' {msg}")),
        }
    }

    let g = raw.garmin.api_base_url.trim_end_matches('/');
    if !(g.starts_with("https://") || g.starts_with("http://127.0.0.1") || g.starts_with("http://localhost")) || g.len() < 12 {
        p.push(format!("[garmin] api_base_url: '{}' braucht https:// (http nur für 127.0.0.1/localhost)", raw.garmin.api_base_url));
    }
    let n = raw.places.nominatim_url.trim_end_matches('/');
    if !(n.starts_with("https://") || n.starts_with("http://127.0.0.1") || n.starts_with("http://localhost")) || n.len() < 12 {
        p.push(format!("[places] nominatim_url: '{}' braucht https:// (http nur für 127.0.0.1/localhost)", raw.places.nominatim_url));
    }
    if raw.garmin.request_delay_ms > 60_000 {
        p.push("[garmin] request_delay_ms: höchstens 60000".into());
    }

    let listen = match raw.server.listen.parse::<SocketAddr>() {
        Ok(a) => Some(a),
        Err(_) => {
            p.push(format!("[server] listen: '{}' ist keine gültige Adresse mit Port", raw.server.listen));
            None
        }
    };

    if let Some(dir) = &raw.server.static_dir {
        if !dir.is_absolute() {
            p.push(format!("[server] static_dir: '{}' ist kein absoluter Pfad", dir.display()));
        }
        if raw.tls.is_none() {
            p.push("[server] static_dir ist gesetzt, dann ist [tls] Pflicht (RFC-0011)".into());
        }
    }
    if let Some(tls) = &raw.tls {
        for (key, file) in [("cert_file", &tls.cert_file), ("key_file", &tls.key_file)] {
            if !file.is_absolute() {
                p.push(format!("[tls] {key}: '{}' ist kein absoluter Pfad", file.display()));
            }
        }
    }

    // Pfade
    let paths = [
        ("[paths] database_dir", &raw.paths.database_dir),
        ("[paths] activities_dir", &raw.paths.activities_dir),
        ("[paths] logs_dir", &raw.paths.logs_dir),
        ("[backup] migration_dir", &raw.backup.migration_dir),
        ("[backup] target_dir", &raw.backup.target_dir),
    ];
    for (key, dir) in paths {
        if !dir.is_absolute() {
            p.push(format!("{key}: '{}' ist kein absoluter Pfad", dir.display()));
        }
    }
    let target = normalize_for_compare(&raw.backup.target_dir);
    for (key, dir) in &paths[..4] {
        if normalize_for_compare(dir) == target {
            p.push(format!("[backup] target_dir muss sich von {key} unterscheiden"));
        }
    }
    if crate::platform::is_network_path(&raw.paths.database_dir) {
        p.push("[paths] database_dir darf nicht auf einer Netzwerkfreigabe liegen (SQLite)".into());
    }
    if crate::platform::is_mapped_network_drive(&raw.backup.target_dir) {
        p.push(
            "[backup] target_dir: verbundene Netzlaufwerke sieht der Dienst nicht, bitte UNC-Pfad angeben (\\\\server\\freigabe\\...)"
                .into(),
        );
    }

    if !valid_hh_mm(&raw.backup.daily_time) {
        p.push(format!("[backup] daily_time: '{}' hat nicht das Format HH:MM", raw.backup.daily_time));
    }

    let log_level = match raw.logging.level.as_str() {
        "error" => Some(tracing::Level::ERROR),
        "warn" => Some(tracing::Level::WARN),
        "info" => Some(tracing::Level::INFO),
        "debug" => Some(tracing::Level::DEBUG),
        "trace" => Some(tracing::Level::TRACE),
        other => {
            p.push(format!("[logging] level: '{other}' ist keiner von error, warn, info, debug, trace"));
            None
        }
    };

    positive(&mut p, "[monitoring] low_disk_free_gb", raw.monitoring.low_disk_free_gb);
    if raw.maps.tile_url.trim().is_empty() {
        p.push("[maps] tile_url darf nicht leer sein".into());
    }
    if !(2..=100_000).contains(&raw.maps.track_max_points) {
        p.push("[maps] track_max_points muss zwischen 2 und 100000 liegen".into());
    }

    // Import
    let imp = &raw.import;
    positive_int(&mut p, "[import] max_file_size_mb", imp.max_file_size_mb);
    positive_int(&mut p, "[import] max_points_per_session", imp.max_points_per_session);
    positive_int(&mut p, "[import] upload_timeout_minutes", imp.upload_timeout_minutes);
    positive_int(&mut p, "[import.duplicates] start_tolerance_s", imp.duplicates.start_tolerance_s);
    ratio(&mut p, "[import.duplicates] duration_tolerance_ratio", imp.duplicates.duration_tolerance_ratio);
    ratio(&mut p, "[import.duplicates] distance_tolerance_ratio", imp.duplicates.distance_tolerance_ratio);
    // Die Sportart-Abschnitte prüft und übernimmt sports::ensure.

    // Verarbeitung
    let pr = &raw.processing;
    positive(&mut p, "[processing.elevation] hysteresis_m", pr.elevation.hysteresis_m);
    positive(&mut p, "[processing.outlier_filter] default_max_speed_kmh", pr.outlier_filter.default_max_speed_kmh);
    for (k, v) in &pr.outlier_filter.max_speed_kmh {
        positive(&mut p, &format!("[processing.outlier_filter.max_speed_kmh] {k}"), *v);
    }
    let vd = &pr.vehicle_detection;
    positive_int(&mut p, "[processing.vehicle_detection] min_duration_s", vd.min_duration_s);
    positive_int(&mut p, "[processing.vehicle_detection] window_s", vd.window_s);
    positive_int(&mut p, "[processing.vehicle_detection] max_pause_s", vd.max_pause_s);
    if vd.window_s <= vd.min_duration_s {
        p.push("[processing.vehicle_detection] window_s muss größer als min_duration_s sein".into());
    }
    for (k, v) in &vd.peak_speed_kmh {
        positive(&mut p, &format!("[processing.vehicle_detection.peak_speed_kmh] {k}"), *v);
    }
    for (k, v) in &vd.avg_speed_kmh {
        positive(&mut p, &format!("[processing.vehicle_detection.avg_speed_kmh] {k}"), *v);
    }

    // Segmente
    let sg = &raw.segments;
    positive(&mut p, "[segments] minimum_length_m", sg.minimum_length_m);
    positive(&mut p, "[segments] route_edge_m", sg.route_edge_m);
    positive(&mut p, "[segments] route_loop_radius_m", sg.route_loop_radius_m);
    let m = &sg.matching;
    for (k, v) in [
        ("matching_radius_meters", m.matching_radius_meters),
        ("max_offset_m", m.max_offset_m),
        ("tolerance_m", m.tolerance_m),
        ("direction_reference_m", m.direction_reference_m),
        ("max_backstep_m", m.max_backstep_m),
        ("max_inner_gap_m", m.max_inner_gap_m),
    ] {
        positive(&mut p, &format!("[segments.matching] {k}"), v);
    }
    positive_int(&mut p, "[segments.matching] max_interpolation_gap_s", m.max_interpolation_gap_s);
    positive_int(&mut p, "[segments.matching] coverage_connection_s", m.coverage_connection_s);
    if !(m.min_coverage_ratio > 0.0 && m.min_coverage_ratio <= 1.0) {
        p.push("[segments.matching] min_coverage_ratio muss in (0, 1] liegen".into());
    }
    if !(m.max_vector_angle_deg > 0.0 && m.max_vector_angle_deg <= 90.0) {
        p.push("[segments.matching] max_vector_angle_deg muss in (0, 90] liegen".into());
    }
    if !(0.0..=0.5).contains(&m.max_inner_gap_ratio) {
        p.push("[segments.matching] max_inner_gap_ratio muss in [0, 0.5] liegen".into());
    }
    let rm = &sg.route_matching;
    for (k, v) in [
        ("matching_radius_meters", rm.matching_radius_meters),
        ("tolerance_m", rm.tolerance_m),
        ("max_inner_gap_m", rm.max_inner_gap_m),
        ("recording_edge_m", rm.recording_edge_m),
        ("direction_reference_m", rm.direction_reference_m),
    ] {
        positive(&mut p, &format!("[segments.route_matching] {k}"), v);
    }
    if !(rm.max_vector_angle_deg > 0.0 && rm.max_vector_angle_deg <= 90.0) {
        p.push("[segments.route_matching] max_vector_angle_deg muss in (0, 90] liegen".into());
    }

    if !p.is_empty() {
        return Err(ConfigError::Invalid { path: path.to_path_buf(), problems: p });
    }
    Ok(Validated {
        path: path.to_path_buf(),
        listen: listen.expect("geprüft"),
        origins,
        timezone: timezone.expect("geprüft"),
        log_level: log_level.expect("geprüft"),
        raw,
    })
}

/// Sportklasse eines Quellsports: exakter Schlüssel, sonst erbt der Untersport vom Sport (`sport-sub_sport`).
pub fn sport_class_for<'a>(mapping: &'a BTreeMap<String, String>, source: &str) -> Option<&'a str> {
    if let Some(c) = mapping.get(source) {
        return Some(c.as_str());
    }
    let (sport, _) = source.split_once('-')?;
    mapping.get(sport).map(String::as_str)
}

/// `https://host` oder `https://host:port`, Kleinbuchstaben, ohne Pfad, Query und Schrägstrich am Ende.
pub fn parse_origin(s: &str) -> Result<Origin, String> {
    let rest = s.strip_prefix("https://").ok_or("muss mit https:// beginnen")?;
    if s != s.to_lowercase() {
        return Err("muss in Kleinbuchstaben stehen".into());
    }
    if rest.is_empty() || rest.contains(['/', '?', '#', '@']) {
        return Err("darf keinen Pfad, keine Query und keinen Schrägstrich am Ende enthalten".into());
    }
    // IPv6-Literale sind im Heimnetz nicht vorgesehen; ein Doppelpunkt trennt den Port.
    let (host, port) = match rest.rsplit_once(':') {
        Some((h, port)) => match port.parse::<u16>() {
            Ok(port) if port > 0 => (h.to_string(), port),
            _ => return Err("hat einen ungültigen Port".into()),
        },
        None => (rest.to_string(), 443),
    };
    if host.is_empty() {
        return Err("hat keinen Hostnamen".into());
    }
    Ok(Origin { origin: s.to_string(), host, port })
}

fn valid_hh_mm(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 5 || b[2] != b':' {
        return false;
    }
    match (s[..2].parse::<u8>(), s[3..].parse::<u8>()) {
        (Ok(h), Ok(m)) => b[..2].iter().chain(&b[3..]).all(u8::is_ascii_digit) && h < 24 && m < 60,
        _ => false,
    }
}

fn positive(p: &mut Vec<String>, key: &str, v: f64) {
    if !(v.is_finite() && v > 0.0) {
        p.push(format!("{key} muss größer 0 sein"));
    }
}

fn positive_int(p: &mut Vec<String>, key: &str, v: u64) {
    if v == 0 {
        p.push(format!("{key} muss größer 0 sein"));
    }
}

fn ratio(p: &mut Vec<String>, key: &str, v: f64) {
    if !(v > 0.0 && v <= 1.0) {
        p.push(format!("{key} muss in (0, 1] liegen"));
    }
}

/// Lexikalischer Vergleich von Pfaden; unter Windows ohne Groß-/Kleinschreibung und mit `\`.
pub fn normalize_for_compare(path: &Path) -> String {
    let s = path.to_string_lossy();
    let s = s.trim_end_matches(['/', '\\']);
    if cfg!(windows) { s.replace('/', "\\").to_lowercase() } else { s.to_string() }
}
