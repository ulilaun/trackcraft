# Baut das Windows-ZIP nach RFC-0005/RFC-0015 aus vorhandenen Build-Ausgaben.
# Aufruf (Repository-Wurzel): ./scripts/package-windows.ps1 -Version 1.0.0 [-OutDir dist]
# Erwartet: backend/target/release/trackcraft.exe, frontend/dist, THIRD-PARTY-LICENSES.txt (vom Release-Workflow erzeugt).
# Lokal (-Local): baut Frontend und Backend selbst und legt statt der Lizenzliste einen Hinweis bei.
# Ein so gebautes ZIP ist kein Release (RFC-0015), nur zum Ausprobieren auf dem eigenen PC.
param(
    [Parameter(Mandatory = $true)][string]$Version,
    [string]$OutDir = "dist",
    [string]$Licenses = "THIRD-PARTY-LICENSES.txt",
    [switch]$Local
)
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

if ($Local) {
    # npm und cargo schreiben Fortschritt auf stderr; Windows PowerShell 5.1 hielte das bei "Stop" für einen Fehler.
    $ErrorActionPreference = "Continue"
    Push-Location frontend
    npm ci; if ($LASTEXITCODE -ne 0) { throw "npm ci fehlgeschlagen" }
    npm run build; if ($LASTEXITCODE -ne 0) { throw "Frontend-Build fehlgeschlagen" }
    Pop-Location
    $env:TRACKCRAFT_VERSION = $Version
    cargo build --release --locked --manifest-path backend/Cargo.toml
    $code = $LASTEXITCODE
    Remove-Item Env:TRACKCRAFT_VERSION
    if ($code -ne 0) { throw "Backend-Build fehlgeschlagen" }
    $ErrorActionPreference = "Stop"
    New-Item -ItemType Directory -Force $OutDir | Out-Null
    $Licenses = Join-Path $OutDir "THIRD-PARTY-LICENSES.txt"
    [IO.File]::WriteAllText((Join-Path (Get-Location) $Licenses), "Lokaler Build ohne Lizenzliste. Die vollständige Liste enthält nur das Release-ZIP.`r`n")
}

$name = "trackcraft-$Version-windows-x86_64"
$stage = Join-Path $OutDir $name
if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
New-Item -ItemType Directory -Force (Join-Path $stage "web") | Out-Null

$required = @(
    "backend/target/release/trackcraft.exe",
    "frontend/dist/index.html",
    "backend/config/trackcraft.example.toml",
    "infrastructure/windows/BETRIEB.md",
    "infrastructure/windows/setup.cmd",
    "LICENSE",
    $Licenses
)
foreach ($f in $required) { if (-not (Test-Path $f)) { throw "Fehlt: $f" } }

Copy-Item "backend/target/release/trackcraft.exe" $stage
Copy-Item "frontend/dist/*" (Join-Path $stage "web") -Recurse
Copy-Item "backend/config/trackcraft.example.toml" $stage
Copy-Item "infrastructure/windows/BETRIEB.md" $stage
Copy-Item "infrastructure/windows/setup.cmd" $stage
Copy-Item "LICENSE" $stage
Copy-Item $Licenses (Join-Path $stage "THIRD-PARTY-LICENSES.txt")

$zip = Join-Path $OutDir "$name.zip"
if (Test-Path $zip) { Remove-Item -Force $zip }
Compress-Archive -Path (Join-Path $stage "*") -DestinationPath $zip
Write-Output $zip
