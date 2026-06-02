#include "toast_popup.h"
#include "../resources/resource.h"
#include "theme.h"
#include "render/d2d_context.h"
#include <d2d1.h>
#include <dwrite.h>

#pragma comment(lib, "d2d1.lib")
#pragma comment(lib, "dwrite.lib")

static constexpr wchar_t kToastClass[] = L"ClipEverythingToast";
static HWND g_currentToast = nullptr;

struct ToastData {
    std::wstring title;
    std::wstring subtitle;
    int fadeStep  = 0;      // 0..15: fade in, pause, fade out
    bool fadingOut = false;
    HINSTANCE hInst;
    ComPtr<ID2D1DCRenderTarget> dcRt;
};

static void RenderToast(HWND hwnd, ToastData* td, int alpha)
{
    RECT rc; GetClientRect(hwnd, &rc);
    int w = rc.right, h = rc.bottom;

    HDC hdc = GetDC(hwnd);
    HDC memDC = CreateCompatibleDC(hdc);
    BITMAPINFO bmi = {};
    bmi.bmiHeader.biSize        = sizeof(BITMAPINFOHEADER);
    bmi.bmiHeader.biWidth       = w;
    bmi.bmiHeader.biHeight      = -h;
    bmi.bmiHeader.biPlanes      = 1;
    bmi.bmiHeader.biBitCount    = 32;
    bmi.bmiHeader.biCompression = BI_RGB;
    void* bits = nullptr;
    HBITMAP hBmp = CreateDIBSection(memDC, &bmi, DIB_RGB_COLORS, &bits, nullptr, 0);
    SelectObject(memDC, hBmp);

    // Direct2D DC 렌더링
    auto& ctx = D2DContext::Get();
    if (!td->dcRt) td->dcRt = ctx.CreateDCRenderTarget();

    if (td->dcRt) {
        RECT bindRc = { 0, 0, w, h };
        td->dcRt->BindDC(memDC, &bindRc);
        td->dcRt->BeginDraw();
        td->dcRt->Clear(D2D1::ColorF(0, 0));

        // 배경 (반투명 어두운 라운드 박스)
        ComPtr<ID2D1SolidColorBrush> bgBrush;
        td->dcRt->CreateSolidColorBrush(Theme::ToastBg(), &bgBrush);
        D2D1_ROUNDED_RECT rr = { D2D1::RectF(2, 2, (float)w-2, (float)h-2), 10, 10 };
        td->dcRt->FillRoundedRectangle(rr, bgBrush.Get());

        // 제목
        ComPtr<ID2D1SolidColorBrush> titleBrush, subBrush;
        td->dcRt->CreateSolidColorBrush(Theme::ToastText(), &titleBrush);
        td->dcRt->CreateSolidColorBrush(Theme::ToastSub(),  &subBrush);

        auto* tf13 = ctx.GetTextFormat(Theme::FluentFont, 13.0f, DWRITE_FONT_WEIGHT_SEMI_BOLD);
        auto* tf11 = ctx.GetTextFormat(Theme::FluentFont, 11.0f);

        if (tf13) {
            td->dcRt->DrawTextW(td->title.c_str(), (UINT32)td->title.length(),
                                 tf13, D2D1::RectF(14, 8, (float)w-14, 30),
                                 titleBrush.Get());
        }
        if (tf11) {
            td->dcRt->DrawTextW(td->subtitle.c_str(), (UINT32)td->subtitle.length(),
                                 tf11, D2D1::RectF(14, 30, (float)w-14, 54),
                                 subBrush.Get());
        }

        td->dcRt->EndDraw();
    }

    // UpdateLayeredWindow
    POINT ptSrc = { 0, 0 };
    SIZE sz = { w, h };
    BLENDFUNCTION bf = { AC_SRC_OVER, 0, (BYTE)alpha, AC_SRC_ALPHA };
    POINT ptDest = {};
    GetWindowRect(hwnd, reinterpret_cast<RECT*>(&ptDest));  // 실제로는 별도 계산
    POINT wndPos = {};
    RECT wr; GetWindowRect(hwnd, &wr);
    wndPos = { wr.left, wr.top };
    UpdateLayeredWindow(hwnd, GetDC(nullptr), &wndPos, &sz, memDC, &ptSrc, 0, &bf, ULW_ALPHA);

    DeleteObject(hBmp);
    DeleteDC(memDC);
    ReleaseDC(hwnd, hdc);
}

static LRESULT CALLBACK ToastWndProc(HWND hwnd, UINT msg, WPARAM wp, LPARAM lp)
{
    auto* td = reinterpret_cast<ToastData*>(GetWindowLongPtrW(hwnd, GWLP_USERDATA));
    switch (msg) {
        case WM_CREATE: {
            auto* cs = reinterpret_cast<CREATESTRUCTW*>(lp);
            td = reinterpret_cast<ToastData*>(cs->lpCreateParams);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, (LONG_PTR)td);
            SetTimer(hwnd, 1, 20, nullptr);  // 50fps
            return 0;
        }
        case WM_TIMER: {
            if (!td) break;
            td->fadeStep++;

            int alpha;
            if (td->fadeStep <= 10) {
                // Fade in (200ms)
                alpha = (int)(td->fadeStep * 25.5f);
            } else if (td->fadeStep <= 135) {
                // 유지 (2500ms)
                alpha = 255;
            } else if (td->fadeStep <= 150) {
                // Fade out (300ms)
                alpha = (int)((150 - td->fadeStep) * 17.0f);
            } else {
                // 완료
                KillTimer(hwnd, 1);
                g_currentToast = nullptr;
                DestroyWindow(hwnd);
                return 0;
            }
            RenderToast(hwnd, td, alpha);
            return 0;
        }
        case WM_DESTROY:
            KillTimer(hwnd, 1);
            delete td;
            if (g_currentToast == hwnd) g_currentToast = nullptr;
            return 0;
    }
    return DefWindowProcW(hwnd, msg, wp, lp);
}

bool RegisterToastClass(HINSTANCE hInst)
{
    WNDCLASSEXW wc = { sizeof(wc) };
    wc.lpfnWndProc   = ToastWndProc;
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
    wc.lpszClassName = kToastClass;
    return RegisterClassExW(&wc) != 0;
}

void ShowToastPopup(HINSTANCE hInst, const ClipboardItem& item)
{
    // 기존 토스트 제거
    if (g_currentToast) { DestroyWindow(g_currentToast); g_currentToast = nullptr; }

    // 위치: 화면 우하단
    RECT wa; SystemParametersInfoW(SPI_GETWORKAREA, 0, &wa, 0);
    int tw = 280, th = 72;
    int tx = wa.right - tw - 16, ty = wa.bottom - th - 16;

    auto* td = new ToastData();
    td->hInst    = hInst;
    td->title    = item.sourceApp + L"에서 복사됨";
    td->subtitle = item.DisplayName();
    if (td->subtitle.length() > 45) td->subtitle = td->subtitle.substr(0, 42) + L"...";

    g_currentToast = CreateWindowExW(
        WS_EX_LAYERED | WS_EX_NOACTIVATE | WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
        kToastClass, nullptr, WS_POPUP,
        tx, ty, tw, th,
        nullptr, nullptr, hInst, td);

    if (g_currentToast) ShowWindow(g_currentToast, SW_SHOWNOACTIVATE);
}
