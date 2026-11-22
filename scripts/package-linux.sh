#!/usr/bin/env bash
# Baut das Linux-Paket nach RFC-0005 „Linux“/RFC-0015 aus vorhandenen Build-Ausgaben.
# Aufruf (Repository-Wurzel): scripts/package-linux.sh <version> [outdir] [lizenzdatei]
# Erwartet: backend/target/release/trackcraft (Linux x86-64), frontend/dist, Lizenzliste.
set -euo pipefail
version="${1:?Version fehlt}"
out="${2:-dist}"
licenses="${3:-THIRD-PARTY-LICENSES.txt}"
cd "$(dirname "$0")/.."

name="trackcraft-${version}-linux-x86_64"
stage="${out}/${name}"
for f in backend/target/release/trackcraft frontend/dist/index.html backend/config/trackcraft.linux.example.toml \
    infrastructure/linux/BETRIEB.md infrastructure/linux/setup.sh LICENSE "$licenses"; do
    [ -e "$f" ] || { echo "Fehlt: $f" >&2; exit 1; }
done
rm -rf "$stage"
mkdir -p "$stage/web"
install -m 0755 backend/target/release/trackcraft "$stage/trackcraft"
cp -r frontend/dist/. "$stage/web/"
install -m 0644 backend/config/trackcraft.linux.example.toml "$stage/trackcraft.example.toml"
install -m 0644 infrastructure/linux/BETRIEB.md "$stage/BETRIEB.md"
install -m 0755 infrastructure/linux/setup.sh "$stage/setup.sh"
install -m 0644 LICENSE "$stage/LICENSE"
install -m 0644 "$licenses" "$stage/THIRD-PARTY-LICENSES.txt"
tar -C "$out" --owner=0 --group=0 -czf "${out}/${name}.tar.gz" "$name"
echo "${out}/${name}.tar.gz"
