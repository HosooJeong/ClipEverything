param([switch]$Installer, [string]$OutputDirectory = "dist")
$ErrorActionPreference = "Stop"
Push-Location $PSScriptRoot
try {
    cargo build --locked --release
    if ($LASTEXITCODE) { throw "Cargo build failed" }
    $output = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot $OutputDirectory))
    New-Item -ItemType Directory -Path $output -Force | Out-Null
    $exe = Join-Path $PSScriptRoot "target\release\clip-everything.exe"
    Copy-Item -LiteralPath $exe -Destination (Join-Path $output "ClipEverything.exe") -Force
    # Use an explicit file list. Never enumerate a profile or a previously run package.
    $flag = Join-Path $PSScriptRoot "target\portable.flag"
    [IO.File]::WriteAllBytes($flag, [byte[]]@())
    Compress-Archive -LiteralPath (Join-Path $output "ClipEverything.exe"),$flag -DestinationPath (Join-Path $output "ClipEverything-portable.zip") -Force
    if ($Installer) {
        cargo build --locked --release -p clip-setup-actions
        if ($LASTEXITCODE) { throw "Setup actions build failed" }
        $wix = Join-Path $PSScriptRoot "target\pack-tools\wix.exe"
        if (!(Test-Path $wix)) { throw "Install WiX first: dotnet tool install --tool-path target/pack-tools wix --version 4.0.6" }
        $dotnetRoot = $env:DOTNET_ROOT
        $rollForward = $env:DOTNET_ROLL_FORWARD
        try {
            if (!$dotnetRoot) { $env:DOTNET_ROOT = Split-Path (Get-Command dotnet).Source }
            if (!$rollForward) { $env:DOTNET_ROLL_FORWARD = "Major" }
            & $wix build installer.wxs setup-ui.wxs -arch x64 -d "AppExe=$exe" -d "UninstallExe=$PSScriptRoot\target\release\uninstall.exe" -d "SetupDll=$PSScriptRoot\target\release\clip_setup_actions.dll" -d "AppIcon=$PSScriptRoot\..\resources\app.ico" -intermediateFolder "target\wix" -pdbtype none -o (Join-Path $output "ClipEverything.msi")
            if ($LASTEXITCODE) { throw "MSI build failed" }
        } finally {
            $env:DOTNET_ROOT = $dotnetRoot
            $env:DOTNET_ROLL_FORWARD = $rollForward
        }
    }
} finally { Pop-Location }
