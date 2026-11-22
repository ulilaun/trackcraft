//! Entwicklerwerkzeug: zeigt, was der Import aus Dateien liest (ohne Koordinaten).
//! Aufruf: cargo run --example inspect -- <datei> [<datei> …]

fn main() {
    for path in std::env::args().skip(1) {
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(e) => {
                println!("{path}: {e}");
                continue;
            }
        };
        match trackcraft::import::parse(&bytes) {
            Err(e) => println!("{path}: abgewiesen ({}): {e}", e.code()),
            Ok(f) => {
                for s in &f.sessions {
                    let pts: Vec<_> = s.sections.iter().flatten().collect();
                    let with = |f: &dyn Fn(&&trackcraft::import::RawPoint) -> bool| pts.iter().filter(|p| f(p)).count();
                    println!(
                        "{path} [{}] session {}: sport={:?} title={:?} file={:?} sections={} points={} time={} pos={} ele={} hr={} speed={} dist={}",
                        f.format.as_str(),
                        s.session_index,
                        s.source_sport,
                        s.title,
                        s.file,
                        s.sections.len(),
                        pts.len(),
                        with(&|p| p.point.t_ms.is_some()),
                        with(&|p| p.point.pos.is_some()),
                        with(&|p| p.point.ele_dm.is_some()),
                        with(&|p| p.point.hr.is_some()),
                        with(&|p| p.speed_mps.is_some()),
                        with(&|p| p.distance_m.is_some()),
                    );
                }
            }
        }
    }
}
