# RFC-0022: Binärformat der Messpunkte

Status: Neu, Piggy 011 (Entwurf zur Freigabe). Legt das Format von `track_sections.data` und `segments.reference` fest (RFC-0001).

## 1. Ziel und Umfang

Ein Aufzeichnungsabschnitt wird als ein BLOB gespeichert. Das Format ist kompakt, verlustfrei bezogen auf die unten festgelegte Auflösung, deterministisch (gleiche Punkte ergeben gleiche Bytes) und streng prüfbar. Es dient auch für die Referenzgeometrie eines Segments (Abschnitt 8). Die Formatversion steht in der Datenbank (`format_version`) und im ersten Byte des BLOBs. Version 1 ist hier festgelegt, neue Sensorwerte erfordern eine neue Version.

## 2. Felder und Auflösung

| Feld | Schlüssel | Einheit und Auflösung | Wertebereich |
| :--- | :--- | :--- | :--- |
| Zeit | `t_ms` | Millisekunden seit 1970-01-01 UTC | beliebig, auch rückläufig (siehe 6) |
| Breite | `lat_e7` | Grad mal 10^7, ganzzahlig (ca. 1 cm) | -900.000.000 bis 900.000.000 |
| Länge | `lon_e7` | Grad mal 10^7, ganzzahlig | -1.800.000.000 bis 1.800.000.000 |
| Höhe | `ele_dm` | Dezimeter (0,1 m) | ganzzahlig, vorzeichenbehaftet |
| Herzfrequenz | `hr` | Schläge pro Minute | 1 bis 300 |

Jedes Feld ist je Punkt optional. Breite und Länge sind nur gemeinsam vorhanden (Position). Ein Punkt ohne Position, aber mit Sensorwerten ist zulässig (RFC-0008). Ein Punkt ohne irgendeinen Wert wird nicht gespeichert.

Umrechnung beim Import: Der Messwert wird auf die nächste ganze Einheit gerundet, Hälften weg von null. Ungültige Quellwerte (Herzfrequenz außerhalb 1 bis 300, z. B. 0 oder der FIT-Ungültig-Wert) gelten als fehlend. Die Rückrechnung in Grad ist `lat_e7 / 10^7` in doppelter Genauigkeit. Alle Auswertungen (Matching, Distanz, Höhenhysterese) rechnen mit den gespeicherten, gerundeten Werten.

Ausreißerpunkte, die der Outlier-Filter verwirft (RFC-0008), werden nicht gespeichert.

## 3. Grundbausteine

- **Varint**: LEB128 ohne Vorzeichen, sieben Bit je Byte, niederwertige Gruppe zuerst, höchstes Bit = „weitere Bytes folgen“. Höchstens 10 Bytes, Wert unter 2^64. Die Kodierung muss minimal sein: Ein letztes Byte 0x00 hinter einem Fortsetzungsbyte ist ungültig.
- **Zigzag**: Ein vorzeichenbehafteter Wert `v` wird als `(v << 1) XOR (v >> 63)` abgebildet (0, -1, 1, -2, ... werden zu 0, 1, 2, 3, ...), dann als Varint geschrieben.
- **Delta**: Jeder Strom beginnt mit Vorgänger 0. Der erste Wert ist also der Absolutwert, jeder weitere die Differenz zum vorherigen vorhandenen Wert.

## 4. Aufbau

```text
Byte 0        Formatversion (1)
dann          Punktzahl n (Varint, 1 bis 1.000.000)
dann          Spalte Zeit
dann          Spalte Position
dann          Spalte Höhe
dann          Spalte Herzfrequenz
```

Jede Spalte hat dieselbe Hülle:

```text
Modus (1 Byte)
  0 = kein Punkt hat den Wert        -> nichts folgt
  1 = alle n Punkte haben den Wert   -> Länge, Werte
  2 = nur einige Punkte haben ihn    -> Länge, Bitmap, Werte
Länge (Varint) = Anzahl der folgenden Bytes dieser Spalte (Bitmap eingeschlossen)
Bitmap (nur Modus 2) = ceil(n / 8) Bytes, Punkt i = Bit (i mod 8) von Byte (i div 8), niederwertigstes Bit zuerst
Werte = nur für vorhandene Punkte, in Punktreihenfolge
```

Die Werteströme je Spalte:

| Spalte | Ströme (je Zigzag-Delta-Varint, je vorhandenem Punkt) |
| :--- | :--- |
| Zeit | `t_ms` |
| Position | zuerst alle `lat_e7`, danach alle `lon_e7` |
| Höhe | `ele_dm` |
| Herzfrequenz | `hr` |

## 5. Kanonische Kodierung und strenge Prüfung

Ein Encoder schreibt immer die kanonische Form: Modus 0, wenn kein Punkt den Wert hat, Modus 1, wenn alle ihn haben, sonst Modus 2. Dadurch gibt es zu jeder Punktfolge genau eine Bytefolge, und Gleichheit lässt sich bytegenau prüfen (z. B. Duplikatvergleich, Sicherungsprüfsummen).

Ein Decoder lehnt den Abschnitt mit einem Formatfehler ab bei: unbekannter Version oder unbekanntem Modus, Punktzahl 0 oder über 1.000.000, Abweichung von `track_sections.point_count` bzw. `segments.reference_point_count`, abgeschnittenen Daten, übrigen Bytes am Ende des BLOBs oder einer Spalte, nicht minimalem oder zu langem Varint, Modus 2 mit leerer oder voller Bitmap, gesetzten Bits hinter Punkt n, Koordinaten außerhalb des Bereichs und Herzfrequenz außerhalb von 1 bis 300. Es gibt kein stilles Reparieren. Ein fehlerhafter Abschnitt wird gemeldet, die übrigen Daten der Aktivität bleiben nutzbar.

## 6. Zeit

Die Zeit eines Punkts ist die Gerätezeit (UTC). Zeitwerte müssen nicht aufsteigend sein, weil das Matching rückläufige Zeit als ungültigen Versuch erkennen muss (RFC-0002 Abschnitt 5). Der Encoder sortiert oder ändert nie. Manuelle Korrekturen von Start und Gesamtzeit ändern die gespeicherten Punkte nicht (RFC-0017).

## 7. Abgleich mit der Datenbank

| Spalte | Regel |
| :--- | :--- |
| `track_sections.format_version` | gleich Byte 0 |
| `track_sections.point_count` | gleich n |
| `track_sections.first_time_ms` | Zeit des ersten Punkts mit Zeit, sonst `NULL` |
| `track_sections.last_time_ms` | Zeit des letzten Punkts mit Zeit, sonst `NULL` |

Die Reihenfolge der Punkte innerhalb eines Abschnitts ist die Aufzeichnungsreihenfolge. Mehrere Abschnitte (GPX) werden über `section_index` geordnet und nie verbunden (RFC-0008).

Wirksame Obergrenzen ergeben sich aus dem Import: 100.000 Punkte je Session (`max_points_per_session`, RFC-0021). Die Grenze von 1.000.000 im Decoder schützt nur vor unsinnigen oder manipulierten Daten, etwa bei einem Restore.

## 8. Referenz eines Segments

`segments.reference` nutzt dasselbe Format. Es enthält die Punkte der Referenzstrecke in Aufzeichnungsrichtung, mindestens zwei, alle mit Position. Höhe ist optional. Zeit und Herzfrequenz werden nicht kopiert (Modus 0), die Referenz hat keinen Zeitbezug (ADR-0005). `segments.reference_format_version` und `reference_point_count` gelten wie in Abschnitt 7.

## 9. Größe

Messung mit 100.000 synthetischen Punkten bei 1 Hz, rund 5 m/s und Rauschen auf Höhe und Herzfrequenz: 800.031 Byte, also 8,0 Byte je Punkt. Ohne Höhe und Herzfrequenz 6,0 Byte je Punkt. Eine Datei mit 100.000 Punkten belegt damit etwa 0,8 MB, ohne Kompression. Eine spätere Kompression (z. B. zstd) wäre eine neue Formatversion.

## 10. Testvektor

Drei Punkte, 1 Hz, mit allen Feldern:

| Punkt | `t_ms` | `lat_e7` | `lon_e7` | `ele_dm` | `hr` |
| :--- | :--- | :--- | :--- | :--- | :--- |
| 0 | 1761814800000 | 478520000 | 117350000 | 5800 | 120 |
| 1 | 1761814801000 | 478520450 | 117350300 | 5801 | 121 |
| 2 | 1761814802000 | 478520900 | 117350590 | 5803 | 121 |

Kodierung (45 Byte, hexadezimal):

```text
01 03
01 0a 809ac3c5c666 d00f d00f
01 11 808badc803 8407 8407 e0f9f46f d804 c404
01 04 d05a 02 04
01 04 f001 02 00
```

Lesart: Version 1, 3 Punkte. Zeit: Modus 1, 10 Byte, Absolutwert und zweimal Delta 1000 (Zigzag 2000). Position: Modus 1, 17 Byte, erst die drei Breiten, dann die drei Längen. Höhe: Modus 1, 4 Byte (5800, +1, +2). Herzfrequenz: Modus 1, 4 Byte (120, +1, +0).

Weitere Pflichtfälle für die Tests (RFC-0014): ein Punkt, Mindest- und Höchstkoordinaten, gemischte vorhandene und fehlende Felder, Punktzahlen 7, 8, 9, 15, 16, 17 (Bitmap-Grenzen), rückläufige Zeit, negative Höhe, alle in Abschnitt 5 genannten Fehlerfälle.

## 11. Referenzimplementierung

Python, dient als Testorakel für die Rust-Umsetzung. Sie wurde mit 300 Zufallsfällen, den Randfällen aus Abschnitt 10 und den Fehlerfällen aus Abschnitt 5 geprüft (Roundtrip bytegleich).

```python
"""Referenzimplementierung TrackCraft Track Section Format, Version 1."""
MAX_POINTS = 1_000_000
LAT_MAX, LON_MAX = 900_000_000, 1_800_000_000

class FormatError(Exception): pass

def _rnd(x):  # runden, Hälften weg von null
    import math
    return int(math.floor(abs(x) + 0.5)) * (1 if x >= 0 else -1)

def _put_uvarint(out, v):
    if v < 0: raise ValueError
    while True:
        b = v & 0x7F; v >>= 7
        if v: out.append(b | 0x80)
        else: out.append(b); return

def _zz(v): return (v << 1) ^ (v >> 63) if v >= 0 else ((-v) << 1) - 1
def _unzz(u): return (u >> 1) ^ -(u & 1)

class _Reader:
    def __init__(s, b, pos=0, end=None): s.b, s.pos, s.end = b, pos, len(b) if end is None else end
    def u8(s):
        if s.pos >= s.end: raise FormatError("Ende erreicht")
        v = s.b[s.pos]; s.pos += 1; return v
    def uvarint(s):
        v = shift = n = 0
        while True:
            c = s.u8(); n += 1
            v |= (c & 0x7F) << shift
            if not c & 0x80:
                if n > 1 and c == 0: raise FormatError("nicht minimaler Varint")
                if v >= 1 << 64: raise FormatError("Varint zu gross")
                return v
            shift += 7
            if n >= 10: raise FormatError("Varint zu lang")
    def take(s, n):
        if s.pos + n > s.end: raise FormatError("Ende erreicht")
        r = s.b[s.pos:s.pos+n]; s.pos += n; return r

def _column(points, n, getter, nstreams):
    """liefert Bytes einer Spalte (mode + payload)"""
    present = [getter(p) for p in points]
    idx = [i for i, v in enumerate(present) if v is not None]
    m = len(idx)
    if m == 0: return bytes([0])
    payload = bytearray()
    if m == n: mode = 1
    else:
        mode = 2
        bm = bytearray((n + 7) // 8)
        for i in idx: bm[i >> 3] |= 1 << (i & 7)
        payload += bm
    for s in range(nstreams):
        prev = 0
        for i in idx:
            v = present[i][s] if nstreams > 1 else present[i]
            _put_uvarint(payload, _zz(v - prev)); prev = v
    out = bytearray([mode]); _put_uvarint(out, len(payload)); out += payload
    return bytes(out)

def encode(points):
    """points: Liste von dicts mit optionalen Schlüsseln t_ms, lat_e7, lon_e7, ele_dm, hr"""
    n = len(points)
    if not 0 < n <= MAX_POINTS: raise ValueError("Punktzahl")
    out = bytearray([1]); _put_uvarint(out, n)
    out += _column(points, n, lambda p: p.get("t_ms"), 1)
    out += _column(points, n, lambda p: (p["lat_e7"], p["lon_e7"]) if p.get("lat_e7") is not None else None, 2)
    out += _column(points, n, lambda p: p.get("ele_dm"), 1)
    out += _column(points, n, lambda p: p.get("hr"), 1)
    return bytes(out)

def _read_column(r, n, nstreams):
    mode = r.u8()
    if mode == 0: return [None] * n
    if mode not in (1, 2): raise FormatError("unbekannter Modus")
    plen = r.uvarint(); end = r.pos + plen
    if end > r.end: raise FormatError("Spaltenlänge")
    sub = _Reader(r.b, r.pos, end)
    if mode == 1: idx = list(range(n))
    else:
        bm = sub.take((n + 7) // 8)
        if n % 8 and bm[-1] >> (n % 8): raise FormatError("Bitmap-Überhang")
        idx = [i for i in range(n) if bm[i >> 3] >> (i & 7) & 1]
        if len(idx) in (0, n): raise FormatError("nicht kanonisch")
    streams = []
    for _ in range(nstreams):
        prev = 0; vals = []
        for _ in idx:
            prev += _unzz(sub.uvarint()); vals.append(prev)
        streams.append(vals)
    if sub.pos != end: raise FormatError("Rest in Spalte")
    r.pos = end
    res = [None] * n
    for k, i in enumerate(idx):
        res[i] = tuple(s[k] for s in streams) if nstreams > 1 else streams[0][k]
    return res

def decode(b, expected_count=None):
    r = _Reader(b)
    if r.u8() != 1: raise FormatError("Formatversion")
    n = r.uvarint()
    if not 0 < n <= MAX_POINTS: raise FormatError("Punktzahl")
    if expected_count is not None and n != expected_count: raise FormatError("Punktzahl passt nicht zur Tabelle")
    t = _read_column(r, n, 1); pos = _read_column(r, n, 2)
    ele = _read_column(r, n, 1); hr = _read_column(r, n, 1)
    if r.pos != r.end: raise FormatError("Rest am Ende")
    pts = []
    for i in range(n):
        p = {}
        if t[i] is not None: p["t_ms"] = t[i]
        if pos[i] is not None:
            la, lo = pos[i]
            if abs(la) > LAT_MAX or abs(lo) > LON_MAX: raise FormatError("Koordinate ausserhalb")
            p["lat_e7"], p["lon_e7"] = la, lo
        if ele[i] is not None: p["ele_dm"] = ele[i]
        if hr[i] is not None:
            if not 1 <= hr[i] <= 300: raise FormatError("HF ausserhalb")
            p["hr"] = hr[i]
        pts.append(p)
    return pts
```

## 12. Offene Punkte

1. Erledigt (08.11.2026, M1a): Der Rust-Codec (`backend/src/domain/track.rs`) besteht den Testvektor, die Pflicht- und Fehlerfälle und ist für 300 Zufallsfälle bytegleich mit der Referenzimplementierung oben, die der Test direkt aus diesem Dokument ausführt.
2. Ob die gleichmäßige Ausdünnung nach Index (`max_points`, RFC-0003) für Karte und Höhenprofil ausreicht oder ein geometrieerhaltendes Verfahren nötig wird.
