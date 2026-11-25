# ClipEverything

English · [한국어](README.ko.md)

ClipEverything is a native Windows x64 launcher for the clipboard items you paste repeatedly. Save a selection once, give it a name or capture a visual preview, and place it on the board for its source application. ClipEverything stores the available clipboard formats in a local SQLite database so text, tables, images, and file selections can be restored to compatible applications.

## Install or run

[Download ClipEverything for Windows](https://github.com/HosooJeong/ClipEverything/releases/latest)

| Download | Use |
| --- | --- |
| `ClipEverything.msi` | Install for the current user. Choose the install folder, shortcuts, startup and tray behavior, and hotkeys. |
| `ClipEverything.exe` | Run directly using your Windows user profile. |
| `ClipEverything-portable.zip` | Extract the whole ZIP to a writable folder. The included `portable.flag` enables storage beside the executable. |

## Everyday use

- Select content in an application and press **Ctrl+Alt+C** to register it. Enter a name, add tags, or take a screenshot for the preview.
- Press **Ctrl+Alt+V** in the destination application to open its board. Click an item or select it with the arrow keys and press **Enter** to paste.
- Open the board from the tray to copy an item for manual pasting. A check mark on the tile confirms that it reached the clipboard.
- Drag tiles to arrange them, use the edge **+** buttons to add rows or columns, and press **Ctrl+F** to search the selected application by title or tag.

The default profile uses `%APPDATA%\ClipEverythingRust` for compatibility with earlier releases. A portable copy uses `portable-data-rust` next to its executable. Format support depends on the receiving application.

## Build

Use Windows x64 with the Rust MSVC toolchain, Visual Studio C++ Build Tools, and the Windows SDK.

```powershell
cd rust
cargo test --workspace --locked --release
.\package.ps1
```

To build the MSI, install the .NET SDK and run these commands from the same directory.

```powershell
dotnet tool install --tool-path target/pack-tools wix --version 4.0.6
.\package.ps1 -Installer
```

The packages are written to `rust/dist/`. See the [project guide](rust/README.md) for installation options, legacy data import, and source layout.
