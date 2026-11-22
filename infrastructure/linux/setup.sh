#!/bin/sh
# TrackCraft unter Linux mit systemd installieren oder aktualisieren (RFC-0005 „Linux“).
# Aus dem entpackten Paket als root starten:
#   sudo ./setup.sh
#   sudo ./setup.sh --names trackcraft.home.arpa,localhost --display-name Alex --backup-dir /mnt/sicherung
# Optionen gehen an "trackcraft setup".
set -eu
cd "$(dirname "$0")"
if [ "$(id -u)" -ne 0 ]; then
    echo "Bitte als root starten: sudo ./setup.sh" >&2
    exit 1
fi
exec ./trackcraft setup --interactive "$@"
