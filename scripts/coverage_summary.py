"""Coverage je Bereich nach RFC-0014 aus einer lcov-Datei des Backends.

Aufruf: python scripts/coverage_summary.py backend/lcov.info [--gate]
Ausgabe: Markdown-Tabelle (für die Zusammenfassung des CI-Jobs). Mit --gate Exit-Code 1 bei verfehltem Ziel (ab M6).
Werte werden auf eine Nachkommastelle abgeschnitten (89,96 % zählt als 89,9 %).
"""

import math
import sys
from pathlib import Path

AREAS = [
    ("Domain", ("domain", "geo", "segment"), 90.0),
    ("Services", ("services", "import", "jobs"), 80.0),
]
TOTAL_TARGET = 75.0
FIXED_EXCLUDES = ("src/main.rs", "src/platform/")


def module_of(rel: str) -> str:
    parts = rel.split("/")
    if len(parts) >= 2 and parts[0] == "src":
        return parts[1].removesuffix(".rs")
    return ""


def load_exclusions(backend: Path) -> list[str]:
    path = backend / "coverage-exclusions.txt"
    if not path.exists():
        return []
    result = []
    for line in path.read_text(encoding="utf-8").splitlines():
        entry = line.split("#", 1)[0].strip()
        if entry:
            result.append(entry.replace("\\", "/"))
    return result


def parse(lcov: Path, backend: Path) -> dict[str, tuple[int, int]]:
    files: dict[str, tuple[int, int]] = {}
    current, found, hit = None, 0, 0
    root = str(backend.resolve()).replace("\\", "/").rstrip("/") + "/"
    for line in lcov.read_text(encoding="utf-8").splitlines():
        if line.startswith("SF:"):
            path = line[3:].replace("\\", "/")
            current = path[len(root):] if path.startswith(root) else path
            found = hit = 0
        elif line.startswith("LF:"):
            found = int(line[3:])
        elif line.startswith("LH:"):
            hit = int(line[3:])
        elif line == "end_of_record" and current:
            files[current] = (found, hit)
            current = None
    return files


def pct(found: int, hit: int) -> float | None:
    if found == 0:
        return None
    return math.floor(hit * 1000 / found) / 10


def main() -> int:
    lcov = Path(sys.argv[1])
    gate = "--gate" in sys.argv
    backend = Path(__file__).resolve().parent.parent / "backend"
    excludes = list(FIXED_EXCLUDES) + load_exclusions(backend)
    files = {
        f: v for f, v in parse(lcov, backend).items()
        if f.startswith("src/") and not any(f == e or f.startswith(e) for e in excludes)
    }

    rows, failed = [], False
    for name, modules, target in AREAS:
        sel = [v for f, v in files.items() if module_of(f) in modules]
        found, hit = sum(v[0] for v in sel), sum(v[1] for v in sel)
        rows.append((name, ", ".join(modules), found, hit, target))
    found, hit = sum(v[0] for v in files.values()), sum(v[1] for v in files.values())
    rows.append(("Gesamt", "Backend ohne Ausnahmen", found, hit, TOTAL_TARGET))

    print("| Bereich | Module | Zeilen | Abdeckung | Ziel | Stand |")
    print("| :--- | :--- | ---: | ---: | ---: | :--- |")
    for name, mods, f, h, target in rows:
        p = pct(f, h)
        if p is None:
            state, shown = "noch kein Code", "–"
        else:
            ok = p >= target
            failed |= not ok
            state, shown = ("erreicht" if ok else "verfehlt"), f"{p:.1f} %".replace(".", ",")
        print(f"| {name} | {mods} | {h}/{f} | {shown} | {target:.0f} % | {state} |")
    if not gate:
        print("\nBis M5 nur Bericht, Gate ab M6 (RFC-0014).")
    return 1 if gate and failed else 0


if __name__ == "__main__":
    sys.exit(main())
