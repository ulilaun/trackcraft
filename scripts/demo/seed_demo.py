"""Eine leere TrackCraft-Instanz mit den Beispieldaten aus make_demo.py befüllen.

Importiert alle GPX-Dateien, legt zwei Saisons, Segmente und Strecken, Profil mit HF-Zonen, ein Ziel und einen Plan an
und wartet, bis die Auswertung fertig ist.
Nur für eine Test-Instanz gedacht (selbst signiertes Zertifikat wird nicht geprüft).

Aufruf:  python scripts/demo/seed_demo.py <gpx-ordner> [https://localhost:8443]
"""

import json
import ssl
import sys
import time
import urllib.error
import urllib.request
import uuid
from datetime import date, timedelta
from pathlib import Path

BASE = sys.argv[2] if len(sys.argv) > 2 else "https://localhost:8443"
CTX = ssl._create_unverified_context()
HEADERS = {"Origin": BASE, "Accept": "application/json"}


def call(method, path, body=None, raw=None, content_type="application/json"):
    data = raw if raw is not None else (json.dumps(body).encode() if body is not None else None)
    req = urllib.request.Request(BASE + "/api/v1" + path, data=data, method=method, headers=dict(HEADERS))
    if data is not None:
        req.add_header("Content-Type", content_type)
    try:
        with urllib.request.urlopen(req, context=CTX, timeout=600) as r:
            text = r.read().decode()
            return r.status, json.loads(text) if text else None
    except urllib.error.HTTPError as e:
        return e.code, json.loads(e.read().decode() or "null")


def upload(path):
    boundary = uuid.uuid4().hex
    head = f'--{boundary}\r\nContent-Disposition: form-data; name="file"; filename="{path.name}"\r\nContent-Type: application/gpx+xml\r\n\r\n'
    raw = head.encode() + path.read_bytes() + f"\r\n--{boundary}--\r\n".encode()
    s, v = call("POST", "/activities/import", raw=raw, content_type=f"multipart/form-data; boundary={boundary}")
    if s != 201:
        sys.exit(f"Upload {path.name}: {s} {v}")
    job = v["data"]
    sessions = [{"session_index": i, "action": "import"} for i, _ in enumerate(job.get("sessions") or [0])]
    s, v = call("POST", f"/import/jobs/{job['id']}/confirm", {"state_token": job["state_token"], "sessions": sessions})
    if s != 202:
        sys.exit(f"Bestätigen {path.name}: {s} {v}")
    return v["data"]["imported_activity_ids"][0]


def wait_jobs():
    for _ in range(600):
        s, v = call("GET", "/jobs")
        busy = [j for j in v.get("data", []) if j.get("status") in ("queued", "running")]
        if not busy:
            return
        time.sleep(2)
    sys.exit("Auswertung wird nicht fertig")


def track(activity_id):
    s, v = call("GET", f"/activities/{activity_id}/track?max_points=100000")
    sec = v["data"]["sections"][0] if "sections" in v["data"] else v["data"][0]
    return sec


def climb(sec, lo_frac, hi_frac, before_m):
    """Anstieg: höchster Punkt im Bereich [lo_frac, hi_frac] der Strecke, Start am tiefsten Punkt davor."""
    n = len(sec["index"])
    lo, hi = int(n * lo_frac), int(n * hi_frac)
    top = max(range(lo, hi), key=lambda i: sec["ele_m"][i] or 0)
    d_top = sec["dist_m"][top]
    cand = [i for i in range(0, top) if sec["dist_m"][i] >= d_top - before_m]
    start = min(cand, key=lambda i: sec["ele_m"][i] or 0)
    return sec["index"][start], sec["index"][top]


def climb_from_low(sec, lo_frac, hi_frac, after_m):
    """Anstieg ab dem tiefsten Punkt im Bereich [lo_frac, hi_frac] bis zum höchsten Punkt danach (z. B. ab dem Hafen)."""
    n = len(sec["index"])
    low = min(range(int(n * lo_frac), int(n * hi_frac)), key=lambda i: sec["ele_m"][i] or 0)
    d_low = sec["dist_m"][low]
    cand = [i for i in range(low, n) if sec["dist_m"][i] <= d_low + after_m]
    top = max(cand, key=lambda i: sec["ele_m"][i] or 0)
    return sec["index"][low], sec["index"][top]


def by_distance(sec, from_m, to_m):
    pick = lambda m: min(range(len(sec["dist_m"])), key=lambda i: abs((sec["dist_m"][i] or 0) - m))
    return sec["index"][pick(from_m)], sec["index"][pick(to_m)]


def create(activity_id, rng, name, sport, kind="segment", location=None):
    body = {"activity_id": activity_id, "section_index": 0, "start_index": rng[0], "end_index": rng[1],
            "name": name, "sport_class": sport, "kind": kind}
    if location:
        body["location"] = location
    s, v = call("POST", "/segments/from-activity", body)
    print(f"{kind} {name}: {s}" + ("" if s in (200, 201, 202) else f" {v}"))


def main():
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    files = sorted(Path(sys.argv[1]).glob("*.gpx"))
    ids, days = {}, {}
    for f in files:
        aid = upload(f)
        ids.setdefault(f.stem.split("-", 3)[3], []).append(aid)
        days[f.stem[:10]] = aid
        print("importiert", f.name)
    wait_jobs()

    for name, start, end in [("Saison 2025", "2025-03-01", "2025-10-31"), ("Saison 2026", "2026-03-01", "2026-10-31")]:
        s, v = call("POST", "/seasons", {"name": name, "start_date": start, "end_date": end})
        print("Saison", name, s)

    first = {k: v[0] for k, v in ids.items()}
    sec = track(first["sa_calobra"])
    create(first["sa_calobra"], climb_from_low(sec, 0.4, 0.6, 11000), "Sa Calobra", "cycling", location="Escorca")
    sec = track(first["formentor"])
    create(first["formentor"], climb(sec, 0.0, 0.3, 6000), "Mirador Es Colomer", "cycling", location="Pollença")
    sec = track(first["lluc"])
    create(first["lluc"], climb(sec, 0.15, 0.5, 14000), "Coll de Femenies", "cycling", location="Escorca")
    sec = track(first["alcudia"])
    create(first["alcudia"], by_distance(sec, 9000, 14000), "Platja de Muro flach", "cycling", location="Muro")
    sec = track(first["boquer"])
    create(first["boquer"], climb(sec, 0.0, 0.5, 4000), "Boquer-Anstieg", "running", location="Pollença")
    sec = track(first["hafenrunde"])
    create(first["hafenrunde"], (sec["index"][0], sec["index"][-1]), "Hafenrunde", "running", kind="route", location="Port de Pollença")
    sec = track(first["formentor"])
    create(first["formentor"], (sec["index"][0], sec["index"][-1]), "Cap de Formentor", "cycling", kind="route", location="Port de Pollença")
    wait_jobs()
    seed_form_and_plan(days)
    wait_jobs()
    print("fertig:", sum(len(v) for v in ids.values()), "Aktivitäten")


def seed_form_and_plan(days):
    """Profil und HF-Zonen (für Belastung und Form), ein Ziel und ein Plan mit erledigten und offenen Einheiten."""
    s, _ = call("PATCH", "/profile", {"nickname": "Alex", "birthday": "1984-06-12"})
    print("Profil", s)
    s, v = call("POST", "/hr-zones", {"sport_class": None, "valid_from": "2025-01-01", "max_hr": 182, "resting_hr": 52})
    print("HF-Zonen", s, "" if s == 201 else v)

    today = date.today()
    s, v = call("POST", "/plan/goals", {
        "date": (today + timedelta(days=48)).isoformat(), "sport_class": "cycling", "name": "Tramuntana-Runde",
        "description": "Erfundenes Beispielziel: einmal quer durchs Gebirge.", "distance_m": 160000, "target_time_s": 6 * 3600 + 30 * 60})
    print("Ziel", s, "" if s == 201 else v)
    goal = v["data"]["id"]

    # Erledigte Einheiten der letzten zehn Tage, je mit der Aktivität des Tages verknüpft
    for day, aid in sorted(days.items()):
        if (today - date.fromisoformat(day)).days > 10:
            continue
        a = call("GET", f"/activities/{aid}")[1]["data"]
        minutes = max(a["total_time_ms"] // 60000, 1)
        s, u = call("POST", "/plan/units", {"date": day, "sport_class": a["sport_class"], "title": a["title"],
                                            "mode": "target", "duration_min": minutes, "target_load": round(minutes * 0.9),
                                            "intensive_pct": 20})
        if s == 201:
            call("PUT", f"/plan/units/{u['data']['id']}/activity", {"activity_id": a["id"]})
        print("Einheit erledigt", day, s)

    # Offene Einheiten bis zum Ziel über die Planungshilfe
    body = {"goal_id": goal, "sport_class": "cycling", "minutes": [0, 75, 60, 0, 90, 180, 60], "max_gain": 5,
            "recovery_weeks": True, "intensity": "polarized"}
    s, v = call("POST", "/plan/suggestion/apply", body)
    print("Planungshilfe", s, v["data"] if s == 201 else v)


if __name__ == "__main__":
    main()
