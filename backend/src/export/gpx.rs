//! GPX 1.1 aus den aufbereiteten Punkten (RFC-0019 „Einzelne Aktivität“): je Abschnitt ein `trkseg`,
//! Höhe, Zeit und Herzfrequenz (Garmin TrackPointExtension v1). Punkte ohne Position fehlen.

use std::fmt::Write;

use quick_xml::escape::escape;

use crate::domain::track::Point;
use crate::import_job::timestamp;

pub struct Meta<'a> {
    pub title: &'a str,
    pub description: Option<&'a str>,
    pub sport_class: &'a str,
    pub start_time_ms: i64,
}

/// Grad mal 10^7 als Dezimalzahl ohne Rundungsrauschen.
fn deg(e7: i32) -> String {
    let sign = if e7 < 0 { "-" } else { "" };
    let a = e7.unsigned_abs();
    format!("{sign}{}.{:07}", a / 10_000_000, a % 10_000_000)
}

/// GPX-Text; `None`, wenn kein Punkt eine Position hat.
pub fn write(meta: &Meta<'_>, sections: &[Vec<Point>]) -> Option<String> {
    if !sections.iter().flatten().any(|p| p.pos.is_some()) {
        return None;
    }
    let mut s = String::with_capacity(sections.iter().map(Vec::len).sum::<usize>() * 120 + 1024);
    s.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    let _ = writeln!(
        s,
        "<gpx version=\"1.1\" creator=\"TrackCraft {}\" xmlns=\"http://www.topografix.com/GPX/1/1\" \
         xmlns:gpxtpx=\"http://www.garmin.com/xmlschemas/TrackPointExtension/v1\" \
         xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" \
         xsi:schemaLocation=\"http://www.topografix.com/GPX/1/1 http://www.topografix.com/GPX/1/1/gpx.xsd\">",
        escape(crate::version::APP_VERSION)
    );
    let _ = writeln!(
        s,
        "  <metadata>\n    <name>{}</name>\n    <time>{}</time>\n  </metadata>",
        escape(meta.title),
        timestamp(meta.start_time_ms)
    );
    s.push_str("  <trk>\n");
    let _ = writeln!(s, "    <name>{}</name>", escape(meta.title));
    if let Some(d) = meta.description.filter(|d| !d.trim().is_empty()) {
        let _ = writeln!(s, "    <desc>{}</desc>", escape(d));
    }
    let _ = writeln!(s, "    <type>{}</type>", escape(meta.sport_class));
    for section in sections {
        if !section.iter().any(|p| p.pos.is_some()) {
            continue;
        }
        s.push_str("    <trkseg>\n");
        for p in section {
            let Some((lat, lon)) = p.pos else { continue };
            let _ = write!(s, "      <trkpt lat=\"{}\" lon=\"{}\">", deg(lat), deg(lon));
            if let Some(e) = p.ele_dm {
                let sign = if e < 0 { "-" } else { "" };
                let a = e.unsigned_abs();
                let _ = write!(s, "<ele>{sign}{}.{}</ele>", a / 10, a % 10);
            }
            if let Some(t) = p.t_ms {
                let _ = write!(s, "<time>{}</time>", timestamp(t));
            }
            if let Some(hr) = p.hr {
                let _ = write!(
                    s,
                    "<extensions><gpxtpx:TrackPointExtension><gpxtpx:hr>{hr}</gpxtpx:hr></gpxtpx:TrackPointExtension></extensions>"
                );
            }
            s.push_str("</trkpt>\n");
        }
        s.push_str("    </trkseg>\n");
    }
    s.push_str("  </trk>\n</gpx>\n");
    Some(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pt(t: i64, lat: i32, lon: i32, ele: Option<i32>, hr: Option<u16>) -> Point {
        Point { t_ms: Some(t), pos: Some((lat, lon)), ele_dm: ele, hr }
    }

    #[test]
    fn writes_sections_and_extensions() {
        let meta = Meta { title: "Runde <A&B>", description: Some("schön"), sport_class: "cycling", start_time_ms: 1_700_000_000_000 };
        let sections = vec![
            vec![pt(1_700_000_000_000, 476_123_456, -5_000_001, Some(-15), Some(140)), Point { t_ms: Some(1), ..Default::default() }],
            vec![Point { hr: Some(90), ..Default::default() }],
            vec![pt(1_700_000_001_500, 1, 2, None, None)],
        ];
        let g = write(&meta, &sections).unwrap();
        assert!(g.contains("<name>Runde &lt;A&amp;B&gt;</name>"));
        assert!(g.contains("<desc>schön</desc>"));
        assert!(g.contains("<trkpt lat=\"47.6123456\" lon=\"-0.5000001\"><ele>-1.5</ele><time>2023-11-14T22:13:20.000Z</time>"));
        assert!(g.contains("<gpxtpx:hr>140</gpxtpx:hr>"));
        assert!(g.contains("<trkpt lat=\"0.0000001\" lon=\"0.0000002\"><time>2023-11-14T22:13:21.500Z</time></trkpt>"));
        // Abschnitt ohne Position fällt weg, Punkt ohne Position ebenso
        assert_eq!(g.matches("<trkseg>").count(), 2);
        assert_eq!(g.matches("<trkpt").count(), 2);
    }

    #[test]
    fn none_without_position() {
        let meta = Meta { title: "x", description: None, sport_class: "studio", start_time_ms: 0 };
        assert!(write(&meta, &[vec![Point { hr: Some(80), t_ms: Some(0), ..Default::default() }]]).is_none());
        assert!(write(&meta, &[]).is_none());
    }
}
