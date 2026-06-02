#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <commdlg.h>
#include <shellapi.h>
#include <shlobj.h>
#include <shlwapi.h>
#include <shobjidl.h>
#include <objbase.h>
#include <commctrl.h>
#include <tlhelp32.h>

#include <filesystem>
#include <fstream>
#include <optional>
#include <string>
#include <vector>
#include <cwctype>

#pragma comment(lib, "comctl32.lib")
#pragma comment(lib, "comdlg32.lib")
#pragma comment(lib, "ole32.lib")
#pragma comment(lib, "shell32.lib")
#pragma comment(lib, "shlwapi.lib")
#pragma comment(lib, "advapi32.lib")

namespace fs = std::filesystem;

namespace {

constexpr int IDI_SETUP_APP = 101;
constexpr int IDR_SETUP_PAYLOAD = 201;

constexpr wchar_t kAppName[] = L"ClipEverything";
constexpr wchar_t kUninstallExeName[] = L"ClipEverything \uC81C\uAC70.exe";
constexpr wchar_t kMainExeName[] = L"ClipEverything.exe";
constexpr wchar_t kShortcutName[] = L"ClipEverything.lnk";
constexpr wchar_t kStartMenuFolderName[] = L"ClipEverything";
constexpr wchar_t kUninstallRegistryKey[] =
    L"Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\ClipEverything";

enum ControlId : int {
    IDC_PATH_LABEL = 1001,
    IDC_PATH_EDIT = 1002,
    IDC_BROWSE = 1003,
    IDC_DESKTOP_SHORTCUT = 1004,
    IDC_STARTMENU_SHORTCUT = 1005,
    IDC_CONFIRM = 1006,
    IDC_CANCEL = 1007,
    IDC_UNINSTALL_INFO = 1008,
    IDC_PURGE_DATA = 1009,
};

enum class WindowMode {
    Install,
    Uninstall,
};

struct LaunchOptions {
    WindowMode mode = WindowMode::Install;
    bool elevatedInstall = false;
    bool confirmedUninstall = false;
    bool desktopShortcut = true;
    bool startMenuShortcut = false;
    bool purgeData = false;
    std::wstring installDir;
};

struct WindowState {
    WindowMode mode = WindowMode::Install;
    int dpi = 96;
    HFONT font = nullptr;
    HWND infoLabel = nullptr;
    HWND pathLabel = nullptr;
    HWND pathEdit = nullptr;
    HWND browseButton = nullptr;
    HWND desktopCheck = nullptr;
    HWND startMenuCheck = nullptr;
    HWND purgeDataCheck = nullptr;
    HWND confirmButton = nullptr;
    HWND cancelButton = nullptr;
    std::wstring installDir;
};

int ScaleForDpi(int value, int dpi)
{
    return MulDiv(value, dpi, 96);
}

std::wstring Quote(const std::wstring& value)
{
    return L"\"" + value + L"\"";
}

std::wstring TrimTrailingSlash(std::wstring value)
{
    while (value.length() > 3 && !value.empty() &&
           (value.back() == L'\\' || value.back() == L'/')) {
        value.pop_back();
    }
    return value;
}

std::wstring NormalizePath(const std::wstring& input)
{
    if (input.empty()) return {};

    DWORD needed = GetFullPathNameW(input.c_str(), 0, nullptr, nullptr);
    if (needed == 0) return {};

    std::wstring out(needed, L'\0');
    DWORD written = GetFullPathNameW(input.c_str(), needed, out.data(), nullptr);
    if (written == 0) return {};
    out.resize(written);
    return TrimTrailingSlash(out);
}

std::wstring GetModulePath()
{
    std::wstring path(MAX_PATH, L'\0');
    while (true) {
        DWORD written = GetModuleFileNameW(nullptr, path.data(), static_cast<DWORD>(path.size()));
        if (written == 0) return {};
        if (written < path.size() - 1) {
            path.resize(written);
            return path;
        }
        path.resize(path.size() * 2);
    }
}

std::wstring GetModuleFileNameOnly()
{
    return fs::path(GetModulePath()).filename().wstring();
}

std::wstring GetModuleDir()
{
    return fs::path(GetModulePath()).parent_path().wstring();
}

std::wstring GetAppDataDir()
{
    wchar_t appData[MAX_PATH] = {};
    SHGetFolderPathW(nullptr, CSIDL_APPDATA, nullptr, 0, appData);
    return std::wstring(appData) + L"\\ClipEverything";
}

std::wstring GetProgramFilesDefaultInstallDir()
{
    wchar_t programFiles[MAX_PATH] = {};
    SHGetFolderPathW(nullptr, CSIDL_PROGRAM_FILES, nullptr, 0, programFiles);
    return std::wstring(programFiles) + L"\\ClipEverything";
}

std::wstring GetDesktopShortcutPath()
{
    wchar_t path[MAX_PATH] = {};
    SHGetFolderPathW(nullptr, CSIDL_DESKTOPDIRECTORY, nullptr, 0, path);
    return std::wstring(path) + L"\\" + kShortcutName;
}

std::wstring GetStartMenuFolderPath()
{
    wchar_t path[MAX_PATH] = {};
    SHGetFolderPathW(nullptr, CSIDL_PROGRAMS, nullptr, 0, path);
    return std::wstring(path) + L"\\" + kStartMenuFolderName;
}

std::wstring GetStartMenuShortcutPath()
{
    return GetStartMenuFolderPath() + L"\\" + kShortcutName;
}

bool IsAdministrator()
{
    return IsUserAnAdmin() != FALSE;
}

void ShowError(HWND owner, const std::wstring& message)
{
    MessageBoxW(owner, message.c_str(), L"ClipEverything", MB_OK | MB_ICONERROR);
}

void ShowInfo(HWND owner, const std::wstring& message)
{
    MessageBoxW(owner, message.c_str(), L"ClipEverything", MB_OK | MB_ICONINFORMATION);
}

bool StartsWith(const std::wstring& value, const std::wstring& prefix)
{
    return value.rfind(prefix, 0) == 0;
}

std::wstring ToLower(std::wstring value)
{
    for (auto& ch : value) ch = static_cast<wchar_t>(towlower(ch));
    return value;
}

std::wstring GetArgValue(const std::wstring& arg, const std::wstring& prefix)
{
    if (!StartsWith(arg, prefix)) return {};
    return arg.substr(prefix.size());
}

LaunchOptions ParseLaunchOptions()
{
    LaunchOptions options;
    options.installDir = GetProgramFilesDefaultInstallDir();

    const std::wstring fileName = GetModuleFileNameOnly();
    if (fileName.find(L"\uC81C\uAC70") != std::wstring::npos) {
        options.mode = WindowMode::Uninstall;
        options.installDir = GetModuleDir();
    }

    int argc = 0;
    LPWSTR* argv = CommandLineToArgvW(GetCommandLineW(), &argc);
    if (!argv) return options;

    for (int i = 1; i < argc; ++i) {
        const std::wstring arg = argv[i];
        if (arg == L"--uninstall") {
            options.mode = WindowMode::Uninstall;
        } else if (arg == L"--elevated-install") {
            options.mode = WindowMode::Install;
            options.elevatedInstall = true;
        } else if (arg == L"--confirmed-uninstall") {
            options.mode = WindowMode::Uninstall;
            options.confirmedUninstall = true;
        } else if (StartsWith(arg, L"--install-dir=")) {
            options.installDir = NormalizePath(GetArgValue(arg, L"--install-dir="));
        } else if (StartsWith(arg, L"--desktop-shortcut=")) {
            options.desktopShortcut = GetArgValue(arg, L"--desktop-shortcut=") == L"1";
        } else if (StartsWith(arg, L"--startmenu-shortcut=")) {
            options.startMenuShortcut = GetArgValue(arg, L"--startmenu-shortcut=") == L"1";
        } else if (StartsWith(arg, L"--purge-data=")) {
            options.purgeData = GetArgValue(arg, L"--purge-data=") == L"1";
        }
    }

    LocalFree(argv);
    if (options.installDir.empty()) {
        options.installDir = (options.mode == WindowMode::Install)
            ? GetProgramFilesDefaultInstallDir()
            : GetModuleDir();
    }
    return options;
}

bool CreateFontForWindow(WindowState* state)
{
    if (state->font) {
        DeleteObject(state->font);
        state->font = nullptr;
    }

    LOGFONTW lf = {};
    lf.lfHeight = -ScaleForDpi(13, state->dpi);
    lf.lfQuality = CLEARTYPE_QUALITY;
    wcscpy_s(lf.lfFaceName, L"Segoe UI");
    state->font = CreateFontIndirectW(&lf);
    return state->font != nullptr;
}

void ApplyFont(WindowState* state, HWND hwnd)
{
    if (state && state->font && hwnd) {
        SendMessageW(hwnd, WM_SETFONT, reinterpret_cast<WPARAM>(state->font), TRUE);
    }
}

bool DirectoryContainsEntries(const fs::path& dir)
{
    std::error_code ec;
    if (!fs::exists(dir, ec) || !fs::is_directory(dir, ec)) return false;
    return fs::directory_iterator(dir, ec) != fs::directory_iterator();
}

bool LooksLikeExistingInstall(const fs::path& dir)
{
    std::error_code ec;
    if (!fs::exists(dir, ec) || !fs::is_directory(dir, ec)) return false;

    const auto appExe = dir / kMainExeName;
    const auto uninstallExe = dir / kUninstallExeName;
    return fs::exists(appExe, ec) || fs::exists(uninstallExe, ec);
}

bool ValidateInstallDir(const std::wstring& rawPath, std::wstring& normalized, std::wstring& error)
{
    normalized = NormalizePath(rawPath);
    if (normalized.empty()) {
        error = L"\uC124\uCE58 \uD3F4\uB354 \uACBD\uB85C\uB97C \uD655\uC778\uD574\uC8FC\uC138\uC694.";
        return false;
    }

    if (PathIsRootW(normalized.c_str())) {
        error = L"\uB4DC\uB77C\uC774\uBE0C \uB8E8\uD2B8\uC5D0\uB294 \uC124\uCE58\uD560 \uC218 \uC5C6\uC2B5\uB2C8\uB2E4.";
        return false;
    }

    std::error_code ec;
    fs::path dir(normalized);
    if (fs::exists(dir, ec) && !fs::is_directory(dir, ec)) {
        error = L"\uC120\uD0DD\uD55C \uACBD\uB85C\uC5D0 \uC774\uBBF8 \uD30C\uC77C\uC774 \uC788\uC2B5\uB2C8\uB2E4.";
        return false;
    }

    if (DirectoryContainsEntries(dir) && !LooksLikeExistingInstall(dir)) {
        error = L"\uBE44\uC5B4 \uC788\uC9C0 \uC54A\uC740 \uB2E4\uB978 \uD3F4\uB354\uC5D0\uB294 \uC124\uCE58\uD560 \uC218 \uC5C6\uC2B5\uB2C8\uB2E4.";
        return false;
    }

    return true;
}

bool WriteResourceToFile(int resourceId, const std::wstring& outputPath)
{
    HRSRC resource = FindResourceW(nullptr, MAKEINTRESOURCEW(resourceId), RT_RCDATA);
    if (!resource) return false;

    HGLOBAL loaded = LoadResource(nullptr, resource);
    if (!loaded) return false;

    const DWORD size = SizeofResource(nullptr, resource);
    const void* data = LockResource(loaded);
    if (!data || size == 0) return false;

    HANDLE file = CreateFileW(outputPath.c_str(), GENERIC_WRITE, 0, nullptr, CREATE_ALWAYS,
                              FILE_ATTRIBUTE_NORMAL, nullptr);
    if (file == INVALID_HANDLE_VALUE) return false;

    DWORD written = 0;
    const BOOL ok = WriteFile(file, data, size, &written, nullptr);
    CloseHandle(file);
    return ok == TRUE && written == size;
}

bool CopySelfTo(const std::wstring& destinationPath)
{
    return CopyFileW(GetModulePath().c_str(), destinationPath.c_str(), FALSE) != FALSE;
}

bool CreateShortcutFile(const std::wstring& linkPath,
                        const std::wstring& targetPath,
                        const std::wstring& workingDir,
                        const std::wstring& iconPath)
{
    IShellLinkW* shellLink = nullptr;
    HRESULT hr = CoCreateInstance(CLSID_ShellLink, nullptr, CLSCTX_INPROC_SERVER,
                                  IID_IShellLinkW, reinterpret_cast<void**>(&shellLink));
    if (FAILED(hr)) return false;

    shellLink->SetPath(targetPath.c_str());
    shellLink->SetWorkingDirectory(workingDir.c_str());
    shellLink->SetIconLocation(iconPath.c_str(), 0);

    IPersistFile* persist = nullptr;
    hr = shellLink->QueryInterface(IID_IPersistFile, reinterpret_cast<void**>(&persist));
    if (FAILED(hr)) {
        shellLink->Release();
        return false;
    }

    hr = persist->Save(linkPath.c_str(), TRUE);
    persist->Release();
    shellLink->Release();
    return SUCCEEDED(hr);
}

void RemovePathIfExists(const std::wstring& path)
{
    std::error_code ec;
    fs::remove(fs::path(path), ec);
}

void RemoveDirectoryTreeIfExists(const std::wstring& path)
{
    std::error_code ec;
    fs::remove_all(fs::path(path), ec);
}

bool SetRegistryString(HKEY key, const wchar_t* name, const std::wstring& value)
{
    return RegSetValueExW(key, name, 0, REG_SZ,
                          reinterpret_cast<const BYTE*>(value.c_str()),
                          static_cast<DWORD>((value.size() + 1) * sizeof(wchar_t))) == ERROR_SUCCESS;
}

bool SetRegistryDword(HKEY key, const wchar_t* name, DWORD value)
{
    return RegSetValueExW(key, name, 0, REG_DWORD,
                          reinterpret_cast<const BYTE*>(&value),
                          sizeof(value)) == ERROR_SUCCESS;
}

bool RegisterUninstallEntry(const std::wstring& installDir)
{
    HKEY key = nullptr;
    if (RegCreateKeyExW(HKEY_LOCAL_MACHINE, kUninstallRegistryKey, 0, nullptr, 0,
                        KEY_WRITE, nullptr, &key, nullptr) != ERROR_SUCCESS) {
        return false;
    }

    const std::wstring appExe = installDir + L"\\" + kMainExeName;
    const std::wstring uninstallExe = installDir + L"\\" + kUninstallExeName;
    const std::wstring uninstallCommand = Quote(uninstallExe);

    bool ok = true;
    ok &= SetRegistryString(key, L"DisplayName", kAppName);
    ok &= SetRegistryString(key, L"DisplayVersion", L"1.0.0");
    ok &= SetRegistryString(key, L"Publisher", kAppName);
    ok &= SetRegistryString(key, L"DisplayIcon", appExe);
    ok &= SetRegistryString(key, L"InstallLocation", installDir);
    ok &= SetRegistryString(key, L"UninstallString", uninstallCommand);
    ok &= SetRegistryString(key, L"QuietUninstallString", uninstallCommand);
    ok &= SetRegistryDword(key, L"NoModify", 1);
    ok &= SetRegistryDword(key, L"NoRepair", 1);

    RegCloseKey(key);
    return ok;
}

void DeleteUninstallEntry()
{
    RegDeleteTreeW(HKEY_LOCAL_MACHINE, kUninstallRegistryKey);
}

bool IsClipEverythingRunning()
{
    HANDLE snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
    if (snapshot == INVALID_HANDLE_VALUE) return false;

    PROCESSENTRY32W entry = {};
    entry.dwSize = sizeof(entry);
    bool running = false;

    if (Process32FirstW(snapshot, &entry)) {
        do {
            std::wstring exeName = ToLower(entry.szExeFile);
            if (exeName == L"clipeverything.exe") {
                running = true;
                break;
            }
        } while (Process32NextW(snapshot, &entry));
    }

    CloseHandle(snapshot);
    return running;
}

bool RelaunchElevated(const std::wstring& args)
{
    SHELLEXECUTEINFOW sei = { sizeof(sei) };
    const std::wstring modulePath = GetModulePath();
    sei.lpVerb = L"runas";
    sei.lpFile = modulePath.c_str();
    sei.lpParameters = args.c_str();
    sei.nShow = SW_SHOWNORMAL;
    return ShellExecuteExW(&sei) == TRUE;
}

std::wstring GetInstallArgs(const std::wstring& installDir, bool desktopShortcut, bool startMenuShortcut)
{
    std::wstring args = L"--elevated-install ";
    args += L"--install-dir=" + Quote(installDir);
    args += desktopShortcut ? L" --desktop-shortcut=1" : L" --desktop-shortcut=0";
    args += startMenuShortcut ? L" --startmenu-shortcut=1" : L" --startmenu-shortcut=0";
    return args;
}

std::wstring GetUninstallArgs(bool purgeData)
{
    std::wstring args = L"--confirmed-uninstall";
    args += purgeData ? L" --purge-data=1" : L" --purge-data=0";
    return args;
}

std::optional<std::wstring> BrowseForFolder(HWND owner, const std::wstring& initialPath)
{
    IFileDialog* dialog = nullptr;
    HRESULT hr = CoCreateInstance(CLSID_FileOpenDialog, nullptr, CLSCTX_INPROC_SERVER,
                                  IID_IFileDialog, reinterpret_cast<void**>(&dialog));
    if (FAILED(hr)) return std::nullopt;

    DWORD options = 0;
    dialog->GetOptions(&options);
    dialog->SetOptions(options | FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM | FOS_PATHMUSTEXIST);
    dialog->SetTitle(L"\uC124\uCE58 \uD3F4\uB354 \uC120\uD0DD");

    if (!initialPath.empty() && fs::exists(initialPath)) {
        IShellItem* folder = nullptr;
        if (SUCCEEDED(SHCreateItemFromParsingName(initialPath.c_str(), nullptr, IID_PPV_ARGS(&folder)))) {
            dialog->SetFolder(folder);
            folder->Release();
        }
    }

    std::optional<std::wstring> result;
    if (SUCCEEDED(dialog->Show(owner))) {
        IShellItem* item = nullptr;
        if (SUCCEEDED(dialog->GetResult(&item))) {
            PWSTR selected = nullptr;
            if (SUCCEEDED(item->GetDisplayName(SIGDN_FILESYSPATH, &selected))) {
                result = NormalizePath(selected);
                CoTaskMemFree(selected);
            }
            item->Release();
        }
    }

    dialog->Release();
    return result;
}

bool ScheduleInstallDirDeletion(const std::wstring& installDir)
{
    wchar_t tempPath[MAX_PATH] = {};
    if (GetTempPathW(MAX_PATH, tempPath) == 0) return false;

    wchar_t tempFile[MAX_PATH] = {};
    if (GetTempFileNameW(tempPath, L"cex", 0, tempFile) == 0) return false;

    fs::path scriptPath = tempFile;
    scriptPath.replace_extension(L".vbs");
    MoveFileExW(tempFile, scriptPath.c_str(), MOVEFILE_REPLACE_EXISTING);

    std::ofstream out(scriptPath, std::ios::binary);
    if (!out.is_open()) return false;

    auto writeLine = [&](const std::string& line) {
        out << line << "\r\n";
    };

    writeLine("\xEF\xBB\xBFOption Explicit");
    writeLine("Dim fso, folder, i");
    writeLine("Set fso = CreateObject(\"Scripting.FileSystemObject\")");

    std::wstring folderLine = L"folder = \"" + installDir + L"\"";
    int len = WideCharToMultiByte(CP_UTF8, 0, folderLine.c_str(), -1, nullptr, 0, nullptr, nullptr);
    std::string utf8Line(len - 1, '\0');
    WideCharToMultiByte(CP_UTF8, 0, folderLine.c_str(), -1, utf8Line.data(), len, nullptr, nullptr);
    writeLine(utf8Line);

    writeLine("For i = 1 To 20");
    writeLine("  On Error Resume Next");
    writeLine("  If fso.FolderExists(folder) Then fso.DeleteFolder folder, True");
    writeLine("  If Err.Number = 0 Then Exit For");
    writeLine("  Err.Clear");
    writeLine("  WScript.Sleep 1000");
    writeLine("Next");
    writeLine("On Error Resume Next");
    writeLine("fso.DeleteFile WScript.ScriptFullName, True");
    out.close();

    std::wstring commandLine = L"wscript.exe " + Quote(scriptPath.wstring());
    STARTUPINFOW si = { sizeof(si) };
    PROCESS_INFORMATION pi = {};
    std::vector<wchar_t> mutableCommand(commandLine.begin(), commandLine.end());
    mutableCommand.push_back(L'\0');

    BOOL ok = CreateProcessW(nullptr, mutableCommand.data(), nullptr, nullptr, FALSE,
                             CREATE_NO_WINDOW, nullptr, nullptr, &si, &pi);
    if (ok) {
        CloseHandle(pi.hThread);
        CloseHandle(pi.hProcess);
        return true;
    }

    return false;
}

bool InstallApplication(const LaunchOptions& options, std::wstring& error)
{
    const std::wstring installDir = options.installDir;
    const std::wstring appExePath = installDir + L"\\" + kMainExeName;
    const std::wstring uninstallExePath = installDir + L"\\" + kUninstallExeName;

    std::error_code ec;
    fs::create_directories(installDir, ec);
    if (ec) {
        error = L"\uC124\uCE58 \uD3F4\uB354\uB97C \uB9CC\uB4E4 \uC218 \uC5C6\uC2B5\uB2C8\uB2E4.";
        return false;
    }

    if (!WriteResourceToFile(IDR_SETUP_PAYLOAD, appExePath)) {
        error = L"\uC571 \uD30C\uC77C\uC744 \uC124\uCE58 \uD3F4\uB354\uB85C \uBCF5\uC0AC\uD558\uC9C0 \uBABB\uD588\uC2B5\uB2C8\uB2E4.";
        return false;
    }

    if (!CopySelfTo(uninstallExePath)) {
        error = L"\uC5B8\uC778\uC2A4\uD1A8 \uD30C\uC77C\uC744 \uB9CC\uB4E4 \uC218 \uC5C6\uC2B5\uB2C8\uB2E4.";
        return false;
    }

    if (options.desktopShortcut) {
        if (!CreateShortcutFile(GetDesktopShortcutPath(), appExePath, installDir, appExePath)) {
            error = L"\uBC14\uD0D5\uD654\uBA74 \uBC14\uB85C\uAC00\uAE30\uB97C \uB9CC\uB4E4 \uC218 \uC5C6\uC2B5\uB2C8\uB2E4.";
            return false;
        }
    } else {
        RemovePathIfExists(GetDesktopShortcutPath());
    }

    if (options.startMenuShortcut) {
        fs::create_directories(GetStartMenuFolderPath(), ec);
        if (ec || !CreateShortcutFile(GetStartMenuShortcutPath(), appExePath, installDir, appExePath)) {
            error = L"\uC2DC\uC791 \uBA54\uB274 \uBC14\uB85C\uAC00\uAE30\uB97C \uB9CC\uB4E4 \uC218 \uC5C6\uC2B5\uB2C8\uB2E4.";
            return false;
        }
    } else {
        RemovePathIfExists(GetStartMenuShortcutPath());
        RemoveDirectoryTreeIfExists(GetStartMenuFolderPath());
    }

    if (!RegisterUninstallEntry(installDir)) {
        error = L"\uC81C\uC5B4\uD310 \uC81C\uAC70 \uC815\uBCF4\uB97C \uB4F1\uB85D\uD558\uC9C0 \uBABB\uD588\uC2B5\uB2C8\uB2E4.";
        return false;
    }

    return true;
}

bool UninstallApplication(const LaunchOptions& options, std::wstring& warning)
{
    const std::wstring installDir = GetModuleDir();

    RemovePathIfExists(GetDesktopShortcutPath());
    RemovePathIfExists(GetStartMenuShortcutPath());
    RemoveDirectoryTreeIfExists(GetStartMenuFolderPath());
    DeleteUninstallEntry();

    if (options.purgeData) {
        std::error_code ec;
        fs::remove_all(GetAppDataDir(), ec);
        if (ec) {
            warning = L"\uC571 \uD30C\uC77C\uC740 \uC81C\uAC70\uB418\uC9C0\uB9CC, \uC124\uC815/\uD074\uB9BD \uB370\uC774\uD130 \uD3F4\uB354\uB97C \uC77C\uBD80 \uC9C0\uC6B0\uC9C0 \uBABB\uD588\uC2B5\uB2C8\uB2E4.";
        }
    }

    if (!ScheduleInstallDirDeletion(installDir)) {
        warning = L"\uC124\uCE58 \uD3F4\uB354 \uC815\uB9AC\uB97C \uC790\uB3D9\uC73C\uB85C \uB9C8\uCE58\uC9C0 \uBABB\uD588\uC2B5\uB2C8\uB2E4. \uD504\uB85C\uC138\uC2A4 \uC885\uB8CC \uD6C4 \uC218\uB3D9\uC73C\uB85C \uC0AD\uC81C\uD574\uC8FC\uC138\uC694.";
    }

    return true;
}

void LayoutInstallWindow(WindowState* state, int width, int height)
{
    const int pad = ScaleForDpi(14, state->dpi);
    const int labelH = ScaleForDpi(48, state->dpi);
    const int rowH = ScaleForDpi(24, state->dpi);
    const int editH = ScaleForDpi(26, state->dpi);
    const int buttonW = ScaleForDpi(88, state->dpi);
    const int buttonH = ScaleForDpi(30, state->dpi);
    const int browseW = ScaleForDpi(90, state->dpi);
    const int gap = ScaleForDpi(10, state->dpi);
    const int pathLabelW = ScaleForDpi(72, state->dpi);

    MoveWindow(state->infoLabel, pad, pad, width - pad * 2, labelH, TRUE);
    MoveWindow(state->pathLabel, pad, pad + labelH + gap, pathLabelW, rowH, TRUE);
    MoveWindow(state->pathEdit, pad + pathLabelW + gap, pad + labelH + gap - ScaleForDpi(2, state->dpi),
               width - pad * 2 - pathLabelW - browseW - gap * 2, editH, TRUE);
    MoveWindow(state->browseButton, width - pad - browseW, pad + labelH + gap - ScaleForDpi(2, state->dpi),
               browseW, buttonH, TRUE);
    MoveWindow(state->desktopCheck, pad, pad + labelH + gap + rowH + gap, width - pad * 2, rowH, TRUE);
    MoveWindow(state->startMenuCheck, pad, pad + labelH + gap + rowH + gap + rowH + ScaleForDpi(4, state->dpi),
               width - pad * 2, rowH, TRUE);
    MoveWindow(state->confirmButton, width - pad - buttonW * 2 - gap, height - pad - buttonH,
               buttonW, buttonH, TRUE);
    MoveWindow(state->cancelButton, width - pad - buttonW, height - pad - buttonH,
               buttonW, buttonH, TRUE);
}

void LayoutUninstallWindow(WindowState* state, int width, int height)
{
    const int pad = ScaleForDpi(14, state->dpi);
    const int infoH = ScaleForDpi(56, state->dpi);
    const int editH = ScaleForDpi(52, state->dpi);
    const int rowH = ScaleForDpi(24, state->dpi);
    const int buttonW = ScaleForDpi(88, state->dpi);
    const int buttonH = ScaleForDpi(30, state->dpi);
    const int gap = ScaleForDpi(10, state->dpi);

    MoveWindow(state->infoLabel, pad, pad, width - pad * 2, infoH, TRUE);
    MoveWindow(state->pathEdit, pad, pad + infoH + gap, width - pad * 2, editH, TRUE);
    MoveWindow(state->purgeDataCheck, pad, pad + infoH + gap + editH + gap, width - pad * 2, rowH, TRUE);
    MoveWindow(state->confirmButton, width - pad - buttonW * 2 - gap, height - pad - buttonH,
               buttonW, buttonH, TRUE);
    MoveWindow(state->cancelButton, width - pad - buttonW, height - pad - buttonH,
               buttonW, buttonH, TRUE);
}

void ApplyFonts(WindowState* state)
{
    ApplyFont(state, state->infoLabel);
    ApplyFont(state, state->pathLabel);
    ApplyFont(state, state->pathEdit);
    ApplyFont(state, state->browseButton);
    ApplyFont(state, state->desktopCheck);
    ApplyFont(state, state->startMenuCheck);
    ApplyFont(state, state->purgeDataCheck);
    ApplyFont(state, state->confirmButton);
    ApplyFont(state, state->cancelButton);
}

void LayoutWindow(HWND hwnd, WindowState* state)
{
    RECT rc = {};
    GetClientRect(hwnd, &rc);
    const int width = rc.right - rc.left;
    const int height = rc.bottom - rc.top;

    if (state->mode == WindowMode::Install) {
        LayoutInstallWindow(state, width, height);
    } else {
        LayoutUninstallWindow(state, width, height);
    }
}

std::wstring GetWindowTitle(WindowMode mode)
{
    return mode == WindowMode::Install
        ? L"ClipEverything \uC124\uCE58"
        : L"ClipEverything \uC81C\uAC70";
}

LRESULT CALLBACK SetupWndProc(HWND hwnd, UINT msg, WPARAM wp, LPARAM lp)
{
    auto* state = reinterpret_cast<WindowState*>(GetWindowLongPtrW(hwnd, GWLP_USERDATA));

    switch (msg) {
    case WM_CREATE: {
        auto* cs = reinterpret_cast<CREATESTRUCTW*>(lp);
        auto* initState = reinterpret_cast<WindowState*>(cs->lpCreateParams);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, reinterpret_cast<LONG_PTR>(initState));
        state = initState;
        state->dpi = GetDpiForWindow(hwnd);
        CreateFontForWindow(state);

        if (state->mode == WindowMode::Install) {
            state->infoLabel = CreateWindowExW(0, L"STATIC",
                L"ClipEverything\uC744 \uC124\uCE58\uD560 \uD3F4\uB354\uC640 \uBC14\uB85C\uAC00\uAE30 \uC635\uC158\uC744 \uC120\uD0DD\uD558\uC138\uC694.",
                WS_CHILD | WS_VISIBLE,
                0, 0, 0, 0, hwnd, reinterpret_cast<HMENU>(1), nullptr, nullptr);
            state->pathLabel = CreateWindowExW(0, L"STATIC",
                L"\uC124\uCE58 \uD3F4\uB354",
                WS_CHILD | WS_VISIBLE,
                0, 0, 0, 0, hwnd, reinterpret_cast<HMENU>(IDC_PATH_LABEL), nullptr, nullptr);
            state->pathEdit = CreateWindowExW(WS_EX_CLIENTEDGE, L"EDIT",
                state->installDir.c_str(),
                WS_CHILD | WS_VISIBLE | WS_TABSTOP | ES_AUTOHSCROLL,
                0, 0, 0, 0, hwnd, reinterpret_cast<HMENU>(IDC_PATH_EDIT), nullptr, nullptr);
            state->browseButton = CreateWindowExW(0, L"BUTTON",
                L"\uCC3E\uC544\uBCF4\uAE30...",
                WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_PUSHBUTTON,
                0, 0, 0, 0, hwnd, reinterpret_cast<HMENU>(IDC_BROWSE), nullptr, nullptr);
            state->desktopCheck = CreateWindowExW(0, L"BUTTON",
                L"\uBC14\uD0D5\uD654\uBA74 \uBC14\uB85C\uAC00\uAE30 \uB9CC\uB4E4\uAE30",
                WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_AUTOCHECKBOX,
                0, 0, 0, 0, hwnd, reinterpret_cast<HMENU>(IDC_DESKTOP_SHORTCUT), nullptr, nullptr);
            state->startMenuCheck = CreateWindowExW(0, L"BUTTON",
                L"\uC2DC\uC791 \uBA54\uB274 \uBC14\uB85C\uAC00\uAE30 \uB9CC\uB4E4\uAE30",
                WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_AUTOCHECKBOX,
                0, 0, 0, 0, hwnd, reinterpret_cast<HMENU>(IDC_STARTMENU_SHORTCUT), nullptr, nullptr);
            state->confirmButton = CreateWindowExW(0, L"BUTTON",
                L"\uC124\uCE58",
                WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_DEFPUSHBUTTON,
                0, 0, 0, 0, hwnd, reinterpret_cast<HMENU>(IDC_CONFIRM), nullptr, nullptr);
            state->cancelButton = CreateWindowExW(0, L"BUTTON",
                L"\uCDE8\uC18C",
                WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_PUSHBUTTON,
                0, 0, 0, 0, hwnd, reinterpret_cast<HMENU>(IDC_CANCEL), nullptr, nullptr);

            SendMessageW(state->desktopCheck, BM_SETCHECK, BST_CHECKED, 0);
            SendMessageW(state->startMenuCheck, BM_SETCHECK, BST_UNCHECKED, 0);
        } else {
            state->infoLabel = CreateWindowExW(0, L"STATIC",
                L"\uB2E4\uC74C \uC704\uCE58\uC5D0\uC11C ClipEverything\uC744 \uC81C\uAC70\uD569\uB2C8\uB2E4.",
                WS_CHILD | WS_VISIBLE,
                0, 0, 0, 0, hwnd, reinterpret_cast<HMENU>(IDC_UNINSTALL_INFO), nullptr, nullptr);
            state->pathEdit = CreateWindowExW(WS_EX_CLIENTEDGE, L"EDIT",
                state->installDir.c_str(),
                WS_CHILD | WS_VISIBLE | ES_MULTILINE | ES_READONLY | ES_AUTOVSCROLL,
                0, 0, 0, 0, hwnd, reinterpret_cast<HMENU>(IDC_PATH_EDIT), nullptr, nullptr);
            state->purgeDataCheck = CreateWindowExW(0, L"BUTTON",
                L"\uC124\uC815 \uBC0F \uD074\uB9BD \uAE30\uB85D\uB3C4 \uD568\uAED8 \uC0AD\uC81C",
                WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_AUTOCHECKBOX,
                0, 0, 0, 0, hwnd, reinterpret_cast<HMENU>(IDC_PURGE_DATA), nullptr, nullptr);
            state->confirmButton = CreateWindowExW(0, L"BUTTON",
                L"\uC81C\uAC70",
                WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_DEFPUSHBUTTON,
                0, 0, 0, 0, hwnd, reinterpret_cast<HMENU>(IDC_CONFIRM), nullptr, nullptr);
            state->cancelButton = CreateWindowExW(0, L"BUTTON",
                L"\uCDE8\uC18C",
                WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_PUSHBUTTON,
                0, 0, 0, 0, hwnd, reinterpret_cast<HMENU>(IDC_CANCEL), nullptr, nullptr);
            SendMessageW(state->purgeDataCheck, BM_SETCHECK, BST_UNCHECKED, 0);
        }

        ApplyFonts(state);
        LayoutWindow(hwnd, state);
        return 0;
    }

    case WM_SIZE:
        if (state) LayoutWindow(hwnd, state);
        return 0;

    case WM_DPICHANGED:
        if (state) {
            state->dpi = HIWORD(wp);
            CreateFontForWindow(state);
            ApplyFonts(state);
            auto* suggested = reinterpret_cast<RECT*>(lp);
            SetWindowPos(hwnd, nullptr,
                         suggested->left, suggested->top,
                         suggested->right - suggested->left,
                         suggested->bottom - suggested->top,
                         SWP_NOZORDER | SWP_NOACTIVATE);
            LayoutWindow(hwnd, state);
        }
        return 0;

    case WM_COMMAND:
        if (!state) break;

        switch (LOWORD(wp)) {
        case IDC_BROWSE: {
            std::wstring current = state->installDir;
            wchar_t buffer[4096] = {};
            if (state->pathEdit) GetWindowTextW(state->pathEdit, buffer, ARRAYSIZE(buffer));
            current = buffer;
            auto folder = BrowseForFolder(hwnd, current);
            if (folder) {
                state->installDir = *folder;
                SetWindowTextW(state->pathEdit, state->installDir.c_str());
            }
            return 0;
        }

        case IDC_CONFIRM:
            if (state->mode == WindowMode::Install) {
                wchar_t pathBuffer[4096] = {};
                GetWindowTextW(state->pathEdit, pathBuffer, ARRAYSIZE(pathBuffer));
                std::wstring normalized;
                std::wstring error;
                if (!ValidateInstallDir(pathBuffer, normalized, error)) {
                    ShowError(hwnd, error);
                    return 0;
                }

                LaunchOptions options;
                options.mode = WindowMode::Install;
                options.installDir = normalized;
                options.desktopShortcut =
                    SendMessageW(state->desktopCheck, BM_GETCHECK, 0, 0) == BST_CHECKED;
                options.startMenuShortcut =
                    SendMessageW(state->startMenuCheck, BM_GETCHECK, 0, 0) == BST_CHECKED;

                if (IsClipEverythingRunning()) {
                    ShowError(hwnd, L"\uC124\uCE58 \uB610\uB294 \uC5C5\uB370\uC774\uD2B8 \uC804\uC5D0 ClipEverything\uC744 \uBA3C\uC800 \uC885\uB8CC\uD574\uC8FC\uC138\uC694.");
                    return 0;
                }

                if (!IsAdministrator()) {
                    if (!RelaunchElevated(GetInstallArgs(options.installDir, options.desktopShortcut, options.startMenuShortcut))) {
                        ShowError(hwnd, L"\uAD00\uB9AC\uC790 \uAD8C\uD55C \uC2E4\uD589\uC744 \uC2DC\uC791\uD558\uC9C0 \uBABB\uD588\uC2B5\uB2C8\uB2E4.");
                    }
                    DestroyWindow(hwnd);
                    return 0;
                }

                std::wstring installError;
                if (!InstallApplication(options, installError)) {
                    ShowError(hwnd, installError);
                    return 0;
                }

                ShowInfo(hwnd, L"ClipEverything \uC124\uCE58\uAC00 \uC644\uB8CC\uB418\uC5C8\uC2B5\uB2C8\uB2E4.");
                DestroyWindow(hwnd);
            } else {
                LaunchOptions options;
                options.mode = WindowMode::Uninstall;
                options.purgeData =
                    SendMessageW(state->purgeDataCheck, BM_GETCHECK, 0, 0) == BST_CHECKED;

                if (IsClipEverythingRunning()) {
                    ShowError(hwnd, L"ClipEverything\uC774 \uC2E4\uD589 \uC911\uC774\uBA74 \uC81C\uAC70\uD560 \uC218 \uC5C6\uC2B5\uB2C8\uB2E4. \uBA3C\uC800 \uD504\uB85C\uADF8\uB7A8\uC744 \uC885\uB8CC\uD574\uC8FC\uC138\uC694.");
                    return 0;
                }

                if (!IsAdministrator()) {
                    if (!RelaunchElevated(GetUninstallArgs(options.purgeData))) {
                        ShowError(hwnd, L"\uAD00\uB9AC\uC790 \uAD8C\uD55C \uC2E4\uD589\uC744 \uC2DC\uC791\uD558\uC9C0 \uBABB\uD588\uC2B5\uB2C8\uB2E4.");
                    }
                    DestroyWindow(hwnd);
                    return 0;
                }

                std::wstring warning;
                if (!UninstallApplication(options, warning)) {
                    ShowError(hwnd, L"ClipEverything \uC81C\uAC70\uB97C \uC9C4\uD589\uD558\uC9C0 \uBABB\uD588\uC2B5\uB2C8\uB2E4.");
                    return 0;
                }

                if (!warning.empty()) {
                    ShowInfo(hwnd, warning);
                } else {
                    ShowInfo(hwnd, L"ClipEverything \uC81C\uAC70\uAC00 \uC644\uB8CC\uB418\uC5C8\uC2B5\uB2C8\uB2E4.");
                }
                DestroyWindow(hwnd);
            }
            return 0;

        case IDC_CANCEL:
            DestroyWindow(hwnd);
            return 0;
        }
        break;

    case WM_CLOSE:
        DestroyWindow(hwnd);
        return 0;

    case WM_DESTROY:
        if (state) {
            if (state->font) DeleteObject(state->font);
            delete state;
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
        }
        PostQuitMessage(0);
        return 0;
    }

    return DefWindowProcW(hwnd, msg, wp, lp);
}

bool RegisterSetupClass(HINSTANCE hInstance)
{
    WNDCLASSEXW wc = { sizeof(wc) };
    wc.style = CS_HREDRAW | CS_VREDRAW;
    wc.lpfnWndProc = SetupWndProc;
    wc.hInstance = hInstance;
    wc.hCursor = LoadCursorW(nullptr, IDC_ARROW);
    wc.hIcon = static_cast<HICON>(LoadImageW(hInstance, MAKEINTRESOURCEW(IDI_SETUP_APP),
                                             IMAGE_ICON, GetSystemMetrics(SM_CXICON),
                                             GetSystemMetrics(SM_CYICON), LR_DEFAULTCOLOR));
    wc.hIconSm = static_cast<HICON>(LoadImageW(hInstance, MAKEINTRESOURCEW(IDI_SETUP_APP),
                                               IMAGE_ICON, GetSystemMetrics(SM_CXSMICON),
                                               GetSystemMetrics(SM_CYSMICON), LR_DEFAULTCOLOR));
    wc.hbrBackground = reinterpret_cast<HBRUSH>(COLOR_WINDOW + 1);
    wc.lpszClassName = L"ClipEverythingSetupHost";
    return RegisterClassExW(&wc) != 0 || GetLastError() == ERROR_CLASS_ALREADY_EXISTS;
}

} // namespace

int WINAPI wWinMain(HINSTANCE hInstance, HINSTANCE, PWSTR, int)
{
    SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    CoInitializeEx(nullptr, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE);
    InitCommonControls();

    const LaunchOptions options = ParseLaunchOptions();

    if (options.elevatedInstall) {
        std::wstring normalized;
        std::wstring error;
        if (!ValidateInstallDir(options.installDir, normalized, error)) {
            ShowError(nullptr, error);
            CoUninitialize();
            return 1;
        }

        if (IsClipEverythingRunning()) {
            ShowError(nullptr, L"\uC124\uCE58 \uB610\uB294 \uC5C5\uB370\uC774\uD2B8 \uC804\uC5D0 ClipEverything\uC744 \uBA3C\uC800 \uC885\uB8CC\uD574\uC8FC\uC138\uC694.");
            CoUninitialize();
            return 1;
        }

        LaunchOptions elevated = options;
        elevated.installDir = normalized;
        std::wstring installError;
        if (!InstallApplication(elevated, installError)) {
            ShowError(nullptr, installError);
            CoUninitialize();
            return 1;
        }

        ShowInfo(nullptr, L"ClipEverything \uC124\uCE58\uAC00 \uC644\uB8CC\uB418\uC5C8\uC2B5\uB2C8\uB2E4.");
        CoUninitialize();
        return 0;
    }

    if (options.confirmedUninstall) {
        if (IsClipEverythingRunning()) {
            ShowError(nullptr, L"ClipEverything\uC774 \uC2E4\uD589 \uC911\uC774\uBA74 \uC81C\uAC70\uD560 \uC218 \uC5C6\uC2B5\uB2C8\uB2E4. \uBA3C\uC800 \uD504\uB85C\uADF8\uB7A8\uC744 \uC885\uB8CC\uD574\uC8FC\uC138\uC694.");
            CoUninitialize();
            return 1;
        }

        std::wstring warning;
        if (!UninstallApplication(options, warning)) {
            ShowError(nullptr, L"ClipEverything \uC81C\uAC70\uB97C \uC9C4\uD589\uD558\uC9C0 \uBABB\uD588\uC2B5\uB2C8\uB2E4.");
            CoUninitialize();
            return 1;
        }

        if (!warning.empty()) {
            ShowInfo(nullptr, warning);
        } else {
            ShowInfo(nullptr, L"ClipEverything \uC81C\uAC70\uAC00 \uC644\uB8CC\uB418\uC5C8\uC2B5\uB2C8\uB2E4.");
        }
        CoUninitialize();
        return 0;
    }

    if (!RegisterSetupClass(hInstance)) {
        ShowError(nullptr, L"\uC124\uCE58 \uCC3D \uCD08\uAE30\uD654\uC5D0 \uC2E4\uD328\uD588\uC2B5\uB2C8\uB2E4.");
        CoUninitialize();
        return 1;
    }

    auto* state = new WindowState{};
    state->mode = options.mode;
    state->installDir = options.mode == WindowMode::Install ? options.installDir : GetModuleDir();

    const int dpi = GetDpiForSystem();
    const int width = ScaleForDpi(options.mode == WindowMode::Install ? 540 : 500, dpi);
    const int height = ScaleForDpi(options.mode == WindowMode::Install ? 250 : 220, dpi);

    HWND hwnd = CreateWindowExW(
        WS_EX_DLGMODALFRAME | WS_EX_CONTROLPARENT,
        L"ClipEverythingSetupHost",
        GetWindowTitle(options.mode).c_str(),
        WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX,
        CW_USEDEFAULT, CW_USEDEFAULT, width, height,
        nullptr, nullptr, hInstance, state);

    if (!hwnd) {
        delete state;
        ShowError(nullptr, L"\uC124\uCE58 \uCC3D\uC744 \uC5F4\uC9C0 \uBABB\uD588\uC2B5\uB2C8\uB2E4.");
        CoUninitialize();
        return 1;
    }

    ShowWindow(hwnd, SW_SHOW);
    UpdateWindow(hwnd);

    MSG msg = {};
    while (GetMessageW(&msg, nullptr, 0, 0)) {
        TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }

    CoUninitialize();
    return static_cast<int>(msg.wParam);
}
