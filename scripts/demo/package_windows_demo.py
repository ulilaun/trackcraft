"""Windows-Demo-ZIP packen (Issue #41): Programm und Weboberfläche aus dem Windows-ZIP, Demo-Datenbank aus dem
Demo-ZIP, dazu `demo.cmd` und `DEMO.md`.

Aufruf:  python scripts/demo/package_windows_demo.py <trackcraft-…-windows-x86_64.zip> <trackcraft-…-demo.zip> <ziel.zip>

`setup.cmd`, `BETRIEB.md` und die Beispielkonfiguration bleiben draußen: Das Demo-ZIP installiert nichts.
"""

import shutil
import sys
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
FROM_WINDOWS = ("trackcraft.exe", "LICENSE", "THIRD-PARTY-LICENSES.txt")


def main():
    if len(sys.argv) != 4:
        sys.exit(__doc__)
    windows, demo, out = (Path(a).resolve() for a in sys.argv[1:])
    out.parent.mkdir(parents=True, exist_ok=True)
    tmp = out.with_suffix(".tmp")
    with zipfile.ZipFile(tmp, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as z:
        with zipfile.ZipFile(windows) as w:
            # Windows PowerShell 5.1 (lokales package-windows.ps1) schreibt Pfade mit Backslash
            entries = {i.filename.replace("\\", "/"): i for i in w.infolist()}
            names = set(entries)
            for need in FROM_WINDOWS + ("web/index.html",):
                if need not in names:
                    sys.exit(f"{windows.name}: {need} fehlt")
            for n in sorted(names):
                if n in FROM_WINDOWS or (n.startswith("web/") and not n.endswith("/")):
                    info = entries[n]
                    data = w.read(info)
                    info.filename = n
                    z.writestr(info, data)
        with zipfile.ZipFile(demo) as d:
            names = set(d.namelist())
            if "database/trackcraft.db" not in names:
                sys.exit(f"{demo.name}: database/trackcraft.db fehlt")
            for n in sorted(names):
                if not n.endswith("/"):
                    info = d.getinfo(n)
                    info.filename = "demo-daten/" + n
                    z.writestr(info, d.read(n))
        z.write(ROOT / "infrastructure" / "windows" / "demo.cmd", "demo.cmd")
        z.write(ROOT / "infrastructure" / "windows" / "DEMO.md", "DEMO.md")
    shutil.move(tmp, out)
    print(f"{out}: {out.stat().st_size / 1e6:.1f} MB")


if __name__ == "__main__":
    main()
