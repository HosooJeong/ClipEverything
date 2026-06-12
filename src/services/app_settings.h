#pragma once
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <string>

struct AppSettings {
    bool showToastNotifications = true;
    bool openOverlayAfterCopy   = true;  // 복사 핫키 직후 오버레이를 열고 이름 입력
    bool runAtStartup           = false;
    UINT copyMods   = MOD_WIN | MOD_CONTROL | MOD_NOREPEAT;
    UINT copyVk     = 'C';
    UINT pasteMods  = MOD_WIN | MOD_CONTROL | MOD_NOREPEAT;
    UINT pasteVk    = 'V';
    std::wstring copyLabel  = L"Win+Ctrl+C";
    std::wstring pasteLabel = L"Win+Ctrl+V";
    bool overlayBoundsSaved = false;
    int overlayX            = 0;
    int overlayY            = 0;
    int overlayHeight       = 0;

    static AppSettings Load();
    void Save() const;
};
