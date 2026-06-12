#include "help_window.h"

#include <string>

#include "../resources/resource.h"
#include "../services/app_settings.h"
#include "../services/localization.h"

namespace {

constexpr wchar_t kHelpClassName[] = L"ClipEverythingHelp";
constexpr int kHelpWidth = 560;
constexpr int kHelpHeight = 520;
constexpr int kMinHelpWidth = 420;
constexpr int kMinHelpHeight = 340;

enum HelpControlId : int {
    IDC_HELP_BODY = 701,
    IDC_HELP_CLOSE = 702,
};

struct HelpCtx {
    const AppSettings* settings = nullptr;
    HFONT bodyFont = nullptr;
    HFONT buttonFont = nullptr;
    HWND body = nullptr;
    HWND close = nullptr;
    int dpi = 96;
};

HWND g_helpWindow = nullptr;

int ScaleForDpi(int value, int dpi)
{
    return MulDiv(value, dpi, 96);
}

HFONT CreateUiFont(int dpi, int pointSize, int weight = FW_NORMAL)
{
    LOGFONTW lf = {};
    lf.lfHeight = -ScaleForDpi(pointSize, dpi);
    lf.lfWeight = weight;
    lf.lfQuality = CLEARTYPE_QUALITY;
    wcscpy_s(lf.lfFaceName, L"Segoe UI");
    return CreateFontIndirectW(&lf);
}

std::wstring GetCopyLabel(const AppSettings& settings)
{
    return settings.copyLabel.empty() ? L"Win+Ctrl+C" : settings.copyLabel;
}

std::wstring GetPasteLabel(const AppSettings& settings)
{
    return settings.pasteLabel.empty() ? L"Win+Ctrl+V" : settings.pasteLabel;
}

std::wstring BuildHelpText(const AppSettings& settings)
{
    std::wstring text = Tr(Str::HelpBody);
    text = ReplaceAll(std::move(text), L"{copy}", GetCopyLabel(settings));
    text = ReplaceAll(std::move(text), L"{paste}", GetPasteLabel(settings));
    return text;
}

RECT GetInitialWindowRect(HWND hParent, int width, int height)
{
    RECT rc = {};
    HMONITOR monitor = nullptr;

    if (hParent && IsWindow(hParent)) {
        RECT parentRect{};
        GetWindowRect(hParent, &parentRect);
        rc = parentRect;
        monitor = MonitorFromRect(&parentRect, MONITOR_DEFAULTTONEAREST);
    } else {
        POINT pt{};
        GetCursorPos(&pt);
        monitor = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
    }

    MONITORINFO mi{ sizeof(mi) };
    if (!monitor || !GetMonitorInfoW(monitor, &mi)) {
        SystemParametersInfoW(SPI_GETWORKAREA, 0, &rc, 0);
    } else {
        rc = mi.rcWork;
    }

    RECT out{};
    out.left = rc.left + ((rc.right - rc.left) - width) / 2;
    out.top = rc.top + ((rc.bottom - rc.top) - height) / 2;
    out.right = out.left + width;
    out.bottom = out.top + height;
    return out;
}

void UpdateFonts(HelpCtx* ctx)
{
    if (!ctx) return;

    if (ctx->bodyFont) {
        DeleteObject(ctx->bodyFont);
        ctx->bodyFont = nullptr;
    }
    if (ctx->buttonFont) {
        DeleteObject(ctx->buttonFont);
        ctx->buttonFont = nullptr;
    }

    ctx->bodyFont = CreateUiFont(ctx->dpi, 13);
    ctx->buttonFont = CreateUiFont(ctx->dpi, 13, FW_MEDIUM);

    if (ctx->body) {
        SendMessageW(ctx->body, WM_SETFONT, reinterpret_cast<WPARAM>(ctx->bodyFont), TRUE);
    }
    if (ctx->close) {
        SendMessageW(ctx->close, WM_SETFONT, reinterpret_cast<WPARAM>(ctx->buttonFont), TRUE);
    }
}

void UpdateHelpText(HelpCtx* ctx)
{
    if (!ctx || !ctx->body || !ctx->settings) return;
    const std::wstring helpText = BuildHelpText(*ctx->settings);
    SetWindowTextW(ctx->body, helpText.c_str());
}

void LayoutControls(HWND hwnd, HelpCtx* ctx, int clientW, int clientH)
{
    if (!ctx) return;

    const int pad = ScaleForDpi(12, ctx->dpi);
    const int gap = ScaleForDpi(10, ctx->dpi);
    const int buttonW = ScaleForDpi(90, ctx->dpi);
    const int buttonH = ScaleForDpi(30, ctx->dpi);

    const int bodyX = pad;
    const int bodyY = pad;
    const int bodyW = max(ScaleForDpi(120, ctx->dpi), clientW - pad * 2);
    const int bodyH = max(ScaleForDpi(120, ctx->dpi), clientH - (pad * 2 + buttonH + gap));

    const int buttonX = clientW - pad - buttonW;
    const int buttonY = clientH - pad - buttonH;

    MoveWindow(ctx->body, bodyX, bodyY, bodyW, bodyH, TRUE);
    MoveWindow(ctx->close, buttonX, buttonY, buttonW, buttonH, TRUE);
    InvalidateRect(hwnd, nullptr, FALSE);
}

void FocusExistingHelpWindow()
{
    if (!g_helpWindow || !IsWindow(g_helpWindow)) return;
    ShowWindow(g_helpWindow, IsIconic(g_helpWindow) ? SW_RESTORE : SW_SHOW);
    SetForegroundWindow(g_helpWindow);
    BringWindowToTop(g_helpWindow);
}

LRESULT CALLBACK HelpWndProc(HWND hwnd, UINT msg, WPARAM wp, LPARAM lp)
{
    auto* ctx = reinterpret_cast<HelpCtx*>(GetWindowLongPtrW(hwnd, GWLP_USERDATA));

    switch (msg) {
    case WM_CREATE: {
        auto* cs = reinterpret_cast<CREATESTRUCTW*>(lp);
        ctx = reinterpret_cast<HelpCtx*>(cs->lpCreateParams);
        ctx->dpi = GetDpiForWindow(hwnd);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, reinterpret_cast<LONG_PTR>(ctx));

        ctx->body = CreateWindowExW(
            WS_EX_CLIENTEDGE, L"EDIT", L"",
            WS_CHILD | WS_VISIBLE | WS_VSCROLL | ES_MULTILINE | ES_AUTOVSCROLL | ES_READONLY,
            0, 0, 0, 0, hwnd, reinterpret_cast<HMENU>(static_cast<INT_PTR>(IDC_HELP_BODY)), nullptr, nullptr);

        ctx->close = CreateWindowExW(
            0, L"BUTTON", Tr(Str::HelpClose),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_PUSHBUTTON,
            0, 0, 0, 0, hwnd, reinterpret_cast<HMENU>(static_cast<INT_PTR>(IDC_HELP_CLOSE)), nullptr, nullptr);

        UpdateFonts(ctx);
        UpdateHelpText(ctx);

        RECT rc{};
        GetClientRect(hwnd, &rc);
        LayoutControls(hwnd, ctx, rc.right - rc.left, rc.bottom - rc.top);
        return 0;
    }

    case WM_SIZE:
        if (ctx) {
            LayoutControls(hwnd, ctx, LOWORD(lp), HIWORD(lp));
        }
        return 0;

    case WM_DPICHANGED:
        if (ctx) {
            ctx->dpi = HIWORD(wp);
            UpdateFonts(ctx);
            auto* suggested = reinterpret_cast<RECT*>(lp);
            SetWindowPos(hwnd, nullptr,
                         suggested->left, suggested->top,
                         suggested->right - suggested->left,
                         suggested->bottom - suggested->top,
                         SWP_NOZORDER | SWP_NOACTIVATE);
            RECT rc{};
            GetClientRect(hwnd, &rc);
            LayoutControls(hwnd, ctx, rc.right - rc.left, rc.bottom - rc.top);
        }
        return 0;

    case WM_GETMINMAXINFO: {
        auto* mmi = reinterpret_cast<MINMAXINFO*>(lp);
        const int dpi = ctx ? ctx->dpi : 96;
        mmi->ptMinTrackSize.x = ScaleForDpi(kMinHelpWidth, dpi);
        mmi->ptMinTrackSize.y = ScaleForDpi(kMinHelpHeight, dpi);
        return 0;
    }

    case WM_COMMAND:
        if (LOWORD(wp) == IDC_HELP_CLOSE) {
            DestroyWindow(hwnd);
            return 0;
        }
        break;

    case WM_CLOSE:
        DestroyWindow(hwnd);
        return 0;

    case WM_DESTROY:
        if (ctx) {
            if (ctx->bodyFont) DeleteObject(ctx->bodyFont);
            if (ctx->buttonFont) DeleteObject(ctx->buttonFont);
            delete ctx;
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
        }
        if (g_helpWindow == hwnd) {
            g_helpWindow = nullptr;
        }
        return 0;
    }

    return DefWindowProcW(hwnd, msg, wp, lp);
}

} // namespace

bool RegisterHelpClass(HINSTANCE hInst)
{
    WNDCLASSEXW wc = { sizeof(wc) };
    wc.lpfnWndProc = HelpWndProc;
    wc.hInstance = hInst;
    wc.hCursor = LoadCursorW(nullptr, IDC_ARROW);
    wc.hIcon = static_cast<HICON>(LoadImageW(hInst, MAKEINTRESOURCEW(IDI_APP),
                                             IMAGE_ICON,
                                             GetSystemMetrics(SM_CXICON),
                                             GetSystemMetrics(SM_CYICON),
                                             LR_DEFAULTCOLOR));
    wc.hIconSm = static_cast<HICON>(LoadImageW(hInst, MAKEINTRESOURCEW(IDI_APP),
                                               IMAGE_ICON,
                                               GetSystemMetrics(SM_CXSMICON),
                                               GetSystemMetrics(SM_CYSMICON),
                                               LR_DEFAULTCOLOR));
    wc.hbrBackground = reinterpret_cast<HBRUSH>(COLOR_WINDOW + 1);
    wc.lpszClassName = kHelpClassName;
    return RegisterClassExW(&wc) != 0 || GetLastError() == ERROR_CLASS_ALREADY_EXISTS;
}

void ShowHelpWindow(HINSTANCE hInst, HWND hParent, const AppSettings& settings)
{
    if (g_helpWindow && IsWindow(g_helpWindow)) {
        auto* ctx = reinterpret_cast<HelpCtx*>(GetWindowLongPtrW(g_helpWindow, GWLP_USERDATA));
        if (ctx) {
            ctx->settings = &settings;
            UpdateHelpText(ctx);
        }
        FocusExistingHelpWindow();
        return;
    }

    auto* ctx = new HelpCtx{};
    ctx->settings = &settings;

    const int referenceDpi = GetDpiForWindow(hParent ? hParent : GetDesktopWindow());
    const int width = ScaleForDpi(kHelpWidth, referenceDpi);
    const int height = ScaleForDpi(kHelpHeight, referenceDpi);
    const RECT rc = GetInitialWindowRect(hParent, width, height);

    HWND hwnd = CreateWindowExW(
        WS_EX_DLGMODALFRAME | WS_EX_CONTROLPARENT,
        kHelpClassName,
        Tr(Str::HelpTitle),
        WS_OVERLAPPEDWINDOW & ~WS_MAXIMIZEBOX,
        rc.left, rc.top,
        rc.right - rc.left,
        rc.bottom - rc.top,
        hParent,
        nullptr,
        hInst,
        ctx);

    if (!hwnd) {
        delete ctx;
        return;
    }

    g_helpWindow = hwnd;
    ShowWindow(hwnd, SW_SHOW);
    UpdateWindow(hwnd);
    FocusExistingHelpWindow();
}
