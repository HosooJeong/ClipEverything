$ErrorActionPreference = "Stop"

$Root = $PSScriptRoot
$Build = Join-Path $Root "build"
$Dist = Join-Path $Root "dist"
$Installer = Join-Path $Root "installer"
$Resources = Join-Path $Root "resources"
$SetupSource = Join-Path $Installer "setup_host.cpp"
$AppExe = Join-Path $Build "ClipEverything.exe"
$AppIcon = Join-Path $Resources "app.ico"
$SetupExe = Join-Path $Dist "ClipEverything-Setup.exe"
$Work = Join-Path $Dist "setup-build"
$RcPath = Join-Path $Work "setup_host.rc"
$ResPath = Join-Path $Work "setup_host.res"
$VcVars = "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat"

if (-not (Test-Path $VcVars)) {
    Write-Error "vcvars64.bat not found"
    exit 1
}

if (-not (Test-Path $SetupSource)) {
    Write-Error "installer\\setup_host.cpp not found."
    exit 1
}

if (-not (Test-Path $AppIcon)) {
    Write-Error "resources\\app.ico not found."
    exit 1
}

& (Join-Path $Root "build.ps1")
if ($LASTEXITCODE -ne 0) {
    Write-Error "Main app build failed."
    exit 1
}

if (-not (Test-Path $AppExe)) {
    Write-Error "build\\ClipEverything.exe not found after build."
    exit 1
}

$tempBat = [System.IO.Path]::GetTempFileName() -replace '\.tmp$', '.bat'
"@echo off`r`ncall `"$VcVars`" >nul 2>&1`r`nset" | Set-Content -Encoding ASCII -Path $tempBat
$envLines = & cmd /c $tempBat 2>&1
Remove-Item $tempBat -ErrorAction SilentlyContinue
foreach ($line in $envLines) {
    if ($line -match '^([^=]+)=(.*)$') {
        [System.Environment]::SetEnvironmentVariable($matches[1], $matches[2], 'Process')
    }
}

if (-not (Test-Path $Dist)) {
    New-Item -ItemType Directory -Path $Dist | Out-Null
}

if (Test-Path $Work) {
    Remove-Item -LiteralPath $Work -Recurse -Force
}
New-Item -ItemType Directory -Path $Work | Out-Null

if (Test-Path $SetupExe) {
    Remove-Item -LiteralPath $SetupExe -Force
}

$iconForRc = $AppIcon.Replace('\', '\\')
$payloadForRc = $AppExe.Replace('\', '\\')
$rc = @"
101 ICON "$iconForRc"
201 RCDATA "$payloadForRc"
"@
Set-Content -Path $RcPath -Value $rc -Encoding ASCII

& rc.exe /nologo /fo "$ResPath" "$RcPath"
if ($LASTEXITCODE -ne 0) {
    Write-Error "rc.exe failed for setup host"
    exit 1
}

$clArgs = @(
    "/nologo", "/W3", "/WX-", "/O2", "/Oi",
    "/std:c++17", "/EHsc", "/MT", "/GS", "/utf-8",
    "/DUNICODE", "/D_UNICODE", "/DWIN32", "/D_WINDOWS",
    "/Fo$Work\",
    "$SetupSource",
    "/link",
    "/nologo", "/SUBSYSTEM:WINDOWS",
    "/OUT:$SetupExe",
    "user32.lib", "kernel32.lib", "gdi32.lib", "ole32.lib", "shell32.lib",
    "shlwapi.lib", "comctl32.lib", "comdlg32.lib", "advapi32.lib",
    "$ResPath"
)

& cl.exe @clArgs
if ($LASTEXITCODE -ne 0) {
    Write-Error "Setup host build failed."
    exit 1
}

Remove-Item -LiteralPath $Work -Recurse -Force -ErrorAction SilentlyContinue

$info = Get-Item $SetupExe
$mb = [math]::Round($info.Length / 1MB, 2)
Write-Host "INSTALLER CREATED: $SetupExe"
Write-Host "  Size: $($info.Length) bytes ($mb MB)"
