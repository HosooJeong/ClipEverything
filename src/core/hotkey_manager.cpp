#include "hotkey_manager.h"

bool HotkeyManager::Initialize(HWND hHostWnd)
{
    _hWnd = hHostWnd;   // 참조용으로만 보관; RegisterHotKey는 NULL(스레드 큐) 사용
    return true;        // 등록은 UpdateHotkeys에서 수행
}

// NULL hWnd → WM_HOTKEY가 호출 스레드 메시지 큐에 posting됨
// (HWND_MESSAGE 창에 MOD_WIN 포함 조합이 등록 거부되는 Windows 동작 우회)
bool HotkeyManager::TryRegister(const HotkeyConfig& cfg)
{
    Unregister();
    if (!RegisterHotKey(NULL, HOTKEY_ID_COPY,  cfg.copyMods,  cfg.copyVk))  return false;
    if (!RegisterHotKey(NULL, HOTKEY_ID_PASTE, cfg.pasteMods, cfg.pasteVk)) {
        UnregisterHotKey(NULL, HOTKEY_ID_COPY);
        return false;
    }
    return true;
}

void HotkeyManager::Unregister()
{
    UnregisterHotKey(NULL, HOTKEY_ID_COPY);
    UnregisterHotKey(NULL, HOTKEY_ID_PASTE);
}

void HotkeyManager::Dispose()
{
    Unregister();
    _registered = false;
}

bool HotkeyManager::UpdateHotkeys(const HotkeyConfig& preferred)
{
    if (TryRegister(preferred)) {
        _cfg = preferred;
        _registered = true;
        return true;
    }

    // 메시지 문구는 호출 측(main)에서 현재 언어로 조립한다
    if (OnConflict)
        OnConflict(preferred.copyLabel + L"/" + preferred.pasteLabel);
    return false;
}

bool HotkeyManager::HandleMessage(WPARAM wParam)
{
    if (wParam == HOTKEY_ID_COPY)  { if (OnCopyHotkey)  OnCopyHotkey();  return true; }
    if (wParam == HOTKEY_ID_PASTE) { if (OnPasteHotkey) OnPasteHotkey(); return true; }
    return false;
}
