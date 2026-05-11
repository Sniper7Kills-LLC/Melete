#!/usr/bin/env bash
# install.sh — fetch the latest released journal-app tarball and
# install it into ~/.local/. Safe to run as `curl -fsSL <url> | bash`.
# No build toolchain required.
#
# Override the manifest source:
#   JOURNAL_MANIFEST_URL=https://releases.journal.app/latest.json bash install.sh
#
# Override the install prefix (default: $HOME/.local):
#   PREFIX=/usr/local sudo -E bash install.sh
#
# To install from a local source checkout instead, use
# `scripts/install-from-source.sh` — that one runs `cargo build`.

set -euo pipefail

MANIFEST_URL="${JOURNAL_MANIFEST_URL:-https://releases.journal.app/latest.json}"
PREFIX="${PREFIX:-$HOME/.local}"

err() { echo "error: $*" >&2; exit 1; }

command -v curl >/dev/null 2>&1 || err "curl not found in PATH"
command -v tar  >/dev/null 2>&1 || err "tar not found in PATH"

# Resolve platform key. Linux x86_64 only for now; extend as more
# targets ship.
case "$(uname -s)-$(uname -m)" in
    Linux-x86_64) PLATFORM="linux-x86_64" ;;
    *) err "unsupported platform $(uname -s)-$(uname -m)" ;;
esac

echo "==> Fetching release manifest: ${MANIFEST_URL}"
MANIFEST="$(curl -fsSL "${MANIFEST_URL}")"

# Hand-parse the manifest so we don't depend on jq. Manifest shape is
# small + stable (see .github/workflows/release.yml).
url_for() {
    printf '%s' "${MANIFEST}" \
        | tr -d '\n' \
        | grep -oE "\"$1\"[[:space:]]*:[[:space:]]*\{[^{}]*\}" \
        | grep -oE '"url"[[:space:]]*:[[:space:]]*"[^"]+"' \
        | head -1 \
        | sed -E 's/.*"url"[[:space:]]*:[[:space:]]*"([^"]+)".*/\1/'
}
sha_for() {
    printf '%s' "${MANIFEST}" \
        | tr -d '\n' \
        | grep -oE "\"$1\"[[:space:]]*:[[:space:]]*\{[^{}]*\}" \
        | grep -oE '"sha256"[[:space:]]*:[[:space:]]*"[a-f0-9]+"' \
        | head -1 \
        | sed -E 's/.*"sha256"[[:space:]]*:[[:space:]]*"([a-f0-9]+)".*/\1/'
}

URL="$(url_for "${PLATFORM}")"
EXPECTED_SHA="$(sha_for "${PLATFORM}")"
[ -n "${URL}" ] || err "manifest has no platforms.${PLATFORM} url"

TMP="$(mktemp -d)"
trap 'rm -rf "${TMP}"' EXIT

echo "==> Downloading ${URL}"
curl -fsSL "${URL}" -o "${TMP}/journal-app.tar.gz"

if [ -n "${EXPECTED_SHA}" ]; then
    ACTUAL_SHA="$(sha256sum "${TMP}/journal-app.tar.gz" | awk '{print $1}')"
    [ "${ACTUAL_SHA}" = "${EXPECTED_SHA}" ] \
        || err "sha256 mismatch — expected ${EXPECTED_SHA}, got ${ACTUAL_SHA}"
    echo "==> Verified sha256"
fi

echo "==> Extracting"
tar -xzf "${TMP}/journal-app.tar.gz" -C "${TMP}"

# Stop any running instance so the new binary doesn't get blocked by
# a held file descriptor.
if pgrep -x journal-app >/dev/null 2>&1; then
    echo "==> Stopping running instance(s)"
    pkill -x journal-app || true
    sleep 1
fi

BIN_DIR="${PREFIX}/bin"
APP_DIR="${PREFIX}/share/applications"
ICON_DIR="${PREFIX}/share/icons/hicolor/scalable/apps"
mkdir -p "${BIN_DIR}" "${APP_DIR}" "${ICON_DIR}"

echo "==> Installing to ${PREFIX}"
install -Dm755 "${TMP}/journal-app" "${BIN_DIR}/journal-app"
sed "s|^Exec=journal-app|Exec=${BIN_DIR}/journal-app|" \
    "${TMP}/dev.s7k.journal.desktop" > "${APP_DIR}/dev.s7k.journal.desktop"
chmod 644 "${APP_DIR}/dev.s7k.journal.desktop"
install -Dm644 "${TMP}/dev.s7k.journal.svg" "${ICON_DIR}/dev.s7k.journal.svg"

if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "${APP_DIR}" || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -f -t "${PREFIX}/share/icons/hicolor" || true
fi

echo
echo "Done. Launch from the application menu or run 'journal-app'."
echo "Ensure ${BIN_DIR} is on your PATH."
