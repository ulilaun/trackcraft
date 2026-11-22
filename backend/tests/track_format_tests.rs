//! Messpunktformat Version 1 (RFC-0022): Testvektor, Pflichtfälle aus Abschnitt 10, Fehlerfälle aus Abschnitt 5
//! und Abgleich mit der Python-Referenzimplementierung aus dem RFC als Orakel.

use trackcraft::domain::track::{self, FormatError, Point};

fn p(t: Option<i64>, pos: Option<(i32, i32)>, ele: Option<i32>, hr: Option<u16>) -> Point {
    Point { t_ms: t, pos, ele_dm: ele, hr }
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn unhex(s: &str) -> Vec<u8> {
    let s: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
}

fn vector() -> Vec<Point> {
    vec![
        p(Some(1761814800000), Some((478520000, 117350000)), Some(5800), Some(120)),
        p(Some(1761814801000), Some((478520450, 117350300)), Some(5801), Some(121)),
        p(Some(1761814802000), Some((478520900, 117350590)), Some(5803), Some(121)),
    ]
}

const VECTOR_HEX: &str =
    "01 03 01 0a 809ac3c5c666 d00f d00f 01 11 808badc803 8407 8407 e0f9f46f d804 c404 01 04 d05a 02 04 01 04 f001 02 00";

#[test]
fn test_vector_encodes_to_45_bytes() {
    let b = track::encode(&vector()).unwrap();
    assert_eq!(b.len(), 45);
    assert_eq!(hex(&b), hex(&unhex(VECTOR_HEX)));
    assert_eq!(track::decode(&b, Some(3)).unwrap(), vector());
}

fn roundtrip(points: &[Point]) {
    let b = track::encode(points).unwrap();
    assert_eq!(track::decode(&b, Some(points.len())).unwrap(), points, "{}", hex(&b));
    // kanonisch: erneutes Kodieren ergibt dieselben Bytes
    assert_eq!(track::encode(&track::decode(&b, None).unwrap()).unwrap(), b);
}

#[test]
fn mandatory_cases_roundtrip() {
    // ein Punkt
    roundtrip(&[p(Some(0), None, None, None)]);
    roundtrip(&[p(None, None, Some(-123), None)]);
    // Mindest- und Höchstkoordinaten
    roundtrip(&[
        p(None, Some((-900_000_000, -1_800_000_000)), None, None),
        p(None, Some((900_000_000, 1_800_000_000)), None, None),
        p(None, Some((0, 0)), None, None),
    ]);
    // HF-Grenzen, negative Höhe, rückläufige Zeit
    roundtrip(&[
        p(Some(10_000), None, Some(-4300), Some(1)),
        p(Some(9_000), None, Some(-4301), Some(300)),
        p(Some(9_000), None, Some(i32::MIN), Some(150)),
        p(Some(i64::MIN), None, Some(i32::MAX), Some(150)),
        p(Some(i64::MAX), None, Some(0), Some(150)),
    ]);
    // gemischte Felder und Bitmap-Grenzen
    for n in [7usize, 8, 9, 15, 16, 17] {
        let pts: Vec<Point> = (0..n)
            .map(|i| {
                let t = (i % 2 == 0).then_some(1_700_000_000_000 + i as i64 * 1000);
                let pos = (i % 3 != 0).then_some((478_520_000 + i as i32 * 37, 117_350_000 - i as i32 * 11));
                let ele = (i % 4 != 1).then_some(5800 - i as i32);
                let hr = (i == n - 1).then_some(130);
                let pt = p(t, pos, ele, hr);
                if pt.is_empty() { p(Some(i as i64), None, None, None) } else { pt }
            })
            .collect();
        roundtrip(&pts);
    }
}

#[test]
fn encoder_rejects_invalid_input() {
    assert!(track::encode(&[]).is_err());
    assert!(track::encode(&[Point::default()]).is_err());
    assert!(track::encode(&[p(None, Some((900_000_001, 0)), None, None)]).is_err());
    assert!(track::encode(&[p(None, Some((0, -1_800_000_001)), None, None)]).is_err());
    assert!(track::encode(&[p(None, None, None, Some(0))]).is_err());
    assert!(track::encode(&[p(None, None, None, Some(301))]).is_err());
}

#[test]
fn decoder_rejects_all_error_cases() {
    let good = unhex(VECTOR_HEX);
    let cases: Vec<(&str, Vec<u8>, FormatError)> = vec![
        (
            "Version",
            {
                let mut b = good.clone();
                b[0] = 2;
                b
            },
            FormatError::Version(2),
        ),
        ("Punktzahl 0", vec![1, 0], FormatError::Count(0)),
        ("Punktzahl zu groß", vec![1, 0xC1, 0x84, 0x3D, 0, 0, 0, 0], FormatError::Count(1_000_001)),
        ("unbekannter Modus", vec![1, 1, 3], FormatError::Mode(3)),
        ("abgeschnitten", good[..good.len() - 1].to_vec(), FormatError::ColumnLength),
        (
            "Rest am Ende",
            {
                let mut b = good.clone();
                b.push(0);
                b
            },
            FormatError::Trailing,
        ),
        ("nicht minimaler Varint (Punktzahl)", vec![1, 0x81, 0x00, 1, 1, 0], FormatError::NonMinimalVarint),
        ("Varint zu lang", vec![1, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x01], FormatError::VarintTooLong),
        ("Varint über 2^64", vec![1, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x02], FormatError::VarintTooLong),
        // Zeit Modus 1 mit Länge 2, aber nur ein Wert -> Rest in der Spalte
        ("Rest in Spalte", vec![1, 1, 1, 2, 0, 0, 0, 0, 0], FormatError::ColumnTrailing),
        // Modus 2 mit voller Bitmap (1 Punkt): nicht kanonisch
        ("volle Bitmap", vec![1, 1, 2, 2, 0b1, 0, 0, 0, 0], FormatError::NonCanonicalBitmap),
        ("leere Bitmap", vec![1, 2, 2, 1, 0b00, 0, 0, 0], FormatError::NonCanonicalBitmap),
        ("Bitmap-Überhang", vec![1, 2, 2, 2, 0b101, 0, 0, 0, 0], FormatError::BitmapOverhang),
        ("Koordinate", { track::encode(&[p(None, Some((900_000_000, 0)), None, None)]).unwrap() }, FormatError::Coordinate),
        // HF 0 per Hand kodiert: Spalten 0,0,0, HF Modus 1 Länge 1 Wert 0
        ("HF 0", vec![1, 1, 0, 0, 0, 1, 1, 0], FormatError::HeartRate),
        ("HF 301", vec![1, 1, 0, 0, 0, 1, 2, 0xDA, 0x04], FormatError::HeartRate),
    ];
    for (name, bytes, expected) in cases {
        // Für „Koordinate“ wird die Breite auf 900.000.001 erhöht.
        let bytes = if name == "Koordinate" {
            // Position Modus 1, Länge 6: zigzag(900000000)=1800000000 -> 5 Byte, Länge 0 -> 1 Byte
            let mut b = bytes;
            let i = b.iter().position(|&x| x == 0x80).unwrap(); // erstes Byte des Breitenwerts
            b[i] = 0x82; // +2 im Zigzag-Wert = +1 im Wert
            b
        } else {
            bytes
        };
        assert_eq!(track::decode(&bytes, None), Err(expected), "{name}: {}", hex(&bytes));
    }
    assert_eq!(track::decode(&good, Some(4)), Err(FormatError::CountMismatch { expected: 4, found: 3 }));
    assert_eq!(track::decode(&[], None), Err(FormatError::Truncated));
}

/// Einfacher deterministischer Zufallsgenerator (xorshift), damit die Fälle reproduzierbar sind.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn range(&mut self, lo: i64, hi: i64) -> i64 {
        lo + (self.next() % ((hi - lo + 1) as u64)) as i64
    }
    fn chance(&mut self, pct: u64) -> bool {
        self.next() % 100 < pct
    }
}

fn random_case(rng: &mut Rng) -> Vec<Point> {
    let n = rng.range(1, 40) as usize;
    let (pt, pp, pe, ph) = (rng.range(0, 100) as u64, rng.range(0, 100) as u64, rng.range(0, 100) as u64, rng.range(0, 100) as u64);
    let mut t = rng.range(1_500_000_000_000, 1_800_000_000_000);
    let mut la = rng.range(-900_000_000, 900_000_000);
    let mut lo = rng.range(-1_800_000_000, 1_800_000_000);
    (0..n)
        .map(|i| {
            t += rng.range(-3000, 5000);
            la = (la + rng.range(-5000, 5000)).clamp(-900_000_000, 900_000_000);
            lo = (lo + rng.range(-5000, 5000)).clamp(-1_800_000_000, 1_800_000_000);
            let pt = p(
                rng.chance(pt).then_some(t),
                rng.chance(pp).then_some((la as i32, lo as i32)),
                rng.chance(pe).then_some(rng.range(-5000, 90000) as i32),
                rng.chance(ph).then_some(rng.range(1, 300) as u16),
            );
            if pt.is_empty() { p(Some(i as i64), None, None, None) } else { pt }
        })
        .collect()
}

#[test]
fn random_roundtrip() {
    let mut rng = Rng(0x5eed_cafe_f00d);
    for _ in 0..500 {
        roundtrip(&random_case(&mut rng));
    }
}

/// Abgleich mit der Python-Referenz aus RFC-0022 Abschnitt 11 (wird übersprungen, wenn kein Python da ist).
#[test]
fn matches_python_reference() {
    let rfc = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../docs/rfc/RFC-0022-track-point-binary-format.md")).unwrap();
    let start = rfc.find("```python\n").expect("Referenzcode") + "```python\n".len();
    let code = &rfc[start..start + rfc[start..].find("```").unwrap()];

    let mut rng = Rng(0x0dd_ba11);
    let cases: Vec<Vec<Point>> = (0..300).map(|_| random_case(&mut rng)).collect();
    let mut lines = String::new();
    for case in &cases {
        let pts: Vec<String> = case
            .iter()
            .map(|pt| {
                let mut f = Vec::new();
                if let Some(t) = pt.t_ms {
                    f.push(format!("\"t_ms\":{t}"));
                }
                if let Some((a, b)) = pt.pos {
                    f.push(format!("\"lat_e7\":{a},\"lon_e7\":{b}"));
                }
                if let Some(e) = pt.ele_dm {
                    f.push(format!("\"ele_dm\":{e}"));
                }
                if let Some(h) = pt.hr {
                    f.push(format!("\"hr\":{h}"));
                }
                format!("{{{}}}", f.join(","))
            })
            .collect();
        lines.push_str(&format!("[{}]\n", pts.join(",")));
    }

    let dir = tempfile::tempdir().unwrap();
    let script = format!("{code}\nimport sys, json\nfor line in open(sys.argv[1]):\n    print(encode(json.loads(line)).hex())\n");
    std::fs::write(dir.path().join("ref.py"), script).unwrap();
    std::fs::write(dir.path().join("cases.jsonl"), &lines).unwrap();
    let out = ["python3", "python"].iter().find_map(|py| {
        std::process::Command::new(py)
            .arg("-I")
            .arg(dir.path().join("ref.py"))
            .arg(dir.path().join("cases.jsonl"))
            .output()
            .ok()
            .filter(|o| o.status.success())
    });
    let Some(out) = out else {
        eprintln!("Python nicht verfügbar, Abgleich mit der Referenz übersprungen");
        return;
    };
    let expected: Vec<String> = String::from_utf8(out.stdout).unwrap().lines().map(str::to_string).collect();
    assert_eq!(expected.len(), cases.len());
    for (i, case) in cases.iter().enumerate() {
        assert_eq!(hex(&track::encode(case).unwrap()), expected[i], "Fall {i}");
    }
}

#[test]
fn rounding_is_half_away_from_zero() {
    assert_eq!(track::round_half_away(2.5), 3);
    assert_eq!(track::round_half_away(-2.5), -3);
    assert_eq!(track::round_half_away(2.4999), 2);
    assert_eq!(track::deg_to_e7(47.85200005), 478520001);
    assert_eq!(track::deg_to_e7(-11.73500005), -117350001);
}
