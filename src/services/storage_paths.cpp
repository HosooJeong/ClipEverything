#include "storage_paths.h"

#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <shlobj.h>
#include <shlwapi.h>

#pragma comment(lib, "shell32.lib")
#pragma comment(lib, "shlwapi.lib")

namespace {

std::wstring GetExecutableDir()
{
    wchar_t path[MAX_PATH] = {};
    GetModuleFileNameW(nullptr, path, MAX_PATH);
    PathRemoveFileSpecW(path);
    return path;
}

std::wstring EnsureDirectory(const std::wstring& path)
{
    CreateDirectoryW(path.c_str(), nullptr);
    return path;
}

} // namespace

bool IsPortableMode()
{
    const std::wstring flagPath = GetExecutableDir() + L"\\portable.flag";
    const DWORD attrs = GetFileAttributesW(flagPath.c_str());
    return attrs != INVALID_FILE_ATTRIBUTES && !(attrs & FILE_ATTRIBUTE_DIRECTORY);
}

std::wstring GetClipEverythingDataDir()
{
    if (IsPortableMode()) {
        return EnsureDirectory(GetExecutableDir() + L"\\portable-data");
    }

    wchar_t appData[MAX_PATH] = {};
    SHGetFolderPathW(nullptr, CSIDL_APPDATA, nullptr, 0, appData);
    return EnsureDirectory(std::wstring(appData) + L"\\ClipEverything");
}

std::wstring GetClipEverythingSettingsPath()
{
    return GetClipEverythingDataDir() + L"\\settings.json";
}

std::wstring GetClipEverythingDatabasePath()
{
    return GetClipEverythingDataDir() + L"\\clips.db";
}
