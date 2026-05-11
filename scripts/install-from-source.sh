#!/usr/bin/env bash
# install-from-source.sh — build the release binary and install
# system-wide via the Makefile. Requires a local Rust toolchain
# (`cargo`) and is meant for contributors. End users should run
# `scripts/install.sh`, which downloads a pre-built tarball instead.

set -euo pipefail

# Repo root (parent of scripts/) so `make build` finds the Makefile
# regardless of caller cwd.
SCRIPT_DIR="$(cd -- "$(dirname -- "$(readlink -f "$0")")" >/dev/null 2>&1 && pwd)"
cd "${SCRIPT_DIR}/.."

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

if pgrep -x melete-app >/dev/null 2>&1; then
    echo "==> Stopping running instance(s)"
    pkill -x melete-app || true
    sleep 1
fi

echo "==> Installing system-wide (will prompt for sudo password)"
sudo make install

echo
echo "Done. Launch via the GNOME application menu or 'melete-app' from a terminal."
