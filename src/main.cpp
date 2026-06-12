// ClipEverything — Win32 C++17 진입점
// 역할: STA 초기화, 단일 인스턴스 보장, 모든 서비스/UI 초기화, 메시지 루프

#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <shellapi.h>
#include <shlobj.h>
#include <objbase.h>

#include "core/hotkey_manager.h"
#include "data/repository.h"
#include "services/app_settings.h"
#include "services/startup_service.h"
#include "services/tray_service.h"
#include "services/clipboard_service.h"
#include "services/toast_service.h"
#include "ui/render/d2d_context.h"
#include "ui/overlay_window.h"
#include "ui/settings_window.h"
#include "ui/help_window.h"
#include "ui/toast_popup.h"
#include "../resources/resource.h"

// ─────────────────────────────────────────
// 단일 인스턴스 Mutex 이름
static constexpr wchar_t kMutexName[] = L"ClipEverything_SingleInstance_Mutex";

// 숨겨진 호스트 창 클래스 이름 (WM_HOTKEY + WM_APP 수신)
static constexpr wchar_t kHostClass[] = L"ClipEverythingHost";

// ─────────────────────────────────────────
// 전역 포인터 (WndProc에서 접근)
struct AppState {
    HINSTANCE        hInst       = nullptr;
    HotkeyManager*   hotkeys     = nullptr;
    TrayService*     tray        = nullptr;
    ClipboardService* clip       = nullptr;
    OverlayWindow*   overlay     = nullptr;
    AppSettings*     settings    = nullptr;
    Repository*      repo        = nullptr;
};
static AppState* g_app = nullptr;

// ─────────────────────────────────────────
// 호스트 창 WndProc
static LRESULT CALLBACK HostWndProc(HWND hwnd, UINT msg, WPARAM wp, LPARAM lp)
{
    switch (msg) {
        case WM_HOTKEY:
            if (g_app && g_app->hotkeys)
                g_app->hotkeys->HandleMessage(wp);
            return 0;

        case WM_APP_TRAY:
            if (g_app && g_app->tray)
                g_app->tray->HandleTrayMessage(lp);
            return 0;

        // 다른 인스턴스가 PostMessage로 보내는 "팝업 열어줘" 요청
        case WM_APP_SHOW_OVERLAY:
            if (g_app && g_app->overlay)
                g_app->overlay->ShowAndRefresh(L"");
            return 0;

        // 클립 저장 완료 → 토스트 표시
        case WM_APP_CLIP_SAVED: {
            if (!g_app || !g_app->settings->showToastNotifications) return 0;
            int64_t itemId = (int64_t)wp;
            if (g_app->repo) {
                if (auto item = g_app->repo->GetItemById(itemId))
                    ShowToast(g_app->hInst, *item);
            }
            return 0;
        }

        case WM_DESTROY:
            PostQuitMessage(0);
            return 0;
    }
    return DefWindowProcW(hwnd, msg, wp, lp);
}

// ─────────────────────────────────────────
// 호스트 창 클래스 등록
static bool RegisterHostClass(HINSTANCE hInst)
{
    WNDCLASSEXW wc = { sizeof(wc) };
    wc.lpfnWndProc   = HostWndProc;
    wc.hInstance     = hInst;
    wc.hIcon         = static_cast<HICON>(LoadImageW(hInst, MAKEINTRESOURCEW(IDI_APP),
                                                     IMAGE_ICON,
                                                     GetSystemMetrics(SM_CXICON),
                                                     GetSystemMetrics(SM_CYICON),
                                                     LR_DEFAULTCOLOR));
    wc.hIconSm       = static_cast<HICON>(LoadImageW(hInst, MAKEINTRESOURCEW(IDI_APP),
                                                     IMAGE_ICON,
                                                     GetSystemMetrics(SM_CXSMICON),
                                                     GetSystemMetrics(SM_CYSMICON),
                                                     LR_DEFAULTCOLOR));
    wc.lpszClassName = kHostClass;
    return RegisterClassExW(&wc) != 0;
}

// ─────────────────────────────────────────
int WINAPI wWinMain(HINSTANCE hInst, HINSTANCE, PWSTR, int)
{
    // ── 1. DPI 인식
    SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

    // ── 2. 단일 인스턴스 검사
    HANDLE hMutex = CreateMutexW(nullptr, TRUE, kMutexName);
    if (GetLastError() == ERROR_ALREADY_EXISTS) {
        // 기존 인스턴스의 팝업 활성화
        // 호스트 창은 message-only(HWND_MESSAGE)라 FindWindowW로는 찾을 수 없음
        HWND hExisting = FindWindowExW(HWND_MESSAGE, nullptr, kHostClass, nullptr);
        if (hExisting)
            PostMessageW(hExisting, WM_APP_SHOW_OVERLAY, 0, 0);
        if (hMutex) CloseHandle(hMutex);
        return 0;
    }

    // ── 3. COM STA 초기화 (클립보드 API 사용 전 필수)
    CoInitializeEx(nullptr, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE);

    // ── 4. Direct2D / WIC / DirectWrite 초기화
    if (!D2DContext::Get().Initialize()) {
        MessageBoxW(nullptr, L"Direct2D 초기화에 실패했습니다.", L"오류", MB_ICONERROR);
        return 1;
    }

    // ── 5. 서비스 인스턴스 생성
    AppSettings  settings = AppSettings::Load();

    Repository   repo;
    if (!repo.Initialize()) {
        MessageBoxW(nullptr, L"데이터베이스 초기화에 실패했습니다.", L"오류", MB_ICONERROR);
        return 1;
    }

    ClipboardService clip(repo, settings);
    HotkeyManager    hotkeys;
    TrayService      tray;

    AppState appState;
    appState.hInst    = hInst;
    appState.hotkeys  = &hotkeys;
    appState.tray     = &tray;
    appState.clip     = &clip;
    appState.settings = &settings;
    appState.repo     = &repo;
    g_app = &appState;

    // ── 6. 창 클래스 등록
    RegisterHostClass(hInst);
    OverlayWindow::RegisterClass(hInst);
    RegisterSettingsClass(hInst);
    RegisterHelpClass(hInst);
    RegisterToastClass(hInst);

    // ── 7. 숨겨진 호스트 창 생성 (WM_HOTKEY 수신용)
    HWND hHost = CreateWindowExW(0, kHostClass, L"ClipEverything",
                                  0, 0, 0, 0, 0,
                                  HWND_MESSAGE, nullptr, hInst, nullptr);
    if (!hHost) {
        MessageBoxW(nullptr, L"호스트 창 생성에 실패했습니다.", L"오류", MB_ICONERROR);
        return 1;
    }

    // ── 8. OverlayWindow 생성
    OverlayWindow overlay(hInst, repo, clip, settings);
    appState.overlay = &overlay;

    // ── 9. 트레이 아이콘 초기화
    tray.OnOpenPopup = [&]() {
        overlay.ShowAndRefresh(L"");
    };
    tray.OnOpenSettings = [&]() {
        ShowSettingsWindow(hInst, hHost, settings, repo,
            [&](const HotkeyConfig& cfg) {
                hotkeys.UpdateHotkeys(cfg);
                settings.copyMods  = cfg.copyMods;
                settings.copyVk    = cfg.copyVk;
                settings.copyLabel = cfg.copyLabel;
                settings.pasteMods = cfg.pasteMods;
                settings.pasteVk   = cfg.pasteVk;
                settings.pasteLabel= cfg.pasteLabel;
                settings.Save();
            });
    };
    tray.OnOpenHelp = [&]() {
        ShowHelpWindow(hInst, nullptr, settings);
    };
    tray.Initialize(hHost, hInst);

    // ── 10. 단축키 초기화
    hotkeys.OnCopyHotkey  = [&]() { clip.OnCopyHotkey();  };
    hotkeys.OnPasteHotkey = [&]() { clip.OnPasteHotkey(); };
    hotkeys.OnConflict = [&](const std::wstring& msg) {
        tray.ShowBalloon(L"단축키 충돌", msg);
    };

    HotkeyConfig hkCfg;
    hkCfg.copyMods   = settings.copyMods;
    hkCfg.copyVk     = settings.copyVk;
    hkCfg.copyLabel  = settings.copyLabel;
    hkCfg.pasteMods  = settings.pasteMods;
    hkCfg.pasteVk    = settings.pasteVk;
    hkCfg.pasteLabel = settings.pasteLabel;
    hotkeys.Initialize(hHost);
    hotkeys.UpdateHotkeys(hkCfg);

    // ── 11. ClipboardService 콜백 연결
    clip.OnPasteRequested = [&]() {
        const SourceInfo& ctx = clip.GetPendingTarget();
        overlay.ShowAndRefresh(ctx.processName);
    };
    clip.OnItemCaptured = [&](int64_t itemId, const SourceInfo&) {
        // 복사 직후 앱을 전면으로 가져오고 오버레이를 연다.
        overlay.ShowAndEditItem(itemId);
        // 토스트 알림은 HostWndProc의 WM_APP_CLIP_SAVED에서 처리
        PostMessageW(hHost, WM_APP_CLIP_SAVED, (WPARAM)itemId, 0);
    };

    // ── 12. 시작 시 팝업 열기 (자동 실행 모드가 아닐 때만)
    // 필요하면 주석 해제:
    // overlay.ShowAndRefresh(L"");

    // ── 13. 메인 메시지 루프
    // WM_HOTKEY는 NULL hWnd로 RegisterHotKey했으므로 스레드 큐에 오며
    // DispatchMessage로는 전달되지 않음 — 루프에서 직접 처리
    MSG msg;
    while (GetMessageW(&msg, nullptr, 0, 0)) {
        if (msg.message == WM_HOTKEY) {
            if (g_app && g_app->hotkeys)
                g_app->hotkeys->HandleMessage(msg.wParam);
            continue;
        }
        TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }

    // ── 14. 정리
    hotkeys.Dispose();
    tray.Dispose();
    D2DContext::Get().Dispose();
    CoUninitialize();
    CloseHandle(hMutex);

    return (int)msg.wParam;
}
