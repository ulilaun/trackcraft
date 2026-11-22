//! Entwicklerwerkzeug: zeigt Sperren und Meldungen der Aufbereitung (ohne Koordinaten).
//! Aufruf: cargo run --example lockinfo -- <datei> …
fn main() {
    let cfg_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("config").join(if cfg!(windows) {
        "trackcraft.example.toml"
    } else {
        "trackcraft.container.example.toml"
    });
    let cfg = trackcraft::config::load(&cfg_path).unwrap().raw;
    for path in std::env::args().skip(1) {
        let bytes = std::fs::read(&path).unwrap();
        match trackcraft::import::parse(&bytes) {
            Err(e) => println!("{path}: {}", e.code()),
            Ok(f) => {
                for s in &f.sessions {
                    let p = trackcraft::import::process::prepare(
                        s,
                        &cfg,
                        &trackcraft::sports::SportMapping::defaults(),
                        chrono_tz::Europe::Berlin,
                    );
                    let msg = match &p.lock {
                        Some(trackcraft::import::process::Lock::Vehicle(v)) => trackcraft::import::process::vehicle_message(
                            v,
                            p.sport_class,
                            chrono_tz::Europe::Berlin,
                            cfg.processing.vehicle_detection.window_s,
                        ),
                        Some(l) => format!("{l:?}"),
                        None => "keine Sperre".into(),
                    };
                    println!("{path} Session {}: {} | Titel: {:?}", s.session_index, msg, p.title);
                }
            }
        }
    }
}
