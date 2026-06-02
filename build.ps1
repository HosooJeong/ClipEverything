# ClipEverything Win32 C++ Build Script
$ErrorActionPreference = "Stop"
$Root = $PSScriptRoot
$Src  = "$Root\src"
$Tp   = "$Root\third_party"
$Res  = "$Root\resources"
$Out  = "$Root\build"
$Obj  = "$Out\obj"

# vcvars 환경 변수를 현재 PowerShell 세션에 적용
$vcvars = "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat"
if (-not (Test-Path $vcvars)) { Write-Error "vcvars64.bat not found"; exit 1 }

$tempBat = [System.IO.Path]::GetTempFileName() -replace '\.tmp$', '.bat'
"@echo off`r`ncall `"$vcvars`" >nul 2>&1`r`nset" | Set-Content -Encoding ASCII -Path $tempBat
$envLines = & cmd /c $tempBat 2>&1
Remove-Item $tempBat -ErrorAction SilentlyContinue
foreach ($l in $envLines) {
    if ($l -match '^([^=]+)=(.*)$') {
        [System.Environment]::SetEnvironmentVariable($matches[1], $matches[2], 'Process')
    }
}

# build 디렉토리
if (-not (Test-Path $Out)) { New-Item -ItemType Directory -Path $Out | Out-Null }
if (-not (Test-Path $Obj)) { New-Item -ItemType Directory -Path $Obj | Out-Null }

# 리소스 컴파일
Write-Host "[1/3] Compiling resources..."
& rc.exe /nologo /fo "$Out\ClipEverything.res" "$Res\ClipEverything.rc"
if ($LASTEXITCODE -ne 0) { Write-Error "rc.exe failed"; exit 1 }

# 컴파일 및 링크
Write-Host "[2/3] Compiling C++ sources (this may take a minute)..."

$clArgs = @(
    "/nologo", "/W3", "/WX-", "/O2", "/Oi", "/GL",
    "/std:c++17", "/EHsc", "/MT", "/GS", "/utf-8",
    "/Fo$Obj\",
    "/DUNICODE", "/D_UNICODE", "/DWIN32", "/D_WINDOWS", "/DNDEBUG",
    "/DSQLITE_THREADSAFE=0", "/DSQLITE_OMIT_LOAD_EXTENSION",
    "/I$Src", "/I$Tp", "/I$Res",
    "$Src\main.cpp",
    "$Src\core\hotkey_manager.cpp",
    "$Src\core\clipboard_reader.cpp",
    "$Src\core\clipboard_writer.cpp",
    "$Src\core\source_detector.cpp",
    "$Src\data\repository.cpp",
    "$Src\services\app_settings.cpp",
    "$Src\services\storage_paths.cpp",
    "$Src\services\clipboard_service.cpp",
    "$Src\services\startup_service.cpp",
    "$Src\services\tray_service.cpp",
    "$Src\services\toast_service.cpp",
    "$Src\ui\render\d2d_context.cpp",
    "$Src\ui\render\image_cache.cpp",
    "$Src\ui\overlay_window.cpp",
    "$Src\ui\toast_popup.cpp",
    "$Src\ui\settings_window.cpp",
    "$Src\ui\help_window.cpp",
    "$Src\ui\rename_dialog.cpp",
    "$Src\ui\tag_dialog.cpp",
    "$Tp\sqlite3.c",
    "/link",
    "/nologo", "/SUBSYSTEM:WINDOWS", "/LTCG",
    "/OUT:$Out\ClipEverything.exe",
    "user32.lib", "kernel32.lib", "gdi32.lib", "ole32.lib", "oleaut32.lib",
    "shell32.lib", "shlwapi.lib", "comctl32.lib", "comdlg32.lib",
    "d2d1.lib", "dwrite.lib", "windowscodecs.lib", "dwmapi.lib",
    "advapi32.lib", "bcrypt.lib", "uxtheme.lib",
    "$Out\ClipEverything.res"
)

& cl.exe @clArgs
if ($LASTEXITCODE -ne 0) { Write-Error "Build FAILED (cl.exe exit $LASTEXITCODE)"; exit 1 }

Write-Host "[3/3] Done."
$exePath = "$Out\ClipEverything.exe"
if (Test-Path $exePath) {
    $info = Get-Item $exePath
    $mb   = [math]::Round($info.Length / 1MB, 2)
    Write-Host ""
    Write-Host "BUILD SUCCEEDED: $exePath"
    Write-Host "  Size: $($info.Length) bytes  ($mb MB)"
} else {
    Write-Error "EXE not produced — check compiler output above"
    exit 1
}
