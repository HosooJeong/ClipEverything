# ClipEverything

English | [한국어](README.ko.md)

ClipEverything is a clipboard history manager for Windows.

It stays in the system tray, captures clipboard content through a global hotkey, and lets you quickly find and paste previous clips again. The goal is not just to store plain text, but to preserve and restore Windows clipboard formats as faithfully as possible.

## Concept

ClipEverything is built around the idea that clipboard history should change with the program you are working in.

Most clipboard managers show one long global list. That works for simple text snippets, but it becomes noisy when the clipboard contains a mix of browser links, Excel ranges, images, document fragments, file paths, and formatted text. ClipEverything records where a clip came from, what kind of content it appears to be, and which application is currently asking for a paste.

For example, clips copied from Excel can be kept with their spreadsheet-related formats, image clips can be shown with thumbnails, and rich text or HTML clips can keep more than their plain text fallback. When the overlay is opened from a target application, the list is scoped to that application context first, with an option to view the full history when needed.

The long-term direction is an adaptive clipboard manager: one that feels different when you are writing, browsing, working with spreadsheets, handling images, or moving files, because those workflows need different clipboard behavior.

This is not a polished finished product yet. It is a working prototype that I am making public to get feedback on the structure, behavior, and overall direction.

## What It Supports

- Native Windows desktop app
- Global copy and paste hotkeys
- Clipboard history storage with duplicate detection
- Automatic history retention: the most recent 1,000 non-favorite items are kept; older ones are pruned automatically (favorites are never deleted)
- Sensitive-content exclusion: clips flagged by password managers (e.g. `ExcludeClipboardContentFromMonitorProcessing`) are never stored
- Multiple clipboard formats, including text, files, and images
- Source application tracking and context-aware history filtering
- Content type detection for text, rich text, HTML, Excel-like data, HWP-like data, and images
- Local SQLite storage
- Overlay UI for searching, selecting, and pasting clips
- Rename, favorite, and tag editing for saved items
- Korean and English UI (follows the system language by default, switchable in Settings)
- System tray menu
- Startup registration
- Scripts for portable packages and a simple installer

## What It Does Not Support

- macOS / Linux
- Cloud sync
- Encrypted storage
- A formal test suite
- CI builds

Clipboard history can contain sensitive data. The app currently stores data in a local SQLite database, but it does not encrypt that database yet.

## Build Environment

The current build is Windows/MSVC only.

Required tools:

- Windows 10 or Windows 11
- Visual Studio 2022 Build Tools
- MSVC C++ build tools
- Windows SDK
- PowerShell

The build script expects `vcvars64.bat` at this path:

```powershell
C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat
```

If Visual Studio is installed somewhere else, update the `$vcvars` value in [build.ps1](build.ps1).

## Build

```powershell
.\build.ps1
```

You can also use the batch wrapper:

```bat
build.bat
```

The executable is generated here:

```text
build\ClipEverything.exe
```

Object files are written to `build\obj`, and build outputs are not committed to git.

## Run

After building, run:

```text
build\ClipEverything.exe
```

Default data location:

```text
%APPDATA%\ClipEverything
```

Settings and the clipboard database are stored there.

Default hotkeys are `Win+Ctrl+C` (copy and save) and `Win+Ctrl+V` (open paste overlay). Both can be changed in Settings; existing installs keep whatever hotkeys are already saved in `settings.json`.

## Packaging

Portable ZIP:

```powershell
.\package-portable.ps1
```

Installer:

```powershell
.\package-installer.ps1
```

Package outputs are generated under `dist`.

Note: the portable package script copies the current user's `settings.json` and `clips.db` if they exist. Before sharing a portable ZIP publicly, check that `portable-data` does not contain personal clipboard history.

## Project Structure

```text
src
├─ core       Windows hotkeys, clipboard read/write, source app detection
├─ data       SQLite repository and shared models
├─ services   Settings, tray, startup, and clipboard workflow
└─ ui         Win32/Direct2D UI
```

More details are available in [docs/program-structure.md](docs/program-structure.md). That document is currently written in Korean.

## Feedback I Am Looking For

- Whether the Windows clipboard format save/restore approach makes sense
- Whether the `core`, `data`, `services`, and `ui` split is maintainable
- Whether the SQLite schema and duplicate detection approach are reasonable
- Whether the Win32 UI code is getting too large or too tightly coupled
- What is missing from a security/privacy perspective for a clipboard history app

## License

MIT License. See [LICENSE](LICENSE).
