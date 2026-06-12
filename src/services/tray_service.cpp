#include "tray_service.h"
#include "localization.h"
#include "../resources/resource.h"

#pragma comment(lib, "shell32.lib")

bool TrayService::Initialize(HWND hMsgWnd, HINSTANCE hInst)
{
    ZeroMemory(&_nid, sizeof(_nid));
    _nid.cbSize = sizeof(NOTIFYICONDATAW);
    _nid.hWnd = hMsgWnd;
    _nid.uID = 1;
    _nid.uFlags = NIF_ICON | NIF_TIP | NIF_MESSAGE;
    _nid.uCallbackMessage = WM_APP_TRAY;

    _nid.hIcon = static_cast<HICON>(LoadImageW(
        hInst, MAKEINTRESOURCEW(IDI_APP), IMAGE_ICON, 16, 16, LR_DEFAULTCOLOR));
    if (!_nid.hIcon) {
        _nid.hIcon = LoadIconW(nullptr, IDI_APPLICATION);
    }

    wcscpy_s(_nid.szTip, L"ClipEverything");

    _added = Shell_NotifyIconW(NIM_ADD, &_nid) != FALSE;

    _nid.uVersion = NOTIFYICON_VERSION_4;
    Shell_NotifyIconW(NIM_SETVERSION, &_nid);

    return _added;
}

void TrayService::Dispose()
{
    if (_added) {
        Shell_NotifyIconW(NIM_DELETE, &_nid);
        _added = false;
    }
}

void TrayService::ShowBalloon(const std::wstring& title, const std::wstring& msg)
{
    if (!_added) return;

    _nid.uFlags |= NIF_INFO;
    _nid.dwInfoFlags = NIIF_INFO;
    wcscpy_s(_nid.szInfoTitle, title.c_str());
    wcscpy_s(_nid.szInfo, msg.c_str());
    Shell_NotifyIconW(NIM_MODIFY, &_nid);
    _nid.uFlags &= ~NIF_INFO;
}

void TrayService::HandleTrayMessage(LPARAM lParam)
{
    const UINT msg = LOWORD(lParam);
    switch (msg) {
    case WM_LBUTTONDBLCLK:
    case NIN_SELECT:
        if (OnOpenPopup) OnOpenPopup();
        break;

    case WM_RBUTTONUP:
    case WM_CONTEXTMENU: {
        HMENU hMenu = CreatePopupMenu();
        AppendMenuW(hMenu, MF_STRING, ID_TRAY_OPEN, Tr(Str::TrayOpen));
        AppendMenuW(hMenu, MF_STRING, ID_TRAY_SETTINGS, Tr(Str::TraySettings));
        AppendMenuW(hMenu, MF_STRING, ID_TRAY_HELP, Tr(Str::TrayHelp));
        AppendMenuW(hMenu, MF_SEPARATOR, 0, nullptr);
        AppendMenuW(hMenu, MF_STRING, ID_TRAY_EXIT, Tr(Str::TrayExit));

        POINT pt{};
        GetCursorPos(&pt);
        SetForegroundWindow(_nid.hWnd);
        const UINT cmd = static_cast<UINT>(TrackPopupMenu(
            hMenu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_NONOTIFY,
            pt.x, pt.y, 0, _nid.hWnd, nullptr));
        DestroyMenu(hMenu);

        switch (cmd) {
        case ID_TRAY_OPEN:
            if (OnOpenPopup) OnOpenPopup();
            break;
        case ID_TRAY_SETTINGS:
            if (OnOpenSettings) OnOpenSettings();
            break;
        case ID_TRAY_HELP:
            if (OnOpenHelp) OnOpenHelp();
            break;
        case ID_TRAY_EXIT:
            PostQuitMessage(0);
            break;
        default:
            break;
        }
        break;
    }
    }
}
