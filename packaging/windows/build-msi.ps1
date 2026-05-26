# Windows portable .zip builder (Phase 7.5, #43).
#
# Phase 7.5 ships a portable .zip only — the previous cargo-wix MSI
# path is parked until Phase B (EV-cert code signing). An unsigned
# MSI triggers Windows Defender SmartScreen warnings that are *more*
# user-hostile than the same `.exe` inside a clearly-portable .zip,
# so removing it is a net usability win in addition to dropping the
# WiX harvest complexity.
#
# Prerequisites the CI step installs before invoking this script:
#   - rustup target add x86_64-pc-windows-msvc
#   - gvsbuild prebuilt GTK4 + libadwaita unpacked under C:\gvsbuild
#     so pkg-config + the linker can find gtk4 / pango / cairo / etc.
#   - 7z (preinstalled on windows-latest)
#
# Inputs:
#   - target/release/melete-app.exe
#   - $env:GVSBUILD_ROOT/bin/*.dll  (when set)
#
# Outputs:
#   - dist/Melete-${env:VERSION}-x86_64-portable.zip

$ErrorActionPreference = 'Stop'

$RepoRoot = (Resolve-Path "$PSScriptRoot\..\..").Path
Set-Location $RepoRoot

$Version = if ($env:VERSION) { $env:VERSION } else { 'dev' }

$Dist = Join-Path $RepoRoot 'dist'
$Stage = Join-Path $Dist 'windows-stage'
New-Item -ItemType Directory -Force -Path $Dist | Out-Null
Remove-Item -Recurse -Force $Stage -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $Stage | Out-Null

if (-not (Test-Path 'target\release\melete-app.exe')) {
    throw 'target\release\melete-app.exe missing — run cargo build --release first'
}

# Assemble bin/. melete-app.exe + every gvsbuild DLL.
$Bin = Join-Path $Stage 'bin'
New-Item -ItemType Directory -Force -Path $Bin | Out-Null
Copy-Item target\release\melete-app.exe $Bin
if ($env:GVSBUILD_ROOT) {
    Get-ChildItem -Path "$env:GVSBUILD_ROOT\bin" -Filter '*.dll' | ForEach-Object {
        Copy-Item $_.FullName $Bin
    }
}

# README.txt next to the binary so first-time users know what to do.
$Readme = @"
Melete — portable Windows build
================================

This folder is a drop-in install. To run:

  bin\melete-app.exe

Notes:
- Unsigned. Windows SmartScreen may warn the first time; click "More info"
  -> "Run anyway". A signed MSI installer will ship in a future release.
- User data lives in %APPDATA%\melete\ (config) and %LOCALAPPDATA%\melete\
  (notebooks). Uninstall: close the app, delete this folder, optionally
  delete the data folders above.
- AGPL-3.0-or-later. https://melete.app
"@
Set-Content -Path (Join-Path $Stage 'README.txt') -Value $Readme

# Portable zip.
$PortableZip = Join-Path $Dist "Melete-$Version-x86_64-portable.zip"
Remove-Item -Force $PortableZip -ErrorAction SilentlyContinue
& 7z a $PortableZip "$Stage\*" | Out-Null
if ($LASTEXITCODE -ne 0) { throw "7z failed with exit $LASTEXITCODE" }

Write-Host "[windows] portable zip → $PortableZip"
