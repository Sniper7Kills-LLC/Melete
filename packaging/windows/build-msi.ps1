# Windows MSI + portable zip builder (Phase 7.5, #43).
#
# Prerequisites the CI step installs before invoking this script:
#   - rustup target add x86_64-pc-windows-msvc
#   - cargo install cargo-wix
#   - gvsbuild prebuilt GTK4 + libadwaita unpacked under C:\gvsbuild
#     so pkg-config + the linker can find gtk4 / pango / cairo / etc.
#   - 7z (preinstalled on windows-latest)
#
# Inputs:
#   - target/release/melete-app.exe
#   - resources/icons/app.melete.svg
#   - packaging/windows/wix/main.wxs
#
# Outputs:
#   - dist/Melete-${env:VERSION}-x86_64.msi
#   - dist/Melete-${env:VERSION}-x86_64-portable.zip
#
# UNSIGNED. End users get a Windows Defender SmartScreen warning
# until we ship with an EV code-signing cert (Phase B).

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

# Assemble the staging tree the WiX harvester + portable zip both
# consume. melete-app.exe + every DLL gvsbuild emits goes into bin/.
$Bin = Join-Path $Stage 'bin'
New-Item -ItemType Directory -Force -Path $Bin | Out-Null
Copy-Item target\release\melete-app.exe $Bin
if ($env:GVSBUILD_ROOT) {
    Get-ChildItem -Path "$env:GVSBUILD_ROOT\bin" -Filter '*.dll' | ForEach-Object {
        Copy-Item $_.FullName $Bin
    }
}

# Portable zip — drop-in folder users can run without an installer.
$PortableZip = Join-Path $Dist "Melete-$Version-x86_64-portable.zip"
Remove-Item -Force $PortableZip -ErrorAction SilentlyContinue
& 7z a $PortableZip "$Stage\*" | Out-Null

# MSI via cargo-wix. Pass --target-dir so the wix scratch lives under
# target/ (cached by the workflow) instead of polluting the repo.
& cargo wix `
    --package melete-app `
    --name Melete `
    --install-version $Version `
    --include "$Stage\bin" `
    --output (Join-Path $Dist "Melete-$Version-x86_64.msi") `
    --nocapture

Write-Host "[windows] portable zip → $PortableZip"
Write-Host "[windows] MSI         → $Dist\Melete-$Version-x86_64.msi"
