#pragma once
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <functional>
#include <string>

// 단축키 ID
constexpr int HOTKEY_ID_COPY  = 9001;
constexpr int HOTKEY_ID_PASTE = 9002;

struct HotkeyConfig {
    UINT copyMods  = MOD_WIN | MOD_CONTROL | MOD_NOREPEAT;
    UINT copyVk    = 'C';
    UINT pasteMods = MOD_WIN | MOD_CONTROL | MOD_NOREPEAT;
    UINT pasteVk   = 'V';
    std::wstring copyLabel  = L"Win+Ctrl+C";
    std::wstring pasteLabel = L"Win+Ctrl+V";
};

class HotkeyManager {
public:
    using Callback = std::function<void()>;

    // hHostWnd: WM_HOTKEY를 수신할 숨겨진 창
    bool Initialize(HWND hHostWnd);
    void Dispose();
    bool UpdateHotkeys(const HotkeyConfig& cfg);

    // WndProc에서 WM_HOTKEY 수신 시 호출
    bool HandleMessage(WPARAM wParam);

    // 이벤트 콜백
    Callback OnCopyHotkey;
    Callback OnPasteHotkey;
    std::function<void(const std::wstring&)> OnConflict;

    const HotkeyConfig& GetConfig() const { return _cfg; }

private:
    HWND        _hWnd = nullptr;
    HotkeyConfig _cfg;
    bool        _registered = false;

    bool TryRegister(const HotkeyConfig& cfg);
    void Unregister();
};
