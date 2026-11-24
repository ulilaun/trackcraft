"""Demo-Datenbank bauen (Release, Issue #41): Beispieldaten erzeugen, in eine frische Instanz importieren, kennzeichnen
und als ZIP packen.

Aufruf:  python scripts/demo/build_demo_db.py <trackcraft-programm> <ziel.zip>

Das ZIP enthält `database/trackcraft.db` (Demo-Kennzeichen gesetzt, ohne WAL-Datei) und `activities/` (Originaldateien).
Entpackt in einen leeren Datenordner und mit `[demo] enabled = true` gestartet, verschiebt TrackCraft die Daten so,
dass die letzte Aktivität gestern war. Dauer etwa 15 Minuten (Segmentauswertung).

Die Beispieldaten enden immer gestern: Ziel und Planungshilfe von `seed_demo.py` rechnen vom heutigen Tag aus, und nur so
passen Plan und Aktivitäten nach der Verschiebung zusammen.
"""

import os
import shutil
import ssl
import subprocess
import sys
import tempfile
import time
import urllib.request
import zipfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
PORT = 18443
BASE = f"https://localhost:{PORT}"
CTX = ssl._create_unverified_context()


def toml_path(p):
    return "'" + str(p) + "'"


def wait_ready(proc, timeout_s=120):
    deadline = time.time() + timeout_s
    while time.time() < deadline:
        if proc.poll() is not None:
            sys.exit(f"TrackCraft ist beim Start beendet (Code {proc.returncode})")
        try:
            with urllib.request.urlopen(BASE + "/api/v1/health", timeout=5, context=CTX) as r:
                if r.status == 200:
                    return
        except OSError:
            pass
        time.sleep(1)
    sys.exit("TrackCraft wird nicht bereit")


def run(cmd, **kw):
    print("+", " ".join(str(c) for c in cmd), flush=True)
    subprocess.run(cmd, check=True, **kw)


def main():
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    exe = Path(sys.argv[1]).resolve()
    out = Path(sys.argv[2]).resolve()

    with tempfile.TemporaryDirectory(prefix="trackcraft-demo-") as tmp:
        root = Path(tmp)
        data = root / "data"
        cert, key = root / "server.pem", root / "server-key.pem"
        run(["openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-keyout", key, "-out", cert, "-days", "2",
             "-subj", "/CN=localhost", "-addext", "subjectAltName=DNS:localhost"], stderr=subprocess.DEVNULL)
        config = root / "trackcraft.toml"
        config.write_text(
            f"""[server]
listen = "127.0.0.1:{PORT}"
allowed_origins = ["{BASE}"]

[tls]
cert_file = {toml_path(cert)}
key_file  = {toml_path(key)}

[paths]
database_dir   = {toml_path(data / "database")}
activities_dir = {toml_path(data / "activities")}
logs_dir       = {toml_path(data / "logs")}

[backup]
target_dir    = {toml_path(root / "backup")}
migration_dir = {toml_path(data / "backups")}
""",
            encoding="utf-8",
        )
        env = dict(os.environ, TRACKCRAFT_CONFIG=str(config))

        gpx = root / "gpx"
        run([sys.executable, HERE / "make_demo.py", gpx], stdout=subprocess.DEVNULL)

        proc = subprocess.Popen([exe], env=env)
        try:
            wait_ready(proc)
            run([sys.executable, HERE / "seed_demo.py", gpx, BASE])
        finally:
            proc.terminate()
            try:
                proc.wait(timeout=60)
            except subprocess.TimeoutExpired:
                proc.kill()
                proc.wait()

        run([exe, "demo", "mark"], env=env)

        db = data / "database" / "trackcraft.db"
        for suffix in ("-wal", "-shm"):
            extra = db.with_name(db.name + suffix)
            if extra.exists() and extra.stat().st_size > 0:
                sys.exit(f"{extra.name} ist nicht leer; die Datenbank wurde nicht vollständig geschrieben")

        out.parent.mkdir(parents=True, exist_ok=True)
        tmp_zip = out.with_suffix(".tmp")
        with zipfile.ZipFile(tmp_zip, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as z:
            z.write(db, "database/trackcraft.db")
            for f in sorted((data / "activities").rglob("*")):
                if f.is_file():
                    z.write(f, "activities/" + f.relative_to(data / "activities").as_posix())
        shutil.move(tmp_zip, out)
        print(f"{out}: {out.stat().st_size / 1e6:.1f} MB")


if __name__ == "__main__":
    main()
