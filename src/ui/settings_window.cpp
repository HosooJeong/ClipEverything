#include "settings_window.h"
#include "../services/startup_service.h"
#include "../resources/resource.h"
#include <commctrl.h>
#include <algorithm>

#ifdef min
#undef min
#endif
#ifdef max
#undef max
#endif

#pragma comment(lib, "comctl32.lib")

static constexpr wchar_t kClass[] = L"ClipEverythingSettings";

namespace {
constexpr int kWindowWidthBase  = 428;
constexpr int kWindowHeightBase = 390;
constexpr int kOuterPadBase     = 12;
constexpr int kSectionGapBase   = 10;
constexpr int kGeneralHeightBase = 98;
constexpr int kHotkeyHeightBase  = 104;
constexpr int kDataHeightBase    = 72;
constexpr int kFooterHeightBase  = 28;
}

struct SettingsCtx {
    AppSettings* settings = nullptr;
    Repository*  repo = nullptr;
    std::function<void(const HotkeyConfig&)> onHotkeyChanged;
    HINSTANCE hInst = nullptr;
    HotkeyConfig pendingConfig;
    bool capturedCopy  = false;
    bool capturedPaste = false;
    int dpi = 96;

    HFONT bodyFont = nullptr;
    HFONT noteFont = nullptr;

    RECT generalRect = {};
    RECT hotkeyRect  = {};
    RECT dataRect    = {};
    RECT footerRect  = {};

    HWND hGeneralGroup = nullptr;
    HWND hStartupCheck = nullptr;
    HWND hToastCheck   = nullptr;
    HWND hOpenOverlayCheck = nullptr;

    HWND hHotkeyGroup = nullptr;
    HWND hCopyLabel   = nullptr;
    HWND hCopyEdit    = nullptr;
    HWND hCopyReset   = nullptr;
    HWND hPasteLabel  = nullptr;
    HWND hPasteEdit   = nullptr;
    HWND hPasteReset  = nullptr;

    HWND hDataGroup = nullptr;
    HWND hDataNote  = nullptr;
    HWND hClearAll  = nullptr;

    HWND hFooterNote = nullptr;
    HWND hClose      = nullptr;
};

static int S(const SettingsCtx* ctx, int n)
{
    return MulDiv(n, ctx->dpi, 96);
}

static HFONT CreateUiFont(const SettingsCtx* ctx, int sizePx, LONG weight)
{
    LOGFONTW lf = {};
    lf.lfHeight = -S(ctx, sizePx);
    lf.lfWeight = weight;
    lf.lfQuality = CLEARTYPE_QUALITY;
    wcscpy_s(lf.lfFaceName, L"Segoe UI");
    return CreateFontIndirectW(&lf);
}

template <typename T>
static void DeleteGdi(T& object)
{
    if (object) {
        DeleteObject(object);
        object = nullptr;
    }
}

static void RecreateFonts(SettingsCtx* ctx)
{
    DeleteGdi(ctx->bodyFont);
    DeleteGdi(ctx->noteFont);

    ctx->bodyFont = CreateUiFont(ctx, 13, FW_NORMAL);
    ctx->noteFont = CreateUiFont(ctx, 12, FW_NORMAL);
}

static void ApplyControlFonts(const SettingsCtx* ctx)
{
    const HWND bodyControls[] = {
        ctx->hGeneralGroup, ctx->hStartupCheck, ctx->hToastCheck, ctx->hOpenOverlayCheck,
        ctx->hHotkeyGroup, ctx->hCopyLabel, ctx->hCopyEdit, ctx->hCopyReset,
        ctx->hPasteLabel, ctx->hPasteEdit, ctx->hPasteReset,
        ctx->hDataGroup, ctx->hDataNote, ctx->hClearAll,
        ctx->hFooterNote, ctx->hClose
    };

    for (HWND control : bodyControls) {
        if (control)
            SendMessageW(control, WM_SETFONT, (WPARAM)ctx->bodyFont, TRUE);
    }

    if (ctx->hDataNote)
        SendMessageW(ctx->hDataNote, WM_SETFONT, (WPARAM)ctx->noteFont, TRUE);
    if (ctx->hFooterNote)
        SendMessageW(ctx->hFooterNote, WM_SETFONT, (WPARAM)ctx->noteFont, TRUE);
}

static RECT GetInitialWindowRect(HWND hParent, int width, int height)
{
    RECT workArea = {};
    SystemParametersInfoW(SPI_GETWORKAREA, 0, &workArea, 0);

    int x = workArea.left + (workArea.right - workArea.left - width) / 2;
    int y = workArea.top + (workArea.bottom - workArea.top - height) / 2;

    if (hParent && IsWindow(hParent) && IsWindowVisible(hParent)) {
        RECT parentRect = {};
        if (GetWindowRect(hParent, &parentRect) &&
            parentRect.right > parentRect.left &&
            parentRect.bottom > parentRect.top) {
            x = parentRect.left + ((parentRect.right - parentRect.left) - width) / 2;
            y = parentRect.top + ((parentRect.bottom - parentRect.top) - height) / 2;
        }
    }

    const int minX = (int)workArea.left;
    const int maxX = (int)workArea.right - width;
    const int minY = (int)workArea.top;
    const int maxY = (int)workArea.bottom - height;

    x = std::max(minX, std::min(x, maxX));
    y = std::max(minY, std::min(y, maxY));

    RECT result = { x, y, x + width, y + height };
    return result;
}

static void LayoutControls(SettingsCtx* ctx, int clientW, int clientH)
{
    const int pad = S(ctx, kOuterPadBase);
    const int gap = S(ctx, kSectionGapBase);
    const int generalH = S(ctx, kGeneralHeightBase);
    const int hotkeyH = S(ctx, kHotkeyHeightBase);
    const int dataH = S(ctx, kDataHeightBase);
    const int footerH = S(ctx, kFooterHeightBase);
    const int innerPad = S(ctx, 14);

    int y = pad;
    ctx->generalRect = { pad, y, clientW - pad, y + generalH };
    y = ctx->generalRect.bottom + gap;

    ctx->hotkeyRect = { pad, y, clientW - pad, y + hotkeyH };
    y = ctx->hotkeyRect.bottom + gap;

    ctx->dataRect = { pad, y, clientW - pad, y + dataH };
    y = ctx->dataRect.bottom + gap;

    ctx->footerRect = { pad, y, clientW - pad, std::min(clientH - pad, y + footerH) };

    SetWindowPos(ctx->hGeneralGroup, nullptr,
                 ctx->generalRect.left, ctx->generalRect.top,
                 ctx->generalRect.right - ctx->generalRect.left,
                 ctx->generalRect.bottom - ctx->generalRect.top,
                 SWP_NOZORDER | SWP_NOACTIVATE);

    const int checkboxX = ctx->generalRect.left + innerPad;
    const int checkboxW = (ctx->generalRect.right - ctx->generalRect.left) - innerPad * 2;
    SetWindowPos(ctx->hStartupCheck, nullptr,
                 checkboxX, ctx->generalRect.top + S(ctx, 22),
                 checkboxW, S(ctx, 20),
                 SWP_NOZORDER | SWP_NOACTIVATE);
    SetWindowPos(ctx->hToastCheck, nullptr,
                 checkboxX, ctx->generalRect.top + S(ctx, 44),
                 checkboxW, S(ctx, 20),
                 SWP_NOZORDER | SWP_NOACTIVATE);
    SetWindowPos(ctx->hOpenOverlayCheck, nullptr,
                 checkboxX, ctx->generalRect.top + S(ctx, 66),
                 checkboxW, S(ctx, 20),
                 SWP_NOZORDER | SWP_NOACTIVATE);

    SetWindowPos(ctx->hHotkeyGroup, nullptr,
                 ctx->hotkeyRect.left, ctx->hotkeyRect.top,
                 ctx->hotkeyRect.right - ctx->hotkeyRect.left,
                 ctx->hotkeyRect.bottom - ctx->hotkeyRect.top,
                 SWP_NOZORDER | SWP_NOACTIVATE);

    const int labelW = S(ctx, 84);
    const int buttonW = S(ctx, 72);
    const int fieldGap = S(ctx, 8);
    const int fieldX = ctx->hotkeyRect.left + innerPad + labelW + fieldGap;
    const int fieldW = ctx->hotkeyRect.right - innerPad - fieldX - fieldGap - buttonW;
    const int rowH = S(ctx, 24);

    const int copyRowY = ctx->hotkeyRect.top + S(ctx, 24);
    SetWindowPos(ctx->hCopyLabel, nullptr,
                 ctx->hotkeyRect.left + innerPad, copyRowY + S(ctx, 3),
                 labelW, S(ctx, 18),
                 SWP_NOZORDER | SWP_NOACTIVATE);
    SetWindowPos(ctx->hCopyEdit, nullptr,
                 fieldX, copyRowY,
                 fieldW, rowH,
                 SWP_NOZORDER | SWP_NOACTIVATE);
    SetWindowPos(ctx->hCopyReset, nullptr,
                 fieldX + fieldW + fieldGap, copyRowY,
                 buttonW, rowH,
                 SWP_NOZORDER | SWP_NOACTIVATE);

    const int pasteRowY = ctx->hotkeyRect.top + S(ctx, 56);
    SetWindowPos(ctx->hPasteLabel, nullptr,
                 ctx->hotkeyRect.left + innerPad, pasteRowY + S(ctx, 3),
                 labelW, S(ctx, 18),
                 SWP_NOZORDER | SWP_NOACTIVATE);
    SetWindowPos(ctx->hPasteEdit, nullptr,
                 fieldX, pasteRowY,
                 fieldW, rowH,
                 SWP_NOZORDER | SWP_NOACTIVATE);
    SetWindowPos(ctx->hPasteReset, nullptr,
                 fieldX + fieldW + fieldGap, pasteRowY,
                 buttonW, rowH,
                 SWP_NOZORDER | SWP_NOACTIVATE);

    SetWindowPos(ctx->hDataGroup, nullptr,
                 ctx->dataRect.left, ctx->dataRect.top,
                 ctx->dataRect.right - ctx->dataRect.left,
                 ctx->dataRect.bottom - ctx->dataRect.top,
                 SWP_NOZORDER | SWP_NOACTIVATE);

    const int clearW = S(ctx, 118);
    const int clearH = S(ctx, 24);
    const int noteX = ctx->dataRect.left + innerPad;
    const int noteW = ctx->dataRect.right - ctx->dataRect.left - innerPad * 2 - clearW - S(ctx, 10);
    SetWindowPos(ctx->hDataNote, nullptr,
                 noteX, ctx->dataRect.top + S(ctx, 25),
                 noteW, S(ctx, 30),
                 SWP_NOZORDER | SWP_NOACTIVATE);
    SetWindowPos(ctx->hClearAll, nullptr,
                 ctx->dataRect.right - innerPad - clearW,
                 ctx->dataRect.top + S(ctx, 26),
                 clearW, clearH,
                 SWP_NOZORDER | SWP_NOACTIVATE);

    const int closeW = S(ctx, 84);
    const int closeH = S(ctx, 26);
    SetWindowPos(ctx->hFooterNote, nullptr,
                 ctx->footerRect.left, ctx->footerRect.top + S(ctx, 4),
                 ctx->footerRect.right - ctx->footerRect.left - closeW - S(ctx, 10),
                 S(ctx, 18),
                 SWP_NOZORDER | SWP_NOACTIVATE);
    SetWindowPos(ctx->hClose, nullptr,
                 ctx->footerRect.right - closeW, ctx->footerRect.top,
                 closeW, closeH,
                 SWP_NOZORDER | SWP_NOACTIVATE);
}

static LRESULT CALLBACK HotkeyEditSubclass(HWND hwnd, UINT msg, WPARAM wp, LPARAM lp,
                                           UINT_PTR id, DWORD_PTR data)
{
    auto* ctx = reinterpret_cast<SettingsCtx*>(data);
    const bool isCopy = (id == IDC_HOTKEY_COPY);
    bool& captured = isCopy ? ctx->capturedCopy : ctx->capturedPaste;
    const std::wstring& prevLabel = isCopy ? ctx->pendingConfig.copyLabel
                                           : ctx->pendingConfig.pasteLabel;

    switch (msg) {
        case WM_SETFOCUS:
            SetWindowTextW(hwnd, L"키를 누르세요...");
            captured = false;
            return 0;

        case WM_KILLFOCUS:
            if (!captured)
                SetWindowTextW(hwnd, prevLabel.c_str());
            return 0;

        case WM_KEYDOWN:
        case WM_SYSKEYDOWN: {
            const UINT vk = (UINT)wp;
            if (vk == VK_CONTROL || vk == VK_SHIFT || vk == VK_MENU ||
                vk == VK_LWIN || vk == VK_RWIN) {
                break;
            }

            if (vk == VK_ESCAPE) {
                captured = false;
                SetWindowTextW(hwnd, prevLabel.c_str());
                if (ctx->hClose) SetFocus(ctx->hClose);
                return 0;
            }

            UINT mods = MOD_NOREPEAT;
            if (GetKeyState(VK_CONTROL) & 0x8000) mods |= MOD_CONTROL;
            if (GetKeyState(VK_SHIFT) & 0x8000)   mods |= MOD_SHIFT;
            if (GetKeyState(VK_MENU) & 0x8000)    mods |= MOD_ALT;
            if ((GetKeyState(VK_LWIN) & 0x8000) ||
                (GetKeyState(VK_RWIN) & 0x8000))  mods |= MOD_WIN;

            if (!(mods & (MOD_CONTROL | MOD_SHIFT | MOD_ALT | MOD_WIN)))
                break;

            std::wstring label;
            if (mods & MOD_WIN)     label += L"Win+";
            if (mods & MOD_CONTROL) label += L"Ctrl+";
            if (mods & MOD_ALT)     label += L"Alt+";
            if (mods & MOD_SHIFT)   label += L"Shift+";

            wchar_t keyName[32] = {};
            const UINT sc = MapVirtualKeyW(vk, MAPVK_VK_TO_VSC);
            GetKeyNameTextW((LONG)(sc << 16), keyName, 32);
            if (keyName[0]) label += keyName;
            else            label += (wchar_t)vk;

            SetWindowTextW(hwnd, label.c_str());

            if (isCopy) {
                ctx->pendingConfig.copyMods  = mods;
                ctx->pendingConfig.copyVk    = vk;
                ctx->pendingConfig.copyLabel = label;
            } else {
                ctx->pendingConfig.pasteMods  = mods;
                ctx->pendingConfig.pasteVk    = vk;
                ctx->pendingConfig.pasteLabel = label;
            }
            captured = true;
            return 0;
        }

        case WM_GETDLGCODE:
            return DLGC_WANTALLKEYS;
    }

    return DefSubclassProc(hwnd, msg, wp, lp);
}

static HWND MakeControl(SettingsCtx* ctx, HWND hwnd, const wchar_t* cls, const wchar_t* text,
                        DWORD style, DWORD exStyle, UINT id)
{
    return CreateWindowExW(
        exStyle, cls, text,
        WS_CHILD | WS_VISIBLE | style,
        0, 0, 0, 0,
        hwnd, id ? (HMENU)(UINT_PTR)id : nullptr, ctx->hInst, nullptr);
}

static void CreateControls(HWND hwnd, SettingsCtx* ctx)
{
    ctx->hGeneralGroup = MakeControl(ctx, hwnd, L"BUTTON", L"일반", BS_GROUPBOX, 0, 0);
    ctx->hStartupCheck = MakeControl(ctx, hwnd, L"BUTTON", L"Windows 시작 시 자동 실행",
                                     BS_AUTOCHECKBOX | WS_TABSTOP, 0, IDC_STARTUP_CHECK);
    ctx->hToastCheck = MakeControl(ctx, hwnd, L"BUTTON", L"복사 시 토스트 알림",
                                   BS_AUTOCHECKBOX | WS_TABSTOP, 0, IDC_TOAST_CHECK);
    ctx->hOpenOverlayCheck = MakeControl(ctx, hwnd, L"BUTTON", L"복사 후 오버레이 열고 이름 입력",
                                         BS_AUTOCHECKBOX | WS_TABSTOP, 0, IDC_OPEN_OVERLAY_CHECK);

    ctx->hHotkeyGroup = MakeControl(ctx, hwnd, L"BUTTON", L"단축키", BS_GROUPBOX, 0, 0);
    ctx->hCopyLabel = MakeControl(ctx, hwnd, L"STATIC", L"복사", SS_LEFT, 0, 0);
    ctx->hCopyEdit = MakeControl(ctx, hwnd, L"EDIT", ctx->settings->copyLabel.c_str(),
                                 ES_AUTOHSCROLL | ES_CENTER | ES_READONLY | WS_TABSTOP,
                                 WS_EX_CLIENTEDGE, IDC_HOTKEY_COPY);
    ctx->hCopyReset = MakeControl(ctx, hwnd, L"BUTTON", L"초기화",
                                  BS_PUSHBUTTON | WS_TABSTOP, 0, IDC_RESET_COPY);
    ctx->hPasteLabel = MakeControl(ctx, hwnd, L"STATIC", L"붙여넣기", SS_LEFT, 0, 0);
    ctx->hPasteEdit = MakeControl(ctx, hwnd, L"EDIT", ctx->settings->pasteLabel.c_str(),
                                  ES_AUTOHSCROLL | ES_CENTER | ES_READONLY | WS_TABSTOP,
                                  WS_EX_CLIENTEDGE, IDC_HOTKEY_PASTE);
    ctx->hPasteReset = MakeControl(ctx, hwnd, L"BUTTON", L"초기화",
                                   BS_PUSHBUTTON | WS_TABSTOP, 0, IDC_RESET_PASTE);

    ctx->hDataGroup = MakeControl(ctx, hwnd, L"BUTTON", L"데이터 관리", BS_GROUPBOX, 0, 0);
    ctx->hDataNote = MakeControl(ctx, hwnd, L"STATIC",
                                 L"저장된 클립 기록만 삭제합니다.\r\n앱 설정은 유지됩니다.",
                                 SS_LEFT, 0, 0);
    ctx->hClearAll = MakeControl(ctx, hwnd, L"BUTTON", L"모든 클립 삭제",
                                 BS_PUSHBUTTON | WS_TABSTOP, 0, IDC_CLEAR_ALL);

    ctx->hFooterNote = MakeControl(ctx, hwnd, L"STATIC",
                                   L"단축키 변경은 닫기 시 적용됩니다.",
                                   SS_LEFT, 0, 0);
    ctx->hClose = MakeControl(ctx, hwnd, L"BUTTON", L"닫기",
                              BS_DEFPUSHBUTTON | WS_TABSTOP, 0, IDC_SETTINGS_CLOSE);

    CheckDlgButton(hwnd, IDC_STARTUP_CHECK,
                   IsStartupRegistered() ? BST_CHECKED : BST_UNCHECKED);
    CheckDlgButton(hwnd, IDC_TOAST_CHECK,
                   ctx->settings->showToastNotifications ? BST_CHECKED : BST_UNCHECKED);
    CheckDlgButton(hwnd, IDC_OPEN_OVERLAY_CHECK,
                   ctx->settings->openOverlayAfterCopy ? BST_CHECKED : BST_UNCHECKED);

    SetWindowSubclass(ctx->hCopyEdit, HotkeyEditSubclass, IDC_HOTKEY_COPY, (DWORD_PTR)ctx);
    SetWindowSubclass(ctx->hPasteEdit, HotkeyEditSubclass, IDC_HOTKEY_PASTE, (DWORD_PTR)ctx);

    ApplyControlFonts(ctx);
}

static void DestroyFonts(SettingsCtx* ctx)
{
    DeleteGdi(ctx->bodyFont);
    DeleteGdi(ctx->noteFont);
}

static LRESULT CALLBACK SettingsWndProc(HWND hwnd, UINT msg, WPARAM wp, LPARAM lp)
{
    auto* ctx = reinterpret_cast<SettingsCtx*>(GetWindowLongPtrW(hwnd, GWLP_USERDATA));

    switch (msg) {
        case WM_CREATE: {
            auto* cs = reinterpret_cast<CREATESTRUCTW*>(lp);
            ctx = reinterpret_cast<SettingsCtx*>(cs->lpCreateParams);
            ctx->dpi = GetDpiForWindow(hwnd);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, (LONG_PTR)ctx);

            ctx->pendingConfig.copyMods   = ctx->settings->copyMods;
            ctx->pendingConfig.copyVk     = ctx->settings->copyVk;
            ctx->pendingConfig.copyLabel  = ctx->settings->copyLabel;
            ctx->pendingConfig.pasteMods  = ctx->settings->pasteMods;
            ctx->pendingConfig.pasteVk    = ctx->settings->pasteVk;
            ctx->pendingConfig.pasteLabel = ctx->settings->pasteLabel;

            RecreateFonts(ctx);
            CreateControls(hwnd, ctx);

            RECT rc = {};
            GetClientRect(hwnd, &rc);
            LayoutControls(ctx, rc.right, rc.bottom);
            return 0;
        }

        case WM_SIZE:
            if (ctx)
                LayoutControls(ctx, LOWORD(lp), HIWORD(lp));
            return 0;

        case WM_DPICHANGED:
            if (ctx) {
                ctx->dpi = HIWORD(wp);
                RecreateFonts(ctx);
                ApplyControlFonts(ctx);

                if (const RECT* suggested = reinterpret_cast<const RECT*>(lp)) {
                    SetWindowPos(hwnd, nullptr,
                                 suggested->left, suggested->top,
                                 suggested->right - suggested->left,
                                 suggested->bottom - suggested->top,
                                 SWP_NOZORDER | SWP_NOACTIVATE);
                }
            }
            return 0;

        case WM_COMMAND:
            if (!ctx) return 0;
            switch (LOWORD(wp)) {
                case IDC_STARTUP_CHECK:
                    SetStartupEnabled(IsDlgButtonChecked(hwnd, IDC_STARTUP_CHECK) == BST_CHECKED);
                    ctx->settings->runAtStartup =
                        IsDlgButtonChecked(hwnd, IDC_STARTUP_CHECK) == BST_CHECKED;
                    ctx->settings->Save();
                    break;

                case IDC_TOAST_CHECK:
                    ctx->settings->showToastNotifications =
                        IsDlgButtonChecked(hwnd, IDC_TOAST_CHECK) == BST_CHECKED;
                    ctx->settings->Save();
                    break;

                case IDC_OPEN_OVERLAY_CHECK:
                    ctx->settings->openOverlayAfterCopy =
                        IsDlgButtonChecked(hwnd, IDC_OPEN_OVERLAY_CHECK) == BST_CHECKED;
                    ctx->settings->Save();
                    break;

                case IDC_CLEAR_ALL:
                    if (MessageBoxW(hwnd, L"모든 클립보드 기록을 삭제하시겠습니까?",
                                    L"확인", MB_YESNO | MB_ICONQUESTION) == IDYES) {
                        ctx->repo->DeleteAll();
                    }
                    break;

                case IDC_RESET_COPY:
                    ctx->pendingConfig.copyMods  = MOD_WIN | MOD_CONTROL | MOD_NOREPEAT;
                    ctx->pendingConfig.copyVk    = 'C';
                    ctx->pendingConfig.copyLabel = L"Win+Ctrl+C";
                    SetDlgItemTextW(hwnd, IDC_HOTKEY_COPY, L"Win+Ctrl+C");
                    ctx->capturedCopy = true;
                    break;

                case IDC_RESET_PASTE:
                    ctx->pendingConfig.pasteMods  = MOD_WIN | MOD_CONTROL | MOD_NOREPEAT;
                    ctx->pendingConfig.pasteVk    = 'V';
                    ctx->pendingConfig.pasteLabel = L"Win+Ctrl+V";
                    SetDlgItemTextW(hwnd, IDC_HOTKEY_PASTE, L"Win+Ctrl+V");
                    ctx->capturedPaste = true;
                    break;

                case IDC_SETTINGS_CLOSE:
                    SendMessageW(hwnd, WM_CLOSE, 0, 0);
                    break;
            }
            return 0;

        // X 버튼·닫기 버튼 모두 같은 경로: 변경된 단축키를 적용하고 닫는다
        case WM_CLOSE:
            if (ctx && ctx->onHotkeyChanged)
                ctx->onHotkeyChanged(ctx->pendingConfig);
            DestroyWindow(hwnd);
            return 0;

        case WM_DESTROY:
            if (ctx) {
                DestroyFonts(ctx);
                delete ctx;
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            }
            return 0;
    }

    return DefWindowProcW(hwnd, msg, wp, lp);
}

bool RegisterSettingsClass(HINSTANCE hInst)
{
    WNDCLASSEXW wc = { sizeof(wc) };
    wc.lpfnWndProc   = SettingsWndProc;
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
    wc.hCursor       = LoadCursorW(nullptr, IDC_ARROW);
    wc.hbrBackground = (HBRUSH)(COLOR_BTNFACE + 1);
    wc.lpszClassName = kClass;
    return RegisterClassExW(&wc) != 0;
}

void ShowSettingsWindow(HINSTANCE hInst, HWND hParent,
                        AppSettings& settings, Repository& repo,
                        std::function<void(const HotkeyConfig&)> onHotkeyChanged)
{
    auto* ctx = new SettingsCtx{ &settings, &repo, onHotkeyChanged, hInst };
    ctx->dpi = GetDpiForWindow(hParent ? hParent : GetDesktopWindow());

    const int width = MulDiv(kWindowWidthBase, ctx->dpi, 96);
    const int height = MulDiv(kWindowHeightBase, ctx->dpi, 96);
    const RECT rc = GetInitialWindowRect(hParent, width, height);

    HWND hwnd = CreateWindowExW(
        WS_EX_DLGMODALFRAME | WS_EX_CONTROLPARENT | WS_EX_TOPMOST,
        kClass, L"ClipEverything 설정",
        WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU,
        rc.left, rc.top, rc.right - rc.left, rc.bottom - rc.top,
        hParent, nullptr, hInst, ctx);

    ShowWindow(hwnd, SW_SHOW);
}
