// Bettet Commit und Builddatum ein (RFC-0015). CI kann beides über Umgebungsvariablen vorgeben.
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=TRACKCRAFT_VERSION");
    println!("cargo:rerun-if-env-changed=TRACKCRAFT_COMMIT");
    println!("cargo:rerun-if-changed=../.git/HEAD");

    let commit = std::env::var("TRACKCRAFT_COMMIT").ok().filter(|s| !s.is_empty()).unwrap_or_else(|| {
        Command::new("git")
            .args(["rev-parse", "--short=12", "HEAD"])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_else(|| "unbekannt".into())
    });
    println!("cargo:rustc-env=TRACKCRAFT_COMMIT={commit}");

    let date = std::env::var("SOURCE_DATE_EPOCH").ok().and_then(|s| s.parse::<i64>().ok()).map(civil_date).unwrap_or_else(|| {
        let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
        civil_date(secs)
    });
    println!("cargo:rustc-env=TRACKCRAFT_BUILD_DATE={date}");
}

/// Datum aus Unix-Sekunden ohne Abhängigkeit (Algorithmus von H. Hinnant).
fn civil_date(secs: i64) -> String {
    let z = secs.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{y:04}-{m:02}-{d:02}")
}
