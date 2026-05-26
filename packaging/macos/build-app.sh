#!/usr/bin/env bash
# macOS .app + DMG builder (Phase 7.5, #43).
#
# Assembles `dist/Melete.app/Contents/{MacOS,Resources,Frameworks}`,
# bundles every Mach-O dependency the linker actually resolves at
# runtime via `dylibbundler`, then packs the whole thing into a DMG
# with `create-dmg` (falls back to `hdiutil create` if absent).
#
# UNSIGNED. macOS will refuse to launch this on a stock Gatekeeper
# install — users get the "Melete cannot be opened because Apple
# cannot check it for malicious software" dialog. Workarounds:
#   1. Right-click → Open → "Open" in the warning sheet.
#   2. `xattr -dr com.apple.quarantine /Applications/Melete.app`
# Real fix is Apple Developer ID signing + notarization (Phase B).
#
# Inputs:
#   - target/release/melete-app
#   - resources/app.melete.desktop  (informational; macOS ignores it)
#   - resources/icons/app.melete.svg
#   - packaging/macos/Info.plist
#
# Outputs:
#   - dist/Melete.app/...
#   - dist/Melete-${VERSION}-${ARCH}.dmg
#
# Env vars (optional):
#   VERSION  — defaults to `git describe`.
#   ARCH     — `arm64` (default on Apple Silicon CI) or `x86_64`.

set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/../.." >/dev/null 2>&1 && pwd)"
cd "${REPO_ROOT}"

ARCH="${ARCH:-$(uname -m)}"
VERSION="${VERSION:-$(git describe --tags --always 2>/dev/null || echo dev)}"
DIST="${REPO_ROOT}/dist"
APP="${DIST}/Melete.app"

if [[ ! -x target/release/melete-app ]]; then
    echo "error: target/release/melete-app missing — run cargo build --release first" >&2
    exit 1
fi

# Skeleton.
rm -rf "${APP}"
mkdir -p "${APP}/Contents/MacOS" \
         "${APP}/Contents/Resources" \
         "${APP}/Contents/Frameworks"

# Binary.
cp target/release/melete-app "${APP}/Contents/MacOS/melete-app"
chmod +x "${APP}/Contents/MacOS/melete-app"

# Info.plist — substitute the version placeholder.
sed "s/__VERSION__/${VERSION}/g" \
    packaging/macos/Info.plist > "${APP}/Contents/Info.plist"

# Icon. macOS needs `.icns`. We carry an SVG; convert via `sips` (in
# the macOS base install) + `iconutil`. If conversion fails we still
# ship the SVG so the bundle is functional sans icon.
ICONSET="${DIST}/AppIcon.iconset"
rm -rf "${ICONSET}"
mkdir -p "${ICONSET}"
if command -v sips >/dev/null 2>&1 && command -v rsvg-convert >/dev/null 2>&1; then
    for size in 16 32 64 128 256 512; do
        rsvg-convert -w "${size}" -h "${size}" resources/icons/app.melete.svg \
            -o "${ICONSET}/icon_${size}x${size}.png"
        # @2x retina variants share the same source rasterised at 2× px.
        rsvg-convert -w "$((size * 2))" -h "$((size * 2))" \
            resources/icons/app.melete.svg \
            -o "${ICONSET}/icon_${size}x${size}@2x.png"
    done
    iconutil -c icns "${ICONSET}" -o "${APP}/Contents/Resources/AppIcon.icns" \
        || cp resources/icons/app.melete.svg "${APP}/Contents/Resources/AppIcon.svg"
else
    cp resources/icons/app.melete.svg "${APP}/Contents/Resources/AppIcon.svg"
fi

# Bundle dylib dependencies into Frameworks/. dylibbundler rewrites
# the binary's LC_LOAD_DYLIB entries so the bundled paths are
# self-contained — required for distribution outside Homebrew.
if command -v dylibbundler >/dev/null 2>&1; then
    dylibbundler -od -b \
        -x "${APP}/Contents/MacOS/melete-app" \
        -d "${APP}/Contents/Frameworks" \
        -p "@executable_path/../Frameworks"
else
    echo "warning: dylibbundler not installed — .app will need Homebrew's gtk4 / libadwaita at runtime" >&2
fi

# DMG. `create-dmg` if available (Homebrew bottle exists), else
# fall back to `hdiutil create` which produces a basic DMG without
# the prettied-up window background.
DMG="${DIST}/Melete-${VERSION}-${ARCH}.dmg"
rm -f "${DMG}"
if command -v create-dmg >/dev/null 2>&1; then
    create-dmg \
        --volname "Melete ${VERSION}" \
        --window-pos 200 120 \
        --window-size 600 320 \
        --icon-size 100 \
        --icon "Melete.app" 175 120 \
        --hide-extension "Melete.app" \
        --app-drop-link 425 120 \
        "${DMG}" "${APP}"
else
    hdiutil create -volname "Melete ${VERSION}" \
        -srcfolder "${APP}" -ov -format UDZO "${DMG}"
fi

echo "[macos] built ${APP}"
echo "[macos] built ${DMG}"
