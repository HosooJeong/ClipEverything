#pragma once
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <shellapi.h>
#include <functional>
#include <string>

class TrayService {
public:
    std::function<void()> OnOpenPopup;
    std::function<void()> OnOpenSettings;
    std::function<void()> OnOpenHelp;

    bool Initialize(HWND hMsgWnd, HINSTANCE hInst);
    void Dispose();
    void ShowBalloon(const std::wstring& title, const std::wstring& msg);

    // hMsgWnd의 WndProc에서 WM_APP_TRAY 수신 시 호출
    void HandleTrayMessage(LPARAM lParam);

private:
    NOTIFYICONDATAW _nid = {};
    bool _added = false;
};
