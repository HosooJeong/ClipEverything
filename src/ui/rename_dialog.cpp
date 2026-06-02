#include "rename_dialog.h"
#include "../resources/resource.h"

static constexpr wchar_t kClass[] = L"ClipEverythingRename";

struct RenameCtx {
    std::wstring current;
    std::wstring result;
    bool  done = false;
};

static LRESULT CALLBACK RenameWndProc(HWND hwnd, UINT msg, WPARAM wp, LPARAM lp)
{
    auto* ctx = reinterpret_cast<RenameCtx*>(GetWindowLongPtrW(hwnd, GWLP_USERDATA));
    switch (msg) {
        case WM_CREATE: {
            auto* cs = reinterpret_cast<CREATESTRUCTW*>(lp);
            ctx = reinterpret_cast<RenameCtx*>(cs->lpCreateParams);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, (LONG_PTR)ctx);

            int dpi = GetDpiForWindow(hwnd);
            auto S = [dpi](int n) { return MulDiv(n, dpi, 96); };

            LOGFONTW lf = {}; lf.lfHeight = -S(13);
            wcscpy_s(lf.lfFaceName, L"Segoe UI Variable");
            HFONT hFont = CreateFontIndirectW(&lf);

            HWND hEdit = CreateWindowExW(WS_EX_CLIENTEDGE, L"EDIT", ctx->current.c_str(),
                WS_CHILD | WS_VISIBLE | ES_AUTOHSCROLL,
                S(12), S(14), S(260), S(24),
                hwnd, (HMENU)(UINT_PTR)IDC_RENAME_EDIT, nullptr, nullptr);
            SendMessageW(hEdit, WM_SETFONT, (WPARAM)hFont, TRUE);
            SendMessageW(hEdit, EM_SETSEL, 0, -1);
            SetFocus(hEdit);

            HWND hOk = CreateWindowExW(0, L"BUTTON", L"확인",
                WS_CHILD | WS_VISIBLE | BS_DEFPUSHBUTTON,
                S(12), S(50), S(80), S(26),
                hwnd, (HMENU)(UINT_PTR)IDC_RENAME_OK, nullptr, nullptr);
            SendMessageW(hOk, WM_SETFONT, (WPARAM)hFont, TRUE);

            HWND hCancel = CreateWindowExW(0, L"BUTTON", L"취소",
                WS_CHILD | WS_VISIBLE | BS_PUSHBUTTON,
                S(100), S(50), S(80), S(26),
                hwnd, (HMENU)(UINT_PTR)IDC_RENAME_CANCEL, nullptr, nullptr);
            SendMessageW(hCancel, WM_SETFONT, (WPARAM)hFont, TRUE);
            return 0;
        }
        case WM_COMMAND:
            switch (LOWORD(wp)) {
                case IDC_RENAME_OK: {
                    wchar_t buf[512] = {};
                    GetDlgItemTextW(hwnd, IDC_RENAME_EDIT, buf, 512);
                    ctx->result = buf;
                    ctx->done   = true;
                    DestroyWindow(hwnd);
                    break;
                }
                case IDC_RENAME_CANCEL:
                    ctx->done = true;
                    DestroyWindow(hwnd);
                    break;
            }
            return 0;
        case WM_KEYDOWN:
            if (wp == VK_ESCAPE) { ctx->done = true; DestroyWindow(hwnd); }
            return 0;
        case WM_CLOSE:
            ctx->done = true;
            DestroyWindow(hwnd);
            return 0;
    }
    return DefWindowProcW(hwnd, msg, wp, lp);
}

bool RegisterRenameClass(HINSTANCE hInst)
{
    WNDCLASSEXW wc = { sizeof(wc) };
    wc.lpfnWndProc   = RenameWndProc;
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
    wc.hbrBackground = (HBRUSH)(COLOR_WINDOW + 1);
    wc.lpszClassName = kClass;
    return RegisterClassExW(&wc) != 0;
}

std::wstring ShowRenameDialog(HINSTANCE hInst, HWND hParent, const std::wstring& current)
{
    int dpi = GetDpiForWindow(hParent ? hParent : GetDesktopWindow());
    auto S = [dpi](int n) { return MulDiv(n, dpi, 96); };

    RenameCtx ctx{ current };

    HWND hwnd = CreateWindowExW(
        WS_EX_DLGMODALFRAME | WS_EX_TOPMOST,
        kClass, L"이름 변경",
        WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU,
        CW_USEDEFAULT, CW_USEDEFAULT, S(300), S(110),
        hParent, nullptr, hInst, &ctx);

    if (hParent) EnableWindow(hParent, FALSE);
    ShowWindow(hwnd, SW_SHOW);

    // 모달 루프
    MSG msg;
    while (!ctx.done && GetMessageW(&msg, nullptr, 0, 0)) {
        if (!IsDialogMessageW(hwnd, &msg)) {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    if (hParent) EnableWindow(hParent, TRUE);
    return ctx.result;
}
