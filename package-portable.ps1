# ClipEverything portable package builder
$ErrorActionPreference = "Stop"

$Root = $PSScriptRoot
$Build = Join-Path $Root "build"
$Dist = Join-Path $Root "dist"
$Stage = Join-Path $Dist "ClipEverything-portable"
$PortableData = Join-Path $Stage "portable-data"
$AppDataDir = Join-Path $env:APPDATA "ClipEverything"
$ExePath = Join-Path $Build "ClipEverything.exe"
$ZipPath = Join-Path $Dist "ClipEverything-portable.zip"

if (-not (Test-Path $ExePath)) {
    Write-Error "build\\ClipEverything.exe not found. Run build.ps1 first."
    exit 1
}

if (-not (Test-Path $Dist)) {
    New-Item -ItemType Directory -Path $Dist | Out-Null
}

if (Test-Path $Stage) {
    Remove-Item -LiteralPath $Stage -Recurse -Force
}

New-Item -ItemType Directory -Path $Stage | Out-Null
New-Item -ItemType Directory -Path $PortableData | Out-Null

Copy-Item -LiteralPath $ExePath -Destination (Join-Path $Stage "ClipEverything.exe")
New-Item -ItemType File -Path (Join-Path $Stage "portable.flag") | Out-Null

$dataFiles = @("settings.json", "clips.db", "clips.db-wal", "clips.db-shm")
foreach ($name in $dataFiles) {
    $source = Join-Path $AppDataDir $name
    if (Test-Path $source) {
        Copy-Item -LiteralPath $source -Destination (Join-Path $PortableData $name)
    }
}

$readme = @"
ClipEverything portable package

1. Extract this ZIP on the target PC.
2. Run ClipEverything.exe.
3. Because portable.flag is included, settings and clipboard history will be stored in the local portable-data folder next to the exe.

Included user data:
- settings.json
- clips.db
- clips.db-wal / clips.db-shm when present

If you want a clean package without current history, remove files from portable-data before sharing.
"@

Set-Content -Path (Join-Path $Stage "README-portable.txt") -Value $readme -Encoding UTF8

if (Test-Path $ZipPath) {
    Remove-Item -LiteralPath $ZipPath -Force
}

Compress-Archive -Path (Join-Path $Stage "*") -DestinationPath $ZipPath -CompressionLevel Optimal

$zipInfo = Get-Item $ZipPath
$mb = [math]::Round($zipInfo.Length / 1MB, 2)

Write-Host "PORTABLE PACKAGE CREATED: $ZipPath"
Write-Host "  Size: $($zipInfo.Length) bytes ($mb MB)"
