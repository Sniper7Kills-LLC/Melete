#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$(readlink -f "$0")")"

if [[ $EUID -eq 0 ]]; then
    echo "error: do not run install.sh as root — sudo is invoked only for the install step" >&2
    exit 1
fi

if ! command -v cargo >/dev/null 2>&1; then
    echo "error: cargo not found in PATH" >&2
    exit 1
fi

echo "==> Building release binary"
make build

if pgrep -x journal-app >/dev/null 2>&1; then
    echo "==> Stopping running instance(s)"
    pkill -x journal-app || true
    sleep 1
fi

echo "==> Installing system-wide (will prompt for sudo password)"
sudo make install

echo
echo "Done. Launch via the GNOME application menu or 'journal-app' from a terminal."
