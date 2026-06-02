#include "startup_service.h"
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <string>

static const wchar_t* kKeyName = L"ClipEverything";
static const wchar_t* kRegPath = L"Software\\Microsoft\\Windows\\CurrentVersion\\Run";

bool IsStartupRegistered()
{
    HKEY key = nullptr;
    if (RegOpenKeyExW(HKEY_CURRENT_USER, kRegPath, 0, KEY_READ, &key) != ERROR_SUCCESS)
        return false;
    DWORD type = 0;
    bool exists = RegQueryValueExW(key, kKeyName, nullptr, &type, nullptr, nullptr) == ERROR_SUCCESS;
    RegCloseKey(key);
    return exists;
}

void SetStartupEnabled(bool enable)
{
    HKEY key = nullptr;
    if (RegOpenKeyExW(HKEY_CURRENT_USER, kRegPath, 0, KEY_WRITE, &key) != ERROR_SUCCESS)
        return;

    if (enable) {
        wchar_t exePath[MAX_PATH] = {};
        GetModuleFileNameW(nullptr, exePath, MAX_PATH);
        std::wstring val = std::wstring(L"\"") + exePath + L"\"";
        RegSetValueExW(key, kKeyName, 0, REG_SZ,
                       (const BYTE*)val.c_str(),
                       (DWORD)((val.size() + 1) * sizeof(wchar_t)));
    } else {
        RegDeleteValueW(key, kKeyName);
    }
    RegCloseKey(key);
}
