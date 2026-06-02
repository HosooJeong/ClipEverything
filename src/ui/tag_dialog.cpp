#include "tag_dialog.h"
#include "../resources/resource.h"
#include <sstream>

static constexpr wchar_t kClass[] = L"ClipEverythingTag";

struct TagCtx {
    std::wstring currentTags;
    std::wstring result;
    bool cancelled = true;
    bool done      = false;
};

// 태그 정규화: # 접두사 보장, 쉼표 구분
static std::wstring NormalizeTags(const std::wstring& raw)
{
    std::wstring out;
    std::wistringstream ss(raw);
    std::wstring token;
    while (std::getline(ss, token, L',')) {
        // 앞뒤 공백 제거
        size_t s = token.find_first_not_of(L" \t");
        size_t e = token.find_last_not_of(L" \t");
        if (s == std::wstring::npos) continue;
        token = token.substr(s, e - s + 1);
        if (token.empty()) continue;
        if (token[0] != L'#') token = L'#' + token;
        if (!out.empty()) out += L",";
        out += token;
    }
    return out;
}

static LRESULT CALLBACK TagWndProc(HWND hwnd, UINT msg, WPARAM wp, LPARAM lp)
{
    auto* ctx = reinterpret_cast<TagCtx*>(GetWindowLongPtrW(hwnd, GWLP_USERDATA));
    switch (msg) {
        case WM_CREATE: {
            auto* cs = reinterpret_cast<CREATESTRUCTW*>(lp);
            ctx = reinterpret_cast<TagCtx*>(cs->lpCreateParams);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, (LONG_PTR)ctx);

            int dpi = GetDpiForWindow(hwnd);
            auto S = [dpi](int n) { return MulDiv(n, dpi, 96); };

            LOGFONTW lf = {}; lf.lfHeight = -S(13);
            wcscpy_s(lf.lfFaceName, L"Segoe UI Variable");
            HFONT hFont = CreateFontIndirectW(&lf);

            HWND hLabel = CreateWindowExW(0, L"STATIC", L"태그 (쉼표로 구분, 예: #업무,#개인)",
                WS_CHILD | WS_VISIBLE | SS_LEFT,
                S(12), S(10), S(280), S(18), hwnd, nullptr, nullptr, nullptr);
            SendMessageW(hLabel, WM_SETFONT, (WPARAM)hFont, TRUE);

            HWND hEdit = CreateWindowExW(WS_EX_CLIENTEDGE, L"EDIT", ctx->currentTags.c_str(),
                WS_CHILD | WS_VISIBLE | ES_AUTOHSCROLL,
                S(12), S(32), S(280), S(24),
                hwnd, (HMENU)(UINT_PTR)IDC_TAG_EDIT, nullptr, nullptr);
            SendMessageW(hEdit, WM_SETFONT, (WPARAM)hFont, TRUE);
            SetFocus(hEdit);

            auto makeBtn = [&](const wchar_t* txt, DWORD style, int x, int y, int w, int h, UINT id) {
                HWND hb = CreateWindowExW(0, L"BUTTON", txt, WS_CHILD | WS_VISIBLE | style,
                    S(x), S(y), S(w), S(h), hwnd, (HMENU)(UINT_PTR)id, nullptr, nullptr);
                SendMessageW(hb, WM_SETFONT, (WPARAM)hFont, TRUE);
            };
            makeBtn(L"저장",     BS_DEFPUSHBUTTON, 12,  68, 80, 26, IDC_TAG_SAVE);
            makeBtn(L"초기화",   BS_PUSHBUTTON,    100, 68, 80, 26, IDC_TAG_CLEAR);
            makeBtn(L"취소",     BS_PUSHBUTTON,    188, 68, 80, 26, IDC_TAG_CANCEL);
            return 0;
        }
        case WM_COMMAND:
            switch (LOWORD(wp)) {
                case IDC_TAG_SAVE: {
                    wchar_t buf[512] = {};
                    GetDlgItemTextW(hwnd, IDC_TAG_EDIT, buf, 512);
                    ctx->result    = NormalizeTags(buf);
                    ctx->cancelled = false;
                    ctx->done      = true;
                    DestroyWindow(hwnd);
                    break;
                }
                case IDC_TAG_CLEAR:
                    SetDlgItemTextW(hwnd, IDC_TAG_EDIT, L"");
                    break;
                case IDC_TAG_CANCEL:
                    ctx->done = true;
                    DestroyWindow(hwnd);
                    break;
            }
            return 0;
        case WM_CLOSE:
            ctx->done = true;
            DestroyWindow(hwnd);
            return 0;
    }
    return DefWindowProcW(hwnd, msg, wp, lp);
}

bool RegisterTagClass(HINSTANCE hInst)
{
    WNDCLASSEXW wc = { sizeof(wc) };
    wc.lpfnWndProc   = TagWndProc;
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

std::wstring ShowTagDialog(HINSTANCE hInst, HWND hParent,
                            const std::wstring& currentTags, bool& isCancelled)
{
    int dpi = GetDpiForWindow(hParent ? hParent : GetDesktopWindow());
    auto S = [dpi](int n) { return MulDiv(n, dpi, 96); };

    TagCtx ctx{ currentTags };

    HWND hwnd = CreateWindowExW(
        WS_EX_DLGMODALFRAME | WS_EX_TOPMOST,
        kClass, L"태그 관리",
        WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU,
        CW_USEDEFAULT, CW_USEDEFAULT, S(320), S(130),
        hParent, nullptr, hInst, &ctx);

    if (hParent) EnableWindow(hParent, FALSE);
    ShowWindow(hwnd, SW_SHOW);

    MSG msg;
    while (!ctx.done && GetMessageW(&msg, nullptr, 0, 0)) {
        if (!IsDialogMessageW(hwnd, &msg)) {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    if (hParent) EnableWindow(hParent, TRUE);
    isCancelled = ctx.cancelled;
    return ctx.result;
}
