# Build persistex.exe and an NSIS installer.
# Run on Windows with the MSVC toolchain and NSIS on PATH (choco install nsis).
$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
$Dist = Join-Path $Root "dist"
$Version = ((Select-String -Path (Join-Path $Root "Cargo.toml") -Pattern '^version' |
             Select-Object -First 1).Line -replace '.*"(.*)".*', '$1').Trim()
if (-not $Version) { throw "could not read version from Cargo.toml" }

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
# choco installs NSIS but does not put makensis on PATH for the current session
$makensis = (Get-Command makensis -ErrorAction SilentlyContinue).Source
if (-not $makensis) {
    $makensis = @(
        "$env:ProgramFiles\NSIS\makensis.exe",
        "${env:ProgramFiles(x86)}\NSIS\makensis.exe",
        "$env:ChocolateyInstall\bin\makensis.exe"
    ) | Where-Object { $_ -and (Test-Path $_) } | Select-Object -First 1
}
if (-not $makensis) {
    throw "makensis not found. Install NSIS (choco install nsis) or put it on PATH."
}
Write-Host "    using $makensis"
& $makensis /DVERSION=$Version /DOUTDIR=$Dist (Join-Path $PSScriptRoot "windows-installer.nsi")
if ($LASTEXITCODE -ne 0) { throw "makensis failed with exit code $LASTEXITCODE" }
Write-Host "==> $Dist\persistex-$Version-windows-setup.exe"
