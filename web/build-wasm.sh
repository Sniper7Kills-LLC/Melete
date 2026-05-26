#!/usr/bin/env bash
# build-wasm.sh — compile the two WASM crates and emit wasm-bindgen
# JS shims into web/src/wasm/generated/{shim,viewer}/.
#
# Prereqs (one-time):
#   rustup target add wasm32-unknown-unknown
#   cargo install wasm-bindgen-cli --version 0.2.122  # match wasm-bindgen crate
#
# Run from the repo root or the web/ dir — both work.

set -euo pipefail

# Resolve repo root (parent of this script's directory).
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/.." >/dev/null 2>&1 && pwd)"

cd "${REPO_ROOT}"

OUT_BASE="${REPO_ROOT}/web/src/wasm/generated"
mkdir -p "${OUT_BASE}/shim" "${OUT_BASE}/viewer"

echo "[build-wasm] cargo build --release --target wasm32-unknown-unknown -p melete-web-shim"
cargo build --target wasm32-unknown-unknown -p melete-web-shim --release

echo "[build-wasm] cargo build --release --target wasm32-unknown-unknown -p melete-web-viewer"
cargo build --target wasm32-unknown-unknown -p melete-web-viewer --release

WASM_DIR="${REPO_ROOT}/target/wasm32-unknown-unknown/release"

echo "[build-wasm] wasm-bindgen → web/src/wasm/generated/shim"
wasm-bindgen "${WASM_DIR}/melete_web_shim.wasm" \
    --out-dir "${OUT_BASE}/shim" \
    --target web

echo "[build-wasm] wasm-bindgen → web/src/wasm/generated/viewer"
wasm-bindgen "${WASM_DIR}/melete_web_viewer.wasm" \
    --out-dir "${OUT_BASE}/viewer" \
    --target web

echo "[build-wasm] done."
echo "  shim   → ${OUT_BASE}/shim/melete_web_shim.{js,wasm}"
echo "  viewer → ${OUT_BASE}/viewer/melete_web_viewer.{js,wasm}"
