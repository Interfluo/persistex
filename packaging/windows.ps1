# Build persistex.exe and an NSIS installer.
# Run on Windows with the MSVC toolchain and NSIS on PATH (choco install nsis).
$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
$Dist = Join-Path $Root "dist"
$Version = (Select-String -Path (Join-Path $Root "Cargo.toml") -Pattern '^version' | Select-Object -First 1) -replace '.*"(.*)".*','$1'

New-Item -ItemType Directory -Force -Path $Dist | Out-Null
Write-Host "==> building"
cargo build --release -p persistex --target x86_64-pc-windows-msvc
Copy-Item "$Root\target\x86_64-pc-windows-msvc\release\persistex.exe" "$Dist\persistex.exe" -Force

# SIGNING: with a code-signing certificate, sign before packaging. Without it,
# SmartScreen will warn recipients on first run.
if ($env:SIGN_PFX) {
    Write-Host "==> signing"
    & signtool sign /f $env:SIGN_PFX /p $env:SIGN_PFX_PASSWORD /fd SHA256 `
        /tr http://timestamp.digicert.com /td SHA256 "$Dist\persistex.exe"
}

Write-Host "==> building installer"
& makensis /DVERSION=$Version /DOUTDIR=$Dist (Join-Path $PSScriptRoot "windows-installer.nsi")
Write-Host "==> $Dist\persistex-$Version-windows-setup.exe"
