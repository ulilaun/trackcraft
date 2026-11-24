"""Erfundene Beispielaktivitäten auf Mallorca als GPX erzeugen (für Screenshots und Vorführungen).

Die Strecken in routes.json stammen aus OpenStreetMap-Daten (© OpenStreetMap-Mitwirkende, ODbL), geroutet mit
BRouter. Zeiten, Tempo, Puls und GPS-Rauschen sind erfunden und mit festem Zufallswert reproduzierbar.

Aufruf:  python scripts/demo/make_demo.py <zielordner> [letzter_tag]

Die letzte Aktivität liegt auf `letzter_tag` (JJJJ-MM-TT, Vorgabe gestern), die erste rund 21 Monate davor. Mit
festem `letzter_tag` ist die Ausgabe reproduzierbar.
"""

import json
import math
import random
import sys
from datetime import date, datetime, timedelta, timezone
from pathlib import Path

HERE = Path(__file__).resolve().parent


def last_sunday(year, month):
    d = date(year, month + 1, 1) - timedelta(days=1) if month < 12 else date(year, 12, 31)
    return d - timedelta(days=(d.weekday() + 1) % 7)


def local_to_utc(dt):
    """Mallorca: MEZ, von letzter März- bis letzter Oktobersonntag MESZ (ohne tzdata-Paket)."""
    summer = last_sunday(dt.year, 3) <= dt.date() < last_sunday(dt.year, 10)
    return (dt - timedelta(hours=2 if summer else 1)).replace(tzinfo=timezone.utc)

STEP_S = 2  # Aufzeichnungsintervall

# Strecke -> (Sportart in der GPX, Titel, Grundtempo km/h bei 0 % Steigung)
KINDS = {
    "formentor": ("road", "Cap de Formentor", 29.0),
    "sa_calobra": ("road", "Sa Calobra", 27.0),
    "lluc": ("road", "Pollença – Lluc", 28.0),
    "alcudia": ("road", "Küstenrunde Alcúdia", 32.0),
    "hafenrunde": ("running", "Hafenrunde Port de Pollença", 11.5),
    "boquer": ("running", "Trail Vall de Boquer", 9.5),
}

# Wer auf Mallorca wohnt: das ganze Jahr alle paar Tage, im Sommer weniger Radfahren (Hitze), mehr Laufen
SPAN_DAYS = 630
RIDES = ["formentor", "lluc", "alcudia", "sa_calobra", "formentor", "alcudia", "lluc"]
RUNS = ["hafenrunde", "hafenrunde", "boquer"]


def haversine(a, b):
    r = 6371000.0
    la1, la2 = math.radians(a[1]), math.radians(b[1])
    dla, dlo = la2 - la1, math.radians(b[0] - a[0])
    h = math.sin(dla / 2) ** 2 + math.cos(la1) * math.cos(la2) * math.sin(dlo / 2) ** 2
    return 2 * r * math.asin(math.sqrt(h))


def prepare(coords):
    """Kumulierte Distanz und auf 150 m geglättete Steigung je Stützpunkt."""
    dist = [0.0]
    for a, b in zip(coords, coords[1:]):
        dist.append(dist[-1] + haversine(a, b))
    grades = []
    j0 = j1 = 0
    for i, d in enumerate(dist):
        while dist[j0] < d - 75:
            j0 += 1
        while j1 < len(dist) - 1 and dist[j1] < d + 75:
            j1 += 1
        span = dist[j1] - dist[j0]
        grades.append((coords[j1][2] - coords[j0][2]) / span if span > 20 else 0.0)
    return dist, grades


def at(coords, dist, d, k):
    """Position auf der Strecke bei Distanz d (k: Suchindex, wird fortgeschrieben)."""
    while k < len(dist) - 2 and dist[k + 1] < d:
        k += 1
    seg = dist[k + 1] - dist[k] or 1.0
    t = min(max((d - dist[k]) / seg, 0.0), 1.0)
    a, b = coords[k], coords[k + 1]
    return [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t], k


def speed_kmh(sport, v0, grade):
    if sport == "running":
        v = v0 / (1 + 9 * max(grade, 0)) if grade >= 0 else v0 * (1 + min(-grade, 0.08) * 2)
        return max(v, 5.0)
    if grade >= 0:
        return max(v0 / (1 + 14 * grade), 9.0)
    return min(v0 * (1 + min(-grade, 0.1) * 6), 62.0)


def activity(rng, name, coords, start_local, form):
    sport, title, base = KINDS[name]
    dist, grades = prepare(coords)
    total = dist[-1]
    v0 = base * form * rng.uniform(0.96, 1.04)
    vf = 1.0
    hr_rest = 62
    d = 0.0
    k = 0
    t = 0
    hr = 95.0
    drift = [0.0, 0.0]
    pts = []
    while d < total:
        p, k = at(coords, dist, d, k)
        g = grades[k]
        vf = min(max(vf + rng.gauss(0, 0.004), 0.95), 1.05)
        v = speed_kmh(sport, v0, g) * vf
        effort = 0.55 + 4.5 * max(g, 0) + (0.12 if sport == "running" else 0.0) + (vf - 1) * 0.5
        target = hr_rest + 105 * min(max(effort, 0.35), 1.0)
        hr += (target - hr) * (STEP_S / 45)
        drift = [drift[0] * 0.9 + rng.gauss(0, 0.6), drift[1] * 0.9 + rng.gauss(0, 0.6)]
        lat = p[1] + drift[1] / 111320
        lon = p[0] + drift[0] / (111320 * math.cos(math.radians(p[1])))
        ele = p[2] + rng.gauss(0, 0.4)
        when = start_local + timedelta(seconds=t)
        pts.append((lat, lon, ele, local_to_utc(when), int(round(hr + rng.gauss(0, 0.6)))))
        d += v / 3.6 * STEP_S
        t += STEP_S
    return sport, title, pts


def gpx(sport, title, pts):
    rows = []
    for lat, lon, ele, when, hr in pts:
        ts = when.strftime("%Y-%m-%dT%H:%M:%SZ")
        rows.append(
            f'<trkpt lat="{lat:.6f}" lon="{lon:.6f}"><ele>{ele:.1f}</ele><time>{ts}</time>'
            f"<extensions><gpxtpx:TrackPointExtension><gpxtpx:hr>{hr}</gpxtpx:hr></gpxtpx:TrackPointExtension></extensions></trkpt>"
        )
    return (
        '<?xml version="1.0" encoding="UTF-8"?>\n'
        '<gpx version="1.1" creator="TrackCraft-Demo" xmlns="http://www.topografix.com/GPX/1/1" '
        'xmlns:gpxtpx="http://www.garmin.com/xmlschemas/TrackPointExtension/v1">\n'
        f"<trk><name>{title}</name><type>{sport}</type><trkseg>\n" + "\n".join(rows) + "\n</trkseg></trk></gpx>\n"
    )


def schedule(rng, last_day):
    """(Datum, Strecke) für alle Aktivitäten: alle zwei bis fünf Tage, Juni bis September öfter Laufen. Rückwärts vom
    letzten Tag aus, damit die letzte Aktivität genau dort liegt."""
    days = []
    day, first = last_day, last_day - timedelta(days=SPAN_DAYS)
    while day >= first:
        days.append(day)
        day -= timedelta(days=rng.choice([2, 3, 3, 4, 5]))
    plan = []
    rides = runs = 0
    for day in reversed(days):
        summer = 6 <= day.month <= 9
        if rng.random() < (0.55 if summer else 0.3):
            plan.append((day, RUNS[runs % len(RUNS)]))
            runs += 1
        else:
            plan.append((day, RIDES[rides % len(RIDES)]))
            rides += 1
    return plan


def main():
    if len(sys.argv) not in (2, 3):
        sys.exit(__doc__)
    out = Path(sys.argv[1])
    last_day = date.fromisoformat(sys.argv[2]) if len(sys.argv) == 3 else date.today() - timedelta(days=1)
    out.mkdir(parents=True, exist_ok=True)
    routes = json.loads((HERE / "routes.json").read_text(encoding="utf-8"))
    rng = random.Random(20260315)
    plan = schedule(rng, last_day)
    for n, (day, name) in enumerate(plan):
        form = 0.92 + 0.12 * n / max(len(plan) - 1, 1) + rng.uniform(-0.03, 0.03)
        hour = 8 if KINDS[name][0] == "road" else rng.choice([7, 18])
        start = datetime(day.year, day.month, day.day, hour, rng.randrange(0, 50, 5))
        sport, title, pts = activity(rng, name, routes[name], start, form)
        path = out / f"{day.isoformat()}-{name}.gpx"
        path.write_text(gpx(sport, title, pts), encoding="utf-8")
        km = sum(haversine((a[1], a[0]), (b[1], b[0])) for a, b in zip(pts, pts[1:])) / 1000
        print(f"{path.name}: {km:.1f} km, {len(pts) * STEP_S / 60:.0f} min")


if __name__ == "__main__":
    main()
