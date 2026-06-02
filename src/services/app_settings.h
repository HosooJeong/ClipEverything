#pragma once
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <string>

struct AppSettings {
    bool showToastNotifications = true;
    bool runAtStartup           = false;
    UINT copyMods   = MOD_CONTROL | MOD_SHIFT | MOD_NOREPEAT;
    UINT copyVk     = 'C';
    UINT pasteMods  = MOD_CONTROL | MOD_SHIFT | MOD_NOREPEAT;
    UINT pasteVk    = 'V';
    std::wstring copyLabel  = L"Ctrl+Shift+C";
    std::wstring pasteLabel = L"Ctrl+Shift+V";
    bool overlayBoundsSaved = false;
    int overlayX            = 0;
    int overlayY            = 0;
    int overlayHeight       = 0;

    static AppSettings Load();
    void Save() const;
};
