#include "overlay_window.h"

#include "../services/app_settings.h"
#include "../resources/resource.h"
#include "theme.h"
#include "render/d2d_context.h"

#include <algorithm>
#include <cmath>
#include <cwchar>
#include <cwctype>
#include <imm.h>
#include <sstream>
#include <vector>
#include <windowsx.h>
#include <uxtheme.h>

#pragma comment(lib, "dwmapi.lib")
#pragma comment(lib, "comctl32.lib")
#pragma comment(lib, "imm32.lib")

#ifndef DWMWA_SYSTEMBACKDROP_TYPE
#define DWMWA_SYSTEMBACKDROP_TYPE 38
#endif

#ifndef DWMWA_WINDOW_CORNER_PREFERENCE
#define DWMWA_WINDOW_CORNER_PREFERENCE 33
#endif

#define DWMWCP_ROUND 2
#define DWMSBT_MAINWINDOW 2

#define WCA_ACCENT_POLICY 19
#define ACCENT_ENABLE_ACRYLICBLURBEHIND 4

struct AccentPolicy {
    int AccentState;
    int AccentFlags;
    int GradientColor;
    int AnimationId;
};

struct WinCompAttribData {
    int Attrib;
    void* pvData;
    int cbData;
};

typedef BOOL(WINAPI* SetWinCompAttrFn)(HWND, WinCompAttribData*);

namespace {

constexpr wchar_t kOverlayFontName[] = L"Malgun Gothic";
constexpr COLORREF kInlineEditTextColor = RGB(0, 0, 0);
constexpr COLORREF kInlineEditBgColor = RGB(255, 255, 255);
constexpr COLORREF kInlinePanelBorderColor = RGB(198, 219, 255);
constexpr COLORREF kTagPanelBorderColor = RGB(197, 217, 251);
constexpr COLORREF kTagEditTextColor = RGB(18, 26, 42);
constexpr COLORREF kTagEditBgColor = RGB(232, 240, 254);
constexpr wchar_t kEditPanelClassName[] = L"ClipEverythingInlineEditPanel";
constexpr wchar_t kHeaderTitle[] = L"📋  ClipEverything";
constexpr UINT_PTR kTooltipTimerId = 101;
constexpr int kDefaultOverlayWidth = 420;
constexpr int kDefaultOverlayHeight = 520;
constexpr int kDefaultOverlayBottomMargin = 20;
constexpr int kMinOverlayHeight = 280;
constexpr int kResizeZonePx = 8;

constexpr float kCardPadX = 12.0f;
constexpr float kCardIconSize = 32.0f;
constexpr float kCardTextGap = 8.0f;
constexpr float kActionSize = 18.0f;
constexpr float kActionGap = 4.0f;
constexpr float kActionRightPad = 12.0f;
constexpr float kTitleY = 7.0f;
constexpr float kSubtitleY = 29.0f;
constexpr float kTagChipHeight = 18.0f;
constexpr float kTagChipPadX = 7.0f;
constexpr float kTagChipGap = 4.0f;
constexpr float kTagRemoveGap = 4.0f;
constexpr float kTagRemoveSize = 10.0f;
constexpr float kInlinePanelInsetX = 6.0f;
constexpr float kInlinePanelInsetY = 4.0f;
constexpr float kTooltipPadX = 8.0f;
constexpr float kTooltipPadY = 4.0f;

constexpr wchar_t kGlyphRename[] = L"\uE70F";
constexpr wchar_t kGlyphFavorite[] = L"\uE735";
constexpr wchar_t kGlyphFavoriteOutline[] = L"\uE734";
constexpr wchar_t kGlyphDelete[] = L"\uE74D";
constexpr wchar_t kGlyphClose[] = L"\uE711";
constexpr wchar_t kTagAddLabel[] = L"+ 태그";

struct TagChipLayout {
    D2D1_ROUNDED_RECT bounds{};
    D2D1_RECT_F textRect{};
    D2D1_RECT_F removeRect{};
    int tagIdx = -1;
    std::wstring text;
};

struct CardLayout {
    D2D1_ROUNDED_RECT background{};
    D2D1_RECT_F iconRect{};
    D2D1_RECT_F titleRect{};
    D2D1_RECT_F renameRect{};
    D2D1_RECT_F favoriteRect{};
    D2D1_RECT_F deleteRect{};
    D2D1_RECT_F subtitleRect{};
    D2D1_RECT_F tagAddRect{};
    D2D1_ROUNDED_RECT tagAddBounds{};
    std::wstring tagAddText;
    float contentLeft = 0.0f;
    float contentRight = 0.0f;
    std::vector<TagChipLayout> tagChips;
};

bool IsWin11()
{
    OSVERSIONINFOEXW osvi = { sizeof(osvi) };
    ULONGLONG mask = VerSetConditionMask(0, VER_BUILDNUMBER, VER_GREATER_EQUAL);
    osvi.dwBuildNumber = 22000;
    return VerifyVersionInfoW(&osvi, VER_BUILDNUMBER, mask) != FALSE;
}

std::wstring TrimCopy(const std::wstring& value)
{
    const size_t start = value.find_first_not_of(L" \t\r\n");
    if (start == std::wstring::npos) return L"";
    const size_t end = value.find_last_not_of(L" \t\r\n");
    return value.substr(start, end - start + 1);
}

std::wstring ToLowerCopy(const std::wstring& value)
{
    std::wstring out = value;
    std::transform(out.begin(), out.end(), out.begin(),
                   [](wchar_t ch) { return (wchar_t)towlower(ch); });
    return out;
}

std::wstring Ellipsize(const std::wstring& value, size_t maxLen)
{
    if (value.length() <= maxLen) return value;
    if (maxLen <= 3) return value.substr(0, maxLen);
    return value.substr(0, maxLen - 3) + L"...";
}

std::wstring GetOverlayTitle(const ClipboardItem& item)
{
    if (item.name && !TrimCopy(*item.name).empty())
        return TrimCopy(*item.name);
    return L"관리명 없음";
}

std::wstring GetInlineRenamePrefill(const ClipboardItem& item)
{
    if (item.name && !TrimCopy(*item.name).empty())
        return TrimCopy(*item.name);

    std::wstring sourceWindow = TrimCopy(item.sourceWindow);
    if (!sourceWindow.empty()) return sourceWindow;

    return TrimCopy(item.sourceApp);
}

std::wstring GetOverlaySubtitle(const ClipboardItem& item)
{
    std::wstring subtitle = TrimCopy(item.sourceApp);
    if (!item.lastCopiedAt.empty() && item.lastCopiedAt.length() >= 10) {
        if (!subtitle.empty()) subtitle += L"  ";
        subtitle += item.lastCopiedAt.substr(5, 5);
    }
    return subtitle;
}

std::wstring NormalizeTag(const std::wstring& raw)
{
    std::wstring tag = TrimCopy(raw);
    if (tag.empty()) return L"";
    if (tag.front() != L'#') tag.insert(tag.begin(), L'#');
    return tag;
}

std::vector<std::wstring> ParseTags(const std::wstring& raw)
{
    std::vector<std::wstring> tags;
    std::wistringstream stream(raw);
    std::wstring token;
    while (std::getline(stream, token, L',')) {
        std::wstring normalized = NormalizeTag(token);
        if (normalized.empty()) continue;

        const std::wstring lowered = ToLowerCopy(normalized);
        bool exists = std::any_of(tags.begin(), tags.end(),
                                  [&](const std::wstring& existing) {
                                      return ToLowerCopy(existing) == lowered;
                                  });
        if (!exists) tags.push_back(normalized);
    }
    return tags;
}

std::wstring JoinTags(const std::vector<std::wstring>& tags)
{
    std::wstring joined;
    for (const auto& tag : tags) {
        if (tag.empty()) continue;
        if (!joined.empty()) joined += L",";
        joined += tag;
    }
    return joined;
}

bool PtInRectF(const D2D1_RECT_F& rect, float x, float y)
{
    return x >= rect.left && x <= rect.right &&
           y >= rect.top && y <= rect.bottom;
}

bool PtInRoundedRect(const D2D1_ROUNDED_RECT& rect, float x, float y)
{
    return PtInRectF(rect.rect, x, y);
}

RECT GetMonitorWorkAreaForRect(const RECT& rect)
{
    RECT fallback{};
    SystemParametersInfoW(SPI_GETWORKAREA, 0, &fallback, 0);

    HMONITOR monitor = MonitorFromRect(&rect, MONITOR_DEFAULTTONEAREST);
    if (!monitor)
        return fallback;

    MONITORINFO mi{ sizeof(mi) };
    if (!GetMonitorInfoW(monitor, &mi))
        return fallback;

    return mi.rcWork;
}

RECT GetMonitorWorkAreaForPoint(const POINT& pt)
{
    RECT fallback{};
    SystemParametersInfoW(SPI_GETWORKAREA, 0, &fallback, 0);

    HMONITOR monitor = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
    if (!monitor)
        return fallback;

    MONITORINFO mi{ sizeof(mi) };
    if (!GetMonitorInfoW(monitor, &mi))
        return fallback;

    return mi.rcWork;
}

float MeasureTextWidth(const std::wstring& text, IDWriteTextFormat* format)
{
    if (text.empty()) return 0.0f;

    auto& ctx = D2DContext::Get();
    if (!ctx.dwFactory || !format)
        return static_cast<float>(text.length()) * 7.0f;

    ComPtr<IDWriteTextLayout> layout;
    HRESULT hr = ctx.dwFactory->CreateTextLayout(
        text.c_str(),
        static_cast<UINT32>(text.length()),
        format,
        2048.0f,
        64.0f,
        &layout);
    if (FAILED(hr) || !layout)
        return static_cast<float>(text.length()) * 7.0f;

    DWRITE_TEXT_METRICS metrics{};
    if (FAILED(layout->GetMetrics(&metrics)))
        return static_cast<float>(text.length()) * 7.0f;

    return metrics.widthIncludingTrailingWhitespace;
}

void DrawCenteredGlyph(ID2D1HwndRenderTarget* rt,
                       const wchar_t* glyph,
                       const D2D1_RECT_F& rect,
                       ID2D1Brush* brush,
                       float size,
                       DWRITE_FONT_WEIGHT weight = DWRITE_FONT_WEIGHT_NORMAL)
{
    auto& ctx = D2DContext::Get();
    auto* tf = ctx.GetTextFormat(Theme::IconFont, size, weight);
    if (!tf) return;

    tf->SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER);
    tf->SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER);
    rt->DrawTextW(glyph, 1, tf, rect, brush);
    tf->SetTextAlignment(DWRITE_TEXT_ALIGNMENT_LEADING);
    tf->SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER);
}

CardLayout BuildCardLayout(const ClipboardItem& item,
                           float x,
                           float y,
                           float w,
                           float h,
                           float scale,
                           bool showActions)
{
    CardLayout layout;
    layout.background = {
        D2D1::RectF(x + 4.0f, y + 2.0f, x + w - 4.0f, y + h - 2.0f),
        Theme::CardCorner,
        Theme::CardCorner,
    };
    const float iconX = x + kCardPadX * scale;
    const float iconY = y + (h - kCardIconSize * scale) / 2.0f;
    layout.iconRect = D2D1::RectF(iconX, iconY,
                                  iconX + kCardIconSize * scale,
                                  iconY + kCardIconSize * scale);

    layout.contentLeft = x + (kCardPadX + kCardIconSize + kCardTextGap) * scale;

    const float actionTop = y + 6.0f * scale;
    const float actionRight = x + w - kActionRightPad * scale;
    layout.deleteRect = D2D1::RectF(actionRight - kActionSize * scale, actionTop,
                                    actionRight, actionTop + kActionSize * scale);
    layout.favoriteRect = D2D1::RectF(layout.deleteRect.left - (kActionGap + kActionSize) * scale,
                                      actionTop,
                                      layout.deleteRect.left - kActionGap * scale,
                                      actionTop + kActionSize * scale);
    layout.renameRect = D2D1::RectF(layout.favoriteRect.left - (6.0f + kActionSize) * scale,
                                    actionTop,
                                    layout.favoriteRect.left - 6.0f * scale,
                                    actionTop + kActionSize * scale);
    layout.contentRight = layout.renameRect.left - 8.0f * scale;

    layout.titleRect = D2D1::RectF(layout.contentLeft,
                                   y + kTitleY * scale,
                                   layout.contentRight,
                                   y + 24.0f * scale);

    const float tagRowTop = y + kSubtitleY * scale;
    const float tagRowBottom = tagRowTop + kTagChipHeight * scale;
    auto& ctx = D2DContext::Get();
    auto* tagTf = ctx.GetTextFormat(kOverlayFontName, 10.0f * scale);
    layout.tagAddText = kTagAddLabel;
    const float addTextWidth = MeasureTextWidth(layout.tagAddText, tagTf);
    const float addChipWidth = kTagChipPadX * scale * 2.0f + addTextWidth;
    layout.tagAddBounds = {
        D2D1::RectF(actionRight - addChipWidth, tagRowTop, actionRight, tagRowBottom),
        Theme::TagCorner,
        Theme::TagCorner,
    };
    layout.tagAddRect = D2D1::RectF(layout.tagAddBounds.rect.left + kTagChipPadX * scale,
                                    tagRowTop,
                                    layout.tagAddBounds.rect.right - kTagChipPadX * scale,
                                    tagRowBottom);

    float tagRight = showActions ? layout.tagAddBounds.rect.left - kTagChipGap * scale : actionRight;
    const float tagLeftLimit = layout.contentLeft + 8.0f * scale;
    const bool showRemove = showActions;
    const auto parsedTags = ParseTags(item.tags);

    for (int i = static_cast<int>(parsedTags.size()) - 1; i >= 0; --i) {
        const std::wstring displayTag = Ellipsize(parsedTags[i], 18);
        const float textWidth = MeasureTextWidth(displayTag, tagTf);
        const float removeWidth = showRemove ? (kTagRemoveGap + kTagRemoveSize) * scale : 0.0f;
        const float chipWidth = kTagChipPadX * scale * 2.0f + textWidth + removeWidth;
        const float chipLeft = tagRight - chipWidth;
        if (chipLeft < tagLeftLimit) break;

        TagChipLayout chip;
        chip.tagIdx = i;
        chip.text = displayTag;
        chip.bounds = {
            D2D1::RectF(chipLeft, tagRowTop, tagRight, tagRowBottom),
            Theme::TagCorner,
            Theme::TagCorner,
        };
        chip.textRect = D2D1::RectF(chipLeft + kTagChipPadX * scale,
                                    tagRowTop,
                                    tagRight - kTagChipPadX * scale - removeWidth,
                                    tagRowBottom);
        if (showRemove) {
            chip.removeRect = D2D1::RectF(chip.bounds.rect.right - (kTagChipPadX + kTagRemoveSize) * scale,
                                          tagRowTop,
                                          chip.bounds.rect.right - kTagChipPadX * scale,
                                          tagRowBottom);
        }
        layout.tagChips.push_back(chip);
        tagRight = chipLeft - kTagChipGap * scale;
    }

    std::reverse(layout.tagChips.begin(), layout.tagChips.end());

    float subtitleRight = actionRight;
    if (showActions) subtitleRight = layout.tagAddBounds.rect.left - 8.0f * scale;
    if (!layout.tagChips.empty()) {
        subtitleRight = (std::min)(subtitleRight, layout.tagChips.front().bounds.rect.left - 8.0f * scale);
    }
    subtitleRight = (std::max)(subtitleRight, layout.contentLeft + 56.0f * scale);

    layout.subtitleRect = D2D1::RectF(layout.contentLeft,
                                      tagRowTop - 1.0f * scale,
                                      subtitleRight,
                                      tagRowBottom);

    return layout;
}

} // namespace

bool OverlayWindow::RegisterClass(HINSTANCE hInst)
{
    WNDCLASSEXW wc = { sizeof(wc) };
    wc.style = CS_HREDRAW | CS_VREDRAW;
    wc.lpfnWndProc = WndProc;
    wc.hInstance = hInst;
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
    wc.hCursor = LoadCursorW(nullptr, IDC_ARROW);
    wc.hbrBackground = nullptr;
    wc.lpszClassName = kClassName;
    if (!RegisterClassExW(&wc) && GetLastError() != ERROR_CLASS_ALREADY_EXISTS)
        return false;

    WNDCLASSEXW panelWc = { sizeof(panelWc) };
    panelWc.lpfnWndProc = EditPanelWndProc;
    panelWc.hInstance = hInst;
    panelWc.hCursor = LoadCursorW(nullptr, IDC_IBEAM);
    panelWc.hbrBackground = nullptr;
    panelWc.lpszClassName = kEditPanelClassName;
    if (!RegisterClassExW(&panelWc) && GetLastError() != ERROR_CLASS_ALREADY_EXISTS)
        return false;

    return true;
}

OverlayWindow::OverlayWindow(HINSTANCE hInst,
                             Repository& repo,
                             ClipboardService& svc,
                             AppSettings& settings)
    : _hInst(hInst), _repo(repo), _svc(svc), _settings(settings)
{
    _hwnd = CreateWindowExW(
        WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
        kClassName, L"ClipEverything",
        WS_POPUP | WS_VSCROLL,
        0, 0, kDefaultOverlayWidth, kDefaultOverlayHeight,
        nullptr, nullptr, hInst, this);
}

OverlayWindow::~OverlayWindow()
{
    EndInlineEditorSession();

    if (_hNameEditBrush) {
        DeleteObject(_hNameEditBrush);
        _hNameEditBrush = nullptr;
    }
    if (_hTagEditBrush) {
        DeleteObject(_hTagEditBrush);
        _hTagEditBrush = nullptr;
    }
    if (_hSearchFont) {
        DeleteObject(_hSearchFont);
        _hSearchFont = nullptr;
    }
    if (_hNameEditFont) {
        DeleteObject(_hNameEditFont);
        _hNameEditFont = nullptr;
    }
    if (_hTagEditFont) {
        DeleteObject(_hTagEditFont);
        _hTagEditFont = nullptr;
    }

    if (_hwnd) {
        D2DContext::Get().ReleaseRenderTarget(_hwnd);
        DestroyWindow(_hwnd);
    }
}

bool OverlayWindow::IsInlineRenameActive() const
{
    return _inlineEditorMode == InlineEditorMode::Rename;
}

bool OverlayWindow::IsInlineTagActive() const
{
    return _inlineEditorMode == InlineEditorMode::TagEdit;
}

bool OverlayWindow::IsInlineEditorActive() const
{
    return IsInlineRenameActive() || IsInlineTagActive();
}

bool OverlayWindow::IsEditingItem(int itemIdx) const
{
    return itemIdx >= 0 &&
           itemIdx < static_cast<int>(_items.size()) &&
           _editingItemId != 0 &&
           _items[itemIdx].id == _editingItemId;
}

void OverlayWindow::RefreshInlineEditActivation()
{
    if (!_hwnd || !IsInlineEditorActive())
        return;

    BringWindowToTop(_hwnd);
    SetForegroundWindow(_hwnd);
    SetActiveWindow(_hwnd);
}

void OverlayWindow::SyncInlineEditIme(HWND edit)
{
    if (!edit)
        return;

    HFONT font = nullptr;
    if (edit == _hNameEdit)
        font = _hNameEditFont;
    else if (edit == _hTagEdit)
        font = _hTagEditFont;

    LOGFONTW lf{};
    if (!font || GetObjectW(font, sizeof(lf), &lf) != sizeof(lf))
        return;

    HIMC himc = ImmGetContext(edit);
    if (!himc)
        return;

    ImmSetCompositionFontW(himc, &lf);

    POINT caret{};
    GetCaretPos(&caret);

    COMPOSITIONFORM comp{};
    comp.dwStyle = CFS_POINT;
    comp.ptCurrentPos = caret;
    ImmSetCompositionWindow(himc, &comp);

    CANDIDATEFORM cand{};
    cand.dwIndex = 0;
    cand.dwStyle = CFS_CANDIDATEPOS;
    cand.ptCurrentPos.x = caret.x;
    cand.ptCurrentPos.y = caret.y + MulDiv(22, _dpi, 96);
    ImmSetCandidateWindow(himc, &cand);

    ImmReleaseContext(edit, himc);
}

LRESULT CALLBACK OverlayWindow::WndProc(HWND hwnd, UINT msg, WPARAM wp, LPARAM lp)
{
    if (msg == WM_CREATE) {
        auto* cs = reinterpret_cast<CREATESTRUCTW*>(lp);
        auto* self = reinterpret_cast<OverlayWindow*>(cs->lpCreateParams);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, reinterpret_cast<LONG_PTR>(self));
        self->_hwnd = hwnd;
        self->OnCreate();
        return 0;
    }

    auto* self = reinterpret_cast<OverlayWindow*>(GetWindowLongPtrW(hwnd, GWLP_USERDATA));
    if (self) return self->HandleMessage(msg, wp, lp);
    return DefWindowProcW(hwnd, msg, wp, lp);
}

LRESULT CALLBACK OverlayWindow::EditPanelWndProc(HWND hwnd, UINT msg, WPARAM wp, LPARAM lp)
{
    if (msg == WM_CREATE) {
        auto* cs = reinterpret_cast<CREATESTRUCTW*>(lp);
        auto* self = reinterpret_cast<OverlayWindow*>(cs->lpCreateParams);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, reinterpret_cast<LONG_PTR>(self));
        return 0;
    }

    auto* self = reinterpret_cast<OverlayWindow*>(GetWindowLongPtrW(hwnd, GWLP_USERDATA));
    const bool isTagPanel = self && hwnd == self->_hTagPanel;
    switch (msg) {
        case WM_ERASEBKGND: {
            RECT rc{};
            GetClientRect(hwnd, &rc);
            HBRUSH brush = isTagPanel && self && self->_hTagEditBrush
                ? self->_hTagEditBrush
                : GetSysColorBrush(COLOR_WINDOW);
            FillRect(reinterpret_cast<HDC>(wp), &rc, brush);
            return 1;
        }
        case WM_PAINT: {
            PAINTSTRUCT ps{};
            HDC hdc = BeginPaint(hwnd, &ps);
            RECT rc{};
            GetClientRect(hwnd, &rc);
            const int dpi = self ? self->_dpi : 96;
            const int radius = MulDiv(isTagPanel ? 8 : 10, dpi, 96);
            const int borderWidth = (std::max)(1, MulDiv(2, dpi, 96));
            const COLORREF borderColor = isTagPanel ? kTagPanelBorderColor : kInlinePanelBorderColor;
            HPEN borderPen = CreatePen(PS_SOLID, borderWidth, borderColor);
            HGDIOBJ oldPen = SelectObject(hdc, borderPen);
            if (isTagPanel) {
                HBRUSH fillBrush = self && self->_hTagEditBrush ? self->_hTagEditBrush : CreateSolidBrush(kTagEditBgColor);
                HGDIOBJ oldBrush = SelectObject(hdc, fillBrush);
                RoundRect(hdc, rc.left, rc.top, rc.right, rc.bottom, radius, radius);
                SelectObject(hdc, oldBrush);
                if (!self || !self->_hTagEditBrush) DeleteObject(fillBrush);
            } else {
                HBRUSH fillBrush = self && self->_hNameEditBrush ? self->_hNameEditBrush : GetSysColorBrush(COLOR_WINDOW);
                HGDIOBJ oldBrush = SelectObject(hdc, fillBrush);
                RoundRect(hdc, rc.left, rc.top, rc.right, rc.bottom, radius, radius);
                SelectObject(hdc, oldBrush);
            }
            SelectObject(hdc, oldPen);
            DeleteObject(borderPen);
            EndPaint(hwnd, &ps);
            return 0;
        }
        case WM_CTLCOLOREDIT:
        case WM_CTLCOLORSTATIC:
            if (self && (HWND)lp == self->_hNameEdit) {
                HDC hdc = reinterpret_cast<HDC>(wp);
                SetBkMode(hdc, OPAQUE);
                SetTextColor(hdc, kInlineEditTextColor);
                SetBkColor(hdc, kInlineEditBgColor);
                return reinterpret_cast<LRESULT>(self->_hNameEditBrush ? self->_hNameEditBrush : GetSysColorBrush(COLOR_WINDOW));
            }
            if (self && (HWND)lp == self->_hTagEdit) {
                HDC hdc = reinterpret_cast<HDC>(wp);
                SetBkMode(hdc, OPAQUE);
                SetTextColor(hdc, kTagEditTextColor);
                SetBkColor(hdc, kTagEditBgColor);
                return reinterpret_cast<LRESULT>(self->_hTagEditBrush ? self->_hTagEditBrush : GetSysColorBrush(COLOR_WINDOW));
            }
            break;
    }

    return DefWindowProcW(hwnd, msg, wp, lp);
}

LRESULT CALLBACK OverlayWindow::EditSubclassProc(HWND hwnd,
                                                 UINT msg,
                                                 WPARAM wp,
                                                 LPARAM lp,
                                                 UINT_PTR,
                                                 DWORD_PTR refData)
{
    auto* self = reinterpret_cast<OverlayWindow*>(refData);
    if (!self) return DefSubclassProc(hwnd, msg, wp, lp);

    if (msg == WM_CHAR && (hwnd == self->_hNameEdit || hwnd == self->_hTagEdit)) {
        if (wp == VK_RETURN || wp == VK_ESCAPE)
            return 0;
    }

    if (msg == WM_KEYDOWN) {
        if (hwnd == self->_hSearch) {
            switch (wp) {
                case VK_UP:
                case VK_DOWN:
                case VK_RETURN:
                case VK_ESCAPE:
                    PostMessageW(self->_hwnd, WM_KEYDOWN, wp, lp);
                    return 0;
            }
        } else if (hwnd == self->_hNameEdit) {
            switch (wp) {
                case VK_RETURN:
                    self->CommitInlineRename(true);
                    return 0;
                case VK_ESCAPE:
                    self->CancelInlineRename(true);
                    return 0;
            }
        } else if (hwnd == self->_hTagEdit) {
            switch (wp) {
                case VK_RETURN:
                    self->CommitInlineTagEdit();
                    return 0;
                case VK_ESCAPE:
                    self->CancelInlineTagEdit();
                    return 0;
            }
        }
    }

    if (msg == WM_SETFOCUS || msg == WM_INPUTLANGCHANGE ||
        msg == WM_IME_STARTCOMPOSITION || msg == WM_IME_COMPOSITION) {
        if (hwnd == self->_hNameEdit || hwnd == self->_hTagEdit)
            self->SyncInlineEditIme(hwnd);
    }

    if (msg == WM_KILLFOCUS) {
        if (hwnd == self->_hNameEdit && self->IsInlineRenameActive()) {
            self->CommitInlineRename(false);
        } else if (hwnd == self->_hTagEdit && self->IsInlineTagActive()) {
            self->CommitInlineTagEdit();
        }
    }

    return DefSubclassProc(hwnd, msg, wp, lp);
}

void OverlayWindow::OnCreate()
{
    _dpi = GetDpiForWindow(_hwnd);

    const RECT initialBounds = GetPreferredBounds();
    const int w = initialBounds.right - initialBounds.left;
    const int h = initialBounds.bottom - initialBounds.top;
    SetWindowPos(_hwnd, nullptr, initialBounds.left, initialBounds.top, w, h,
                 SWP_NOZORDER | SWP_NOACTIVATE);

    const int searchY = MulDiv((int)Theme::HeaderHeight, _dpi, 96);
    const int searchH = MulDiv((int)Theme::SearchHeight, _dpi, 96);
    _hSearch = CreateWindowExW(
        0, L"EDIT", nullptr,
        WS_CHILD | WS_VISIBLE | ES_AUTOHSCROLL,
        MulDiv(12, _dpi, 96), searchY + MulDiv(6, _dpi, 96),
        w - MulDiv(24, _dpi, 96), searchH - MulDiv(12, _dpi, 96),
        _hwnd, nullptr, _hInst, nullptr);

    LOGFONTW searchLf{};
    searchLf.lfHeight = -MulDiv(13, _dpi, 96);
    searchLf.lfWeight = FW_NORMAL;
    searchLf.lfQuality = CLEARTYPE_QUALITY;
    wcscpy_s(searchLf.lfFaceName, kOverlayFontName);
    _hSearchFont = CreateFontIndirectW(&searchLf);
    _hNameEditBrush = CreateSolidBrush(kInlineEditBgColor);
    _hTagEditBrush = CreateSolidBrush(kTagEditBgColor);
    SendMessageW(_hSearch, WM_SETFONT, reinterpret_cast<WPARAM>(_hSearchFont), TRUE);
    SendMessageW(_hSearch, EM_SETCUEBANNER, 0, reinterpret_cast<LPARAM>(L"검색..."));
    SetWindowSubclass(_hSearch, EditSubclassProc, 0, reinterpret_cast<DWORD_PTR>(this));

    _hNamePanel = CreateWindowExW(
        0, kEditPanelClassName, nullptr,
        WS_CHILD | WS_CLIPSIBLINGS | WS_CLIPCHILDREN,
        0, 0, 0, 0,
        _hwnd, nullptr, _hInst, this);

    _hNameEdit = CreateWindowExW(
        0, L"EDIT", nullptr,
        WS_CHILD | ES_AUTOHSCROLL,
        0, 0, 0, 0,
        _hNamePanel, nullptr, _hInst, nullptr);

    LOGFONTW nameLf{};
    nameLf.lfHeight = -MulDiv(13, _dpi, 96);
    nameLf.lfWeight = FW_SEMIBOLD;
    nameLf.lfQuality = CLEARTYPE_QUALITY;
    wcscpy_s(nameLf.lfFaceName, kOverlayFontName);
    _hNameEditFont = CreateFontIndirectW(&nameLf);
    SendMessageW(_hNameEdit, WM_SETFONT, reinterpret_cast<WPARAM>(_hNameEditFont), TRUE);
    SendMessageW(_hNameEdit, EM_SETCUEBANNER, 0, reinterpret_cast<LPARAM>(L"관리명 입력"));
    SendMessageW(_hNameEdit, EM_SETMARGINS, EC_LEFTMARGIN | EC_RIGHTMARGIN,
                 MAKELPARAM(MulDiv(4, _dpi, 96), MulDiv(4, _dpi, 96)));
    SetWindowTheme(_hNameEdit, L"", L"");
    SetWindowSubclass(_hNameEdit, EditSubclassProc, 1, reinterpret_cast<DWORD_PTR>(this));

    _hTagPanel = CreateWindowExW(
        0, kEditPanelClassName, nullptr,
        WS_CHILD | WS_CLIPSIBLINGS | WS_CLIPCHILDREN,
        0, 0, 0, 0,
        _hwnd, nullptr, _hInst, this);

    _hTagEdit = CreateWindowExW(
        0, L"EDIT", nullptr,
        WS_CHILD | ES_AUTOHSCROLL,
        0, 0, 0, 0,
        _hTagPanel, nullptr, _hInst, nullptr);

    LOGFONTW tagLf{};
    tagLf.lfHeight = -MulDiv(10, _dpi, 96);
    tagLf.lfWeight = FW_SEMIBOLD;
    tagLf.lfQuality = CLEARTYPE_QUALITY;
    wcscpy_s(tagLf.lfFaceName, kOverlayFontName);
    _hTagEditFont = CreateFontIndirectW(&tagLf);
    SendMessageW(_hTagEdit, WM_SETFONT, reinterpret_cast<WPARAM>(_hTagEditFont), TRUE);
    SendMessageW(_hTagEdit, EM_SETMARGINS, EC_LEFTMARGIN | EC_RIGHTMARGIN,
                 MAKELPARAM(MulDiv(7, _dpi, 96), MulDiv(7, _dpi, 96)));
    SetWindowTheme(_hTagEdit, L"", L"");
    SetWindowSubclass(_hTagEdit, EditSubclassProc, 2, reinterpret_cast<DWORD_PTR>(this));

    ApplyWindowEffect();
    SyncScrollBar();
}

void OverlayWindow::ApplyWindowEffect()
{
    int corner = DWMWCP_ROUND;
    DwmSetWindowAttribute(_hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, &corner, sizeof(corner));
}

float OverlayWindow::GetListTopPx() const
{
    return (Theme::HeaderHeight + Theme::SearchHeight + 2.0f) * (_dpi / 96.0f);
}

float OverlayWindow::GetListHeightPx() const
{
    RECT rc{};
    GetClientRect(_hwnd, &rc);
    const float listHeight = static_cast<float>(rc.bottom) - GetListTopPx() -
                             Theme::StatusHeight * (_dpi / 96.0f);
    return (std::max)(0.0f, listHeight);
}

float OverlayWindow::GetMaxScrollPx() const
{
    const float maxScroll = _contentHeight * (_dpi / 96.0f) - GetListHeightPx();
    return (std::max)(0.0f, maxScroll);
}

void OverlayWindow::ClampScrollOffset()
{
    _scrollOffset = (std::clamp)(_scrollOffset, 0.0f, GetMaxScrollPx());
}

void OverlayWindow::SyncScrollBar()
{
    if (!_hwnd)
        return;

    ClampScrollOffset();

    RECT rc{};
    GetClientRect(_hwnd, &rc);
    const int contentPx = (std::max)(0, static_cast<int>(std::lround(_contentHeight * (_dpi / 96.0f))));
    const int pagePx = (std::max)(0, static_cast<int>(std::lround(GetListHeightPx())));

    SCROLLINFO si{ sizeof(si) };
    si.fMask = SIF_RANGE | SIF_PAGE | SIF_POS | SIF_DISABLENOSCROLL;
    si.nMin = 0;
    si.nMax = (std::max)(0, contentPx - 1);
    si.nPage = static_cast<UINT>(pagePx);
    si.nPos = (std::max)(0, static_cast<int>(std::lround(_scrollOffset)));
    SetScrollInfo(_hwnd, SB_VERT, &si, TRUE);
}

RECT OverlayWindow::GetDefaultBounds() const
{
    RECT wa{};
    SystemParametersInfoW(SPI_GETWORKAREA, 0, &wa, 0);

    const int width = MulDiv(kDefaultOverlayWidth, _dpi, 96);
    const int height = MulDiv(kDefaultOverlayHeight, _dpi, 96);
    const int x = wa.left + (wa.right - wa.left - width) / 2;
    const int y = wa.bottom - height - MulDiv(kDefaultOverlayBottomMargin, _dpi, 96);
    return RECT{ x, y, x + width, y + height };
}

RECT OverlayWindow::ClampBoundsToWorkArea(const RECT& bounds) const
{
    RECT clamped = bounds;
    RECT workArea = GetMonitorWorkAreaForRect(bounds);
    const int minHeight = MulDiv(kMinOverlayHeight, _dpi, 96);
    const int width = clamped.right - clamped.left;
    int height = clamped.bottom - clamped.top;

    if (height < minHeight)
        height = minHeight;
    const int workHeight = static_cast<int>(workArea.bottom - workArea.top);
    const int maxHeight = (std::max)(minHeight, workHeight);
    if (height > maxHeight)
        height = maxHeight;

    clamped.right = clamped.left + width;
    clamped.bottom = clamped.top + height;

    const int maxLeft = (std::max)(static_cast<int>(workArea.left),
                                   static_cast<int>(workArea.right) - width);
    const int maxTop = (std::max)(static_cast<int>(workArea.top),
                                  static_cast<int>(workArea.bottom) - height);

    clamped.left = (std::clamp)(static_cast<int>(clamped.left),
                                static_cast<int>(workArea.left),
                                maxLeft);
    clamped.top = (std::clamp)(static_cast<int>(clamped.top),
                               static_cast<int>(workArea.top),
                               maxTop);
    clamped.right = clamped.left + width;
    clamped.bottom = clamped.top + height;
    return clamped;
}

RECT OverlayWindow::GetPreferredBounds() const
{
    RECT bounds = GetDefaultBounds();
    if (_settings.overlayBoundsSaved) {
        const int width = MulDiv(kDefaultOverlayWidth, _dpi, 96);
        const int height = _settings.overlayHeight > 0
            ? _settings.overlayHeight
            : (bounds.bottom - bounds.top);
        bounds = RECT{
            _settings.overlayX,
            _settings.overlayY,
            _settings.overlayX + width,
            _settings.overlayY + height
        };
    }

    return ClampBoundsToWorkArea(bounds);
}

void OverlayWindow::SaveWindowBounds()
{
    if (!_hwnd)
        return;

    RECT rc{};
    if (!GetWindowRect(_hwnd, &rc))
        return;

    _settings.overlayBoundsSaved = true;
    _settings.overlayX = rc.left;
    _settings.overlayY = rc.top;
    _settings.overlayHeight = rc.bottom - rc.top;
    _settings.Save();
}

bool OverlayWindow::IsPointInResizeTopZone(int y) const
{
    return y >= 0 && y <= MulDiv(kResizeZonePx, _dpi, 96);
}

bool OverlayWindow::IsPointInResizeBottomZone(int y, int clientHeight) const
{
    const int zone = MulDiv(kResizeZonePx, _dpi, 96);
    return y >= clientHeight - zone && y <= clientHeight;
}

void OverlayWindow::PositionWindow(bool activate)
{
    const RECT bounds = GetPreferredBounds();
    const UINT flags = activate ? 0 : SWP_NOACTIVATE;
    SetWindowPos(_hwnd, HWND_TOPMOST,
                 bounds.left, bounds.top,
                 bounds.right - bounds.left,
                 bounds.bottom - bounds.top,
                 flags);
}

void OverlayWindow::ShowAndRefresh(const std::wstring& contextApp)
{
    ShowOverlay(contextApp, 0);
}

void OverlayWindow::ShowAndEditItem(int64_t itemId)
{
    ShowOverlay(L"", itemId);
}

void OverlayWindow::ShowOverlay(const std::wstring& contextApp, int64_t editItemId)
{
    EndInlineEditorSession();
    ResetTooltip(false);
    _hoverAction = {};
    _pressedAction = {};
    _lastMousePos = { -1, -1 };

    _contextApp = contextApp;
    _showAll = false;
    _scrollOffset = 0.0f;
    _selectedIdx = -1;
    _hoverIdx = -1;
    _isClosing = false;

    SetWindowTextW(_hSearch, L"");
    LoadItems();

    const bool activate = (editItemId != 0);
    PositionWindow(activate);
    SyncScrollBar();

    if (editItemId != 0) {
        const int idx = FindItemIndexById(editItemId);
        if (idx >= 0) SetSelectedIndex(idx);
    }

    ShowWindow(_hwnd, activate ? SW_SHOW : SW_SHOWNOACTIVATE);
    AnimateWindow(_hwnd, 150, AW_BLEND);
    SetForegroundWindow(_hwnd);
    if (activate) SetActiveWindow(_hwnd);

    if (editItemId != 0) {
        const int idx = FindItemIndexById(editItemId);
        if (idx >= 0) {
            BeginInlineRename(idx);
        } else {
            SetFocus(_hSearch);
        }
    } else {
        SetFocus(_hSearch);
    }

    InvalidateRect(_hwnd, nullptr, FALSE);
}

void OverlayWindow::Hide()
{
    SaveWindowBounds();
    EndInlineEditorSession();
    ResetTooltip(false);
    _hoverAction = {};
    _pressedAction = {};
    _isClosing = true;
    AnimateWindow(_hwnd, 120, AW_BLEND | AW_HIDE);
    ShowWindow(_hwnd, SW_HIDE);
}

void OverlayWindow::LoadItems()
{
    wchar_t buffer[512]{};
    GetWindowTextW(_hSearch, buffer, 512);
    std::wstring search = buffer;

    const std::wstring app = _showAll ? L"" : _contextApp;
    _items = _repo.GetItems(search, app);
    if (_hoverAction.cardIdx >= static_cast<int>(_items.size()))
        _hoverAction = {};
    if (_tooltipAction.cardIdx >= static_cast<int>(_items.size()))
        ResetTooltip(false);

    _contentHeight = static_cast<float>(_items.size()) * Theme::CardHeight;
    if (_selectedIdx >= static_cast<int>(_items.size())) _selectedIdx = -1;
    ClampScrollOffset();
    SyncScrollBar();

    if (IsInlineRenameActive() || IsInlineTagActive()) {
        const int idx = FindItemIndexById(_editingItemId);
        if (idx >= 0) {
            _selectedIdx = idx;
            UpdateInlineEditorLayout();
        } else {
            EndInlineEditorSession();
        }
    }

    InvalidateRect(_hwnd, nullptr, TRUE);
}

int OverlayWindow::FindItemIndexById(int64_t itemId) const
{
    for (int i = 0; i < static_cast<int>(_items.size()); ++i) {
        if (_items[i].id == itemId) return i;
    }
    return -1;
}

void OverlayWindow::BeginInlineRename(int itemIdx)
{
    if (!_hNamePanel || !_hNameEdit || itemIdx < 0 || itemIdx >= static_cast<int>(_items.size()))
        return;

    EndInlineEditorSession();
    SetSelectedIndex(itemIdx);
    _hoverIdx = -1;
    _hoverAction = {};
    _pressedAction = {};
    ResetTooltip(false);

    const ClipboardItem& item = _items[itemIdx];
    const std::wstring prefill = GetInlineRenamePrefill(item);

    _inlineEditorMode = InlineEditorMode::Rename;
    _editingItemId = item.id;

    SetWindowTextW(_hNameEdit, prefill.c_str());
    RefreshInlineEditActivation();
    UpdateInlineEditorLayout();
    ShowWindow(_hNamePanel, SW_SHOW);
    ShowWindow(_hNameEdit, SW_SHOW);
    SetFocus(_hNameEdit);
    const int textLen = GetWindowTextLengthW(_hNameEdit);
    SendMessageW(_hNameEdit, EM_SETSEL, textLen, textLen);
    SyncInlineEditIme(_hNameEdit);
    InvalidateRect(_hwnd, nullptr, FALSE);
}

void OverlayWindow::CommitInlineRename(bool closeOverlay)
{
    if (!IsInlineRenameActive() || !_hNameEdit) {
        if (closeOverlay) Hide();
        return;
    }

    const int64_t itemId = _editingItemId;
    const int textLen = GetWindowTextLengthW(_hNameEdit);
    std::wstring value;
    if (textLen > 0) {
        std::vector<wchar_t> buffer(textLen + 1, L'\0');
        GetWindowTextW(_hNameEdit, buffer.data(), textLen + 1);
        value.assign(buffer.data());
    }
    value = TrimCopy(value);

    EndInlineEditorSession();
    _repo.Rename(itemId, value);

    if (closeOverlay) {
        Hide();
        return;
    }

    LoadItems();
    const int idx = FindItemIndexById(itemId);
    if (idx >= 0) SetSelectedIndex(idx);
}

void OverlayWindow::CancelInlineRename(bool closeOverlay)
{
    EndInlineEditorSession();
    if (closeOverlay) {
        Hide();
    } else {
        InvalidateRect(_hwnd, nullptr, FALSE);
    }
}

void OverlayWindow::BeginInlineTagEdit(int itemIdx, int tagIdx)
{
    if (!_hTagPanel || !_hTagEdit || itemIdx < 0 || itemIdx >= static_cast<int>(_items.size()))
        return;

    EndInlineEditorSession();
    SetSelectedIndex(itemIdx);
    _hoverIdx = -1;
    _hoverAction = {};
    _pressedAction = {};
    ResetTooltip(false);

    _inlineEditorMode = InlineEditorMode::TagEdit;
    _editingItemId = _items[itemIdx].id;
    _editingTagIdx = tagIdx;

    std::wstring initialText;
    if (tagIdx >= 0) {
        const auto tags = ParseTags(_items[itemIdx].tags);
        if (tagIdx < static_cast<int>(tags.size()))
            initialText = tags[tagIdx];
    }

    SetWindowTextW(_hTagEdit, initialText.c_str());
    RefreshInlineEditActivation();
    UpdateInlineEditorLayout();
    ShowWindow(_hTagPanel, SW_SHOW);
    ShowWindow(_hTagEdit, SW_SHOW);
    SetFocus(_hTagEdit);
    const int textLen = GetWindowTextLengthW(_hTagEdit);
    SendMessageW(_hTagEdit, EM_SETSEL, textLen, textLen);
    SyncInlineEditIme(_hTagEdit);
    InvalidateRect(_hwnd, nullptr, FALSE);
}

void OverlayWindow::CommitInlineTagEdit()
{
    if (!IsInlineTagActive() || !_hTagEdit) return;

    const int64_t itemId = _editingItemId;
    const int editingTagIdx = _editingTagIdx;
    const int textLen = GetWindowTextLengthW(_hTagEdit);
    std::wstring value;
    if (textLen > 0) {
        std::vector<wchar_t> buffer(textLen + 1, L'\0');
        GetWindowTextW(_hTagEdit, buffer.data(), textLen + 1);
        value.assign(buffer.data());
    }
    value = NormalizeTag(value);

    EndInlineEditorSession();

    if (value.empty()) {
        InvalidateRect(_hwnd, nullptr, FALSE);
        return;
    }

    const int idx = FindItemIndexById(itemId);
    if (idx < 0) {
        LoadItems();
        return;
    }

    auto tags = ParseTags(_items[idx].tags);
    if (editingTagIdx >= 0 && editingTagIdx < static_cast<int>(tags.size()))
        tags.erase(tags.begin() + editingTagIdx);

    const std::wstring lowered = ToLowerCopy(value);
    bool exists = std::any_of(tags.begin(), tags.end(),
                              [&](const std::wstring& existing) {
                                  return ToLowerCopy(existing) == lowered;
                              });
    if (!exists) tags.push_back(value);

    _repo.SetTags(itemId, JoinTags(tags));
    LoadItems();

    const int refreshedIdx = FindItemIndexById(itemId);
    if (refreshedIdx >= 0) SetSelectedIndex(refreshedIdx);
}

void OverlayWindow::CancelInlineTagEdit()
{
    EndInlineEditorSession();
    InvalidateRect(_hwnd, nullptr, FALSE);
}

void OverlayWindow::CommitActiveInlineEditor(bool closeOverlay)
{
    if (IsInlineRenameActive()) {
        CommitInlineRename(closeOverlay);
        return;
    }

    if (IsInlineTagActive()) {
        CommitInlineTagEdit();
        if (closeOverlay) Hide();
        return;
    }

    if (closeOverlay) Hide();
}

void OverlayWindow::EndInlineEditorSession()
{
    _inlineEditorMode = InlineEditorMode::None;
    _editingItemId = 0;
    _editingTagIdx = -1;

    if (_hNamePanel) ShowWindow(_hNamePanel, SW_HIDE);
    if (_hNameEdit) ShowWindow(_hNameEdit, SW_HIDE);
    if (_hTagPanel) ShowWindow(_hTagPanel, SW_HIDE);
    if (_hTagEdit) ShowWindow(_hTagEdit, SW_HIDE);
}

void OverlayWindow::UpdateInlineEditorLayout()
{
    if ((!IsInlineRenameActive() && !IsInlineTagActive()) ||
        _selectedIdx < 0 || _selectedIdx >= static_cast<int>(_items.size())) {
        if (_hNamePanel) ShowWindow(_hNamePanel, SW_HIDE);
        if (_hNameEdit) ShowWindow(_hNameEdit, SW_HIDE);
        if (_hTagPanel) ShowWindow(_hTagPanel, SW_HIDE);
        if (_hTagEdit) ShowWindow(_hTagEdit, SW_HIDE);
        return;
    }

    RECT rc{};
    GetClientRect(_hwnd, &rc);

    const float scale = _dpi / 96.0f;
    const float cardH = Theme::CardHeight * scale;
    const float listTop = (Theme::HeaderHeight + Theme::SearchHeight + 2.0f) * scale;
    const float listBottom = static_cast<float>(rc.bottom) - Theme::StatusHeight * scale;
    const float cardY = listTop + _selectedIdx * cardH - _scrollOffset;

    if (cardY + cardH < listTop || cardY > listBottom) {
        if (_hNamePanel) ShowWindow(_hNamePanel, SW_HIDE);
        if (_hNameEdit) ShowWindow(_hNameEdit, SW_HIDE);
        if (_hTagPanel) ShowWindow(_hTagPanel, SW_HIDE);
        if (_hTagEdit) ShowWindow(_hTagEdit, SW_HIDE);
        return;
    }

    const CardLayout layout = BuildCardLayout(
        _items[_selectedIdx],
        0.0f,
        cardY,
        static_cast<float>(rc.right),
        cardH,
        scale,
        true);

    if (IsInlineRenameActive()) {
        const int panelX = static_cast<int>(layout.titleRect.left);
        const int panelY = static_cast<int>(cardY + 4.0f * scale);
        const int panelW = (std::max)(140, static_cast<int>(layout.titleRect.right - layout.titleRect.left));
        const int panelH = (std::max)(28, static_cast<int>(26.0f * scale));
        const int editInsetX = static_cast<int>(kInlinePanelInsetX * scale);
        const int editInsetY = static_cast<int>(kInlinePanelInsetY * scale);

        SetWindowPos(_hNamePanel, HWND_TOP, panelX, panelY, panelW, panelH,
                     SWP_NOACTIVATE | SWP_SHOWWINDOW);
        SetWindowPos(_hNameEdit, nullptr,
                     editInsetX, editInsetY,
                     panelW - editInsetX * 2,
                     panelH - editInsetY * 2,
                     SWP_NOZORDER | SWP_NOACTIVATE | SWP_SHOWWINDOW);
        SyncInlineEditIme(_hNameEdit);
        ShowWindow(_hTagPanel, SW_HIDE);
        ShowWindow(_hTagEdit, SW_HIDE);
        return;
    }

    if (IsInlineTagActive()) {
        int panelX = 0;
        int panelY = 0;
        int panelW = 0;
        int panelH = 0;
        bool usingChipRect = false;

        if (_editingTagIdx >= 0) {
            for (const auto& chip : layout.tagChips) {
                if (chip.tagIdx != _editingTagIdx) continue;
                panelX = static_cast<int>(chip.bounds.rect.left);
                panelY = static_cast<int>(chip.bounds.rect.top);
                panelW = static_cast<int>(chip.bounds.rect.right - chip.bounds.rect.left);
                panelH = static_cast<int>(chip.bounds.rect.bottom - chip.bounds.rect.top);
                usingChipRect = true;
                break;
            }
        }

        if (!usingChipRect) {
            panelX = static_cast<int>(layout.tagAddBounds.rect.left);
            panelY = static_cast<int>(layout.tagAddBounds.rect.top);
            panelW = static_cast<int>(layout.tagAddBounds.rect.right - layout.tagAddBounds.rect.left);
            panelH = static_cast<int>(layout.tagAddBounds.rect.bottom - layout.tagAddBounds.rect.top);
        }

        const int editInsetX = static_cast<int>(kTagChipPadX * scale);
        const int editInsetY = static_cast<int>(2.0f * scale);

        SetWindowPos(_hTagPanel, HWND_TOP, panelX, panelY, panelW, panelH,
                     SWP_NOACTIVATE | SWP_SHOWWINDOW);
        SetWindowPos(_hTagEdit, nullptr,
                     editInsetX, editInsetY,
                     panelW - editInsetX * 2,
                     panelH - editInsetY * 2,
                     SWP_NOZORDER | SWP_NOACTIVATE | SWP_SHOWWINDOW);
        SyncInlineEditIme(_hTagEdit);
        ShowWindow(_hNamePanel, SW_HIDE);
        ShowWindow(_hNameEdit, SW_HIDE);
    }
}

void OverlayWindow::DrawCard(ID2D1HwndRenderTarget* rt,
                             const ClipboardItem& item,
                             int itemIdx,
                             float x,
                             float y,
                             float w,
                             float h,
                             bool hover,
                             bool selected)
{
    auto& ctx = D2DContext::Get();
    const float scale = _dpi / 96.0f;
    const bool isEditingCard = IsEditingItem(itemIdx);
    const bool suppressHoverFx = IsInlineEditorActive();
    const bool suppressSelectedFx = suppressHoverFx && isEditingCard;
    const bool effectiveHover = hover && !suppressHoverFx;
    const bool effectiveSelected = selected && !suppressSelectedFx;
    const bool isEditingTag = IsInlineTagActive() && item.id == _editingItemId;
    CardLayout layout = BuildCardLayout(item, x, y, w, h, scale, true);

    ComPtr<ID2D1SolidColorBrush> bgBrush;
    const D2D1_COLOR_F bgColor = effectiveSelected ? Theme::CardSelectedSurface() :
                                 effectiveHover ? Theme::CardHoverSurface() :
                                                  Theme::CardSurface();
    rt->CreateSolidColorBrush(bgColor, &bgBrush);
    rt->FillRoundedRectangle(layout.background, bgBrush.Get());
    ComPtr<ID2D1SolidColorBrush> cardBorderBrush;
    rt->CreateSolidColorBrush(
        effectiveSelected ? Theme::ActionActive() :
                   Theme::BorderSubtle(),
        &cardBorderBrush);
    rt->DrawRoundedRectangle(layout.background, cardBorderBrush.Get(),
                             effectiveSelected ? 1.2f : 0.8f);

    ID2D1Bitmap* icon = nullptr;
    if (item.contentType == ContentType::Image && !item.thumbnail.empty())
        icon = _imgCache.GetThumbnail(rt, item.thumbnail, item.id);
    else if (!item.exePath.empty())
        icon = _imgCache.GetAppIcon(rt, item.exePath);

    if (icon) {
        rt->DrawBitmap(icon, layout.iconRect);
    } else {
        ComPtr<ID2D1SolidColorBrush> iconBrush;
        rt->CreateSolidColorBrush(Theme::TextSecond(), &iconBrush);
        auto* tf = ctx.GetTextFormat(Theme::IconFont, 20.0f * scale);
        if (tf) {
            tf->SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER);
            rt->DrawTextW(item.ContentTypeIcon(), 1, tf, layout.iconRect, iconBrush.Get());
            tf->SetTextAlignment(DWRITE_TEXT_ALIGNMENT_LEADING);
        }
    }

    ComPtr<ID2D1SolidColorBrush> textBrush, subBrush, tagBrush, tagHoverBrush, tagTextBrush,
                                 actionBrush, actionHoverBrush, dangerBrush, dangerBgBrush, favoriteBrush;
    rt->CreateSolidColorBrush(Theme::TextPrimary(), &textBrush);
    rt->CreateSolidColorBrush(Theme::TextSecond(), &subBrush);
    rt->CreateSolidColorBrush(Theme::TagBg(), &tagBrush);
    rt->CreateSolidColorBrush(Theme::TagHoverBg(), &tagHoverBrush);
    rt->CreateSolidColorBrush(Theme::TagText(), &tagTextBrush);
    rt->CreateSolidColorBrush(Theme::TextSecond(), &actionBrush);
    rt->CreateSolidColorBrush(Theme::ActionActive(), &actionHoverBrush);
    rt->CreateSolidColorBrush(Theme::Danger(), &dangerBrush);
    rt->CreateSolidColorBrush(Theme::DangerBg(), &dangerBgBrush);
    rt->CreateSolidColorBrush(Theme::Favorite(), &favoriteBrush);

    auto* titleTf = ctx.GetTextFormat(kOverlayFontName, 13.0f * scale, DWRITE_FONT_WEIGHT_SEMI_BOLD);
    auto* subTf = ctx.GetTextFormat(kOverlayFontName, 11.0f * scale);
    auto* tagTf = ctx.GetTextFormat(kOverlayFontName, 10.0f * scale, DWRITE_FONT_WEIGHT_SEMI_BOLD);

    auto isHovered = [&](ActionTarget type, int tagIdx = -1) {
        if (suppressHoverFx)
            return false;
        return _hoverAction.type == type &&
               _hoverAction.cardIdx == itemIdx &&
               (tagIdx < 0 || _hoverAction.tagIdx == tagIdx);
    };
    auto isPressed = [&](ActionTarget type, int tagIdx = -1) {
        return _pressedAction.type == type &&
               _pressedAction.cardIdx == itemIdx &&
               (tagIdx < 0 || _pressedAction.tagIdx == tagIdx);
    };
    auto actionBgColor = [&](ActionTarget type) {
        if (type == ActionTarget::Delete) {
            return isPressed(type) ? Theme::DangerBg() : Theme::DangerBg();
        }
        return isPressed(type) ? Theme::ActionHoverBg() : Theme::ActionHoverBg();
    };
    auto actionIconBrush = [&](ActionTarget type) -> ID2D1Brush* {
        if (type == ActionTarget::Delete)
            return (isHovered(type) || isPressed(type)) ? dangerBrush.Get() : actionBrush.Get();
        if (type == ActionTarget::Favorite && item.isFavorite)
            return favoriteBrush.Get();
        return (isHovered(type) || isPressed(type)) ? actionHoverBrush.Get() : actionBrush.Get();
    };
    auto drawActionSurface = [&](const D2D1_RECT_F& rect, ActionTarget type) {
        if (!(isHovered(type) || isPressed(type))) return;
        D2D1_ROUNDED_RECT rr = {
            D2D1::RectF(rect.left - 3.0f * scale, rect.top - 2.0f * scale,
                        rect.right + 3.0f * scale, rect.bottom + 2.0f * scale),
            5.0f * scale,
            5.0f * scale,
        };
        ComPtr<ID2D1SolidColorBrush> fill;
        rt->CreateSolidColorBrush(actionBgColor(type), &fill);
        rt->FillRoundedRectangle(rr, fill.Get());
    };

    if (titleTf && !(IsInlineRenameActive() && item.id == _editingItemId)) {
        titleTf->SetTextAlignment(DWRITE_TEXT_ALIGNMENT_LEADING);
        const std::wstring displayName = Ellipsize(GetOverlayTitle(item), 60);
        rt->DrawTextW(displayName.c_str(),
                      static_cast<UINT32>(displayName.length()),
                      titleTf,
                      layout.titleRect,
                      textBrush.Get());
    }

    if (subTf) {
        subTf->SetTextAlignment(DWRITE_TEXT_ALIGNMENT_LEADING);
        const std::wstring subtitle = Ellipsize(GetOverlaySubtitle(item), 40);
        rt->DrawTextW(subtitle.c_str(),
                      static_cast<UINT32>(subtitle.length()),
                      subTf,
                      layout.subtitleRect,
                      subBrush.Get());
    }

    if (tagTf) {
        for (const auto& chip : layout.tagChips) {
            if (IsInlineTagActive() && item.id == _editingItemId && chip.tagIdx == _editingTagIdx)
                continue;
            const bool chipHovered = isHovered(ActionTarget::TagChip, chip.tagIdx);
            const bool removeHovered = isHovered(ActionTarget::TagRemove, chip.tagIdx);
            rt->FillRoundedRectangle(chip.bounds, (chipHovered || removeHovered) ? tagHoverBrush.Get() : tagBrush.Get());
            tagTf->SetTextAlignment(DWRITE_TEXT_ALIGNMENT_LEADING);
            rt->DrawTextW(chip.text.c_str(),
                          static_cast<UINT32>(chip.text.length()),
                          tagTf,
                          chip.textRect,
                          tagTextBrush.Get());

            if (chip.removeRect.right > chip.removeRect.left) {
                DrawCenteredGlyph(rt, kGlyphClose, chip.removeRect,
                                  removeHovered ? dangerBrush.Get() : tagTextBrush.Get(),
                                  8.0f * scale);
            }
        }
    }

    if (!(isEditingTag && _editingTagIdx < 0)) {
        rt->FillRoundedRectangle(layout.tagAddBounds,
                                 (isHovered(ActionTarget::TagAdd) || isPressed(ActionTarget::TagAdd))
                                     ? tagHoverBrush.Get()
                                     : tagBrush.Get());
        if (tagTf) {
            tagTf->SetTextAlignment(DWRITE_TEXT_ALIGNMENT_LEADING);
            rt->DrawTextW(layout.tagAddText.c_str(),
                          static_cast<UINT32>(layout.tagAddText.length()),
                          tagTf,
                          layout.tagAddRect,
                          tagTextBrush.Get());
        }
    }

    drawActionSurface(layout.renameRect, ActionTarget::Rename);
    drawActionSurface(layout.favoriteRect, ActionTarget::Favorite);
    drawActionSurface(layout.deleteRect, ActionTarget::Delete);
    DrawCenteredGlyph(rt, kGlyphRename, layout.renameRect, actionIconBrush(ActionTarget::Rename), 12.0f * scale);
    DrawCenteredGlyph(rt,
                      item.isFavorite ? kGlyphFavorite : kGlyphFavoriteOutline,
                      layout.favoriteRect,
                      actionIconBrush(ActionTarget::Favorite),
                      13.0f * scale);
    DrawCenteredGlyph(rt, kGlyphDelete, layout.deleteRect, actionIconBrush(ActionTarget::Delete), 12.0f * scale);
}

void OverlayWindow::OnPaint()
{
    RECT rc{};
    GetClientRect(_hwnd, &rc);
    const int w = rc.right;
    const int h = rc.bottom;

    auto* rt = D2DContext::Get().GetRenderTarget(_hwnd, w, h);
    if (!rt) {
        ValidateRect(_hwnd, nullptr);
        return;
    }

    const float scale = _dpi / 96.0f;
    auto& ctx = D2DContext::Get();

    rt->BeginDraw();
    rt->Clear(Theme::WindowChrome());
    rt->SetAntialiasMode(D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
    rt->SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);

    ComPtr<ID2D1SolidColorBrush> textBrush, subBrush, borderBrush, toggleBrush, toggleTextBrush,
                                 chromeBrush, headerBrush, searchBrush, contentBrush, statusBrush;
    rt->CreateSolidColorBrush(Theme::TextPrimary(), &textBrush);
    rt->CreateSolidColorBrush(Theme::TextSecond(), &subBrush);
    rt->CreateSolidColorBrush(Theme::BorderSubtle(), &borderBrush);
    rt->CreateSolidColorBrush(_showAll ? Theme::Accent() : Theme::BorderSubtle(), &toggleBrush);
    rt->CreateSolidColorBrush(_showAll ? D2D1::ColorF(D2D1::ColorF::White) : Theme::TextSecond(),
                              &toggleTextBrush);
    rt->CreateSolidColorBrush(Theme::WindowChrome(), &chromeBrush);
    rt->CreateSolidColorBrush(Theme::HeaderSurface(), &headerBrush);
    rt->CreateSolidColorBrush(Theme::SearchSurface(), &searchBrush);
    rt->CreateSolidColorBrush(Theme::ContentSurface(), &contentBrush);
    rt->CreateSolidColorBrush(Theme::StatusSurface(), &statusBrush);

    const float headerH = Theme::HeaderHeight * scale;
    const float searchBottom = (Theme::HeaderHeight + Theme::SearchHeight) * scale;
    const float statusY = static_cast<float>(h) - Theme::StatusHeight * scale;
    const float outerInset = 4.0f * scale;
    const D2D1_ROUNDED_RECT frame = {
        D2D1::RectF(outerInset, outerInset,
                    static_cast<float>(w) - outerInset,
                    static_cast<float>(h) - outerInset),
        Theme::CornerRadius,
        Theme::CornerRadius,
    };
    rt->FillRoundedRectangle(frame, chromeBrush.Get());
    rt->DrawRoundedRectangle(frame, borderBrush.Get(), 1.0f);

    rt->FillRectangle(
        D2D1::RectF(frame.rect.left + 1.0f, frame.rect.top + 1.0f,
                    frame.rect.right - 1.0f, searchBottom + 2.0f),
        headerBrush.Get());
    rt->FillRectangle(
        D2D1::RectF(frame.rect.left + 1.0f, searchBottom + 2.0f,
                    frame.rect.right - 1.0f, statusY),
        contentBrush.Get());
    rt->FillRectangle(
        D2D1::RectF(frame.rect.left + 1.0f, statusY,
                    frame.rect.right - 1.0f, frame.rect.bottom - 1.0f),
        statusBrush.Get());

    D2D1_ROUNDED_RECT searchRect = {
        D2D1::RectF(10.0f * scale, headerH + 4.0f * scale,
                    static_cast<float>(w) - 10.0f * scale, searchBottom - 4.0f * scale),
        10.0f * scale,
        10.0f * scale,
    };
    rt->FillRoundedRectangle(searchRect, searchBrush.Get());
    rt->DrawRoundedRectangle(searchRect, borderBrush.Get(), 1.0f);

    const float toggleW = 68.0f * scale;
    const float toggleH = 22.0f * scale;
    const float toggleX = static_cast<float>(w) - toggleW - 36.0f * scale;
    const float toggleY = (headerH - toggleH) / 2.0f;
    const float closeLeft = static_cast<float>(w) - 32.0f * scale;
    const float titleRight = (std::max)(24.0f * scale, (std::min)(toggleX, closeLeft) - 8.0f * scale);

    auto* titleTf = ctx.GetTextFormat(kOverlayFontName, 13.0f * scale, DWRITE_FONT_WEIGHT_SEMI_BOLD);
    if (titleTf) {
        titleTf->SetTextAlignment(DWRITE_TEXT_ALIGNMENT_LEADING);
        rt->DrawTextW(kHeaderTitle,
                      static_cast<UINT32>(wcslen(kHeaderTitle)),
                      titleTf,
                      D2D1::RectF(12.0f * scale, 0.0f, titleRight, headerH),
                      textBrush.Get());
    }

    D2D1_ROUNDED_RECT toggleRect = {
        D2D1::RectF(toggleX, toggleY, toggleX + toggleW, toggleY + toggleH),
        11.0f * scale,
        11.0f * scale,
    };
    rt->FillRoundedRectangle(toggleRect, toggleBrush.Get());
    if (_hoverAction.type == ActionTarget::ToggleAll) {
        ComPtr<ID2D1SolidColorBrush> hoverBrush;
        rt->CreateSolidColorBrush(Theme::ActionHoverBg(), &hoverBrush);
        rt->FillRoundedRectangle(toggleRect, hoverBrush.Get());
    }

    auto* smallTf = ctx.GetTextFormat(kOverlayFontName, 10.0f * scale);
    if (smallTf) {
        smallTf->SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER);
        rt->DrawTextW(L"전체 포함", 5, smallTf,
                      D2D1::RectF(toggleX, toggleY, toggleX + toggleW, toggleY + toggleH),
                      toggleTextBrush.Get());
        smallTf->SetTextAlignment(DWRITE_TEXT_ALIGNMENT_LEADING);
    }

    auto* closeTf = ctx.GetTextFormat(kOverlayFontName, 15.0f * scale, DWRITE_FONT_WEIGHT_LIGHT);
    if (closeTf) {
        if (_hoverAction.type == ActionTarget::Close) {
            ComPtr<ID2D1SolidColorBrush> hoverBrush;
            rt->CreateSolidColorBrush(Theme::ActionHoverBg(), &hoverBrush);
            D2D1_ROUNDED_RECT closeBg = {
                D2D1::RectF(static_cast<float>(w) - 28.0f * scale, 8.0f * scale,
                            static_cast<float>(w) - 6.0f * scale, headerH - 8.0f * scale),
                5.0f * scale,
                5.0f * scale,
            };
            rt->FillRoundedRectangle(closeBg, hoverBrush.Get());
        }
        closeTf->SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER);
        closeTf->SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER);
        rt->DrawTextW(L"\u00D7", 1, closeTf,
                      D2D1::RectF(closeLeft, 0.0f,
                                  static_cast<float>(w), headerH),
                      subBrush.Get());
        closeTf->SetTextAlignment(DWRITE_TEXT_ALIGNMENT_LEADING);
        closeTf->SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER);
    }

    rt->DrawLine(D2D1::Point2F(frame.rect.left, searchBottom + 1.0f),
                 D2D1::Point2F(frame.rect.right, searchBottom + 1.0f),
                 borderBrush.Get(), 1.0f);

    const float listTop = searchBottom + 2.0f;
    const float listH = static_cast<float>(h) - listTop - Theme::StatusHeight * scale;
    rt->PushAxisAlignedClip(
        D2D1::RectF(frame.rect.left, listTop, frame.rect.right, listTop + listH),
        D2D1_ANTIALIAS_MODE_ALIASED);

    if (_items.empty()) {
        auto* hintTf = ctx.GetTextFormat(kOverlayFontName, 13.0f * scale);
        if (hintTf) {
            hintTf->SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER);
            rt->DrawTextW(L"클립보드 항목이 없습니다.", 13, hintTf,
                          D2D1::RectF(0.0f, listTop + 40.0f * scale,
                                      static_cast<float>(w), listTop + 80.0f * scale),
                          subBrush.Get());
            hintTf->SetTextAlignment(DWRITE_TEXT_ALIGNMENT_LEADING);
        }
    } else {
        const float cardH = Theme::CardHeight * scale;
        for (int i = 0; i < static_cast<int>(_items.size()); ++i) {
            const float cy = listTop + i * cardH - _scrollOffset;
            if (cy + cardH < listTop) continue;
            if (cy > listTop + listH) break;
            DrawCard(rt, _items[i], i, 0.0f, cy, static_cast<float>(w), cardH,
                     i == _hoverIdx, i == _selectedIdx);
        }
    }

    rt->PopAxisAlignedClip();

    rt->DrawLine(D2D1::Point2F(frame.rect.left, statusY),
                 D2D1::Point2F(frame.rect.right, statusY),
                 borderBrush.Get(), 1.0f);

    auto* statusTf = ctx.GetTextFormat(kOverlayFontName, 11.0f * scale);
    if (statusTf) {
        std::wstring status = L"총 " + std::to_wstring(_items.size()) + L"개";
        if (!_contextApp.empty() && !_showAll)
            status += L"  (" + _contextApp + L")";
        if (IsInlineRenameActive())
            status += L"   관리명 입력  Enter 저장  Esc 취소";
        else if (IsInlineTagActive())
            status += L"   태그 입력  Enter 추가  Esc 취소";
        else if (_hoverAction.type != ActionTarget::None && !GetHoverStatusText().empty()) {
            status += L"   ";
            status += GetHoverStatusText();
        }
        else
            status += L"   Enter 붙여넣기";

        rt->DrawTextW(status.c_str(), static_cast<UINT32>(status.length()), statusTf,
                      D2D1::RectF(12.0f * scale, statusY,
                                  frame.rect.right, static_cast<float>(h)),
                      subBrush.Get());
    }

    if (!IsInlineEditorActive() && _tooltipVisible && _tooltipAction.type != ActionTarget::None) {
        const std::wstring tooltip = GetTooltipText(_tooltipAction);
        if (!tooltip.empty()) {
            auto* tooltipTf = ctx.GetTextFormat(kOverlayFontName, 10.0f * scale);
            const float textWidth = MeasureTextWidth(tooltip, tooltipTf);
            const float bubbleW = textWidth + kTooltipPadX * scale * 2.0f;
            const float bubbleH = 24.0f * scale;
            float bubbleX = _tooltipAction.rect.left + (_tooltipAction.rect.right - _tooltipAction.rect.left - bubbleW) / 2.0f;
            float bubbleY = _tooltipAction.rect.top - bubbleH - 8.0f * scale;
            bubbleX = (std::max)(8.0f * scale, (std::min)(bubbleX, static_cast<float>(w) - bubbleW - 8.0f * scale));
            if (bubbleY < headerH) bubbleY = _tooltipAction.rect.bottom + 8.0f * scale;

            ComPtr<ID2D1SolidColorBrush> tooltipBg, tooltipText;
            rt->CreateSolidColorBrush(Theme::TooltipBg(), &tooltipBg);
            rt->CreateSolidColorBrush(Theme::TooltipText(), &tooltipText);

            D2D1_ROUNDED_RECT bubble = {
                D2D1::RectF(bubbleX, bubbleY, bubbleX + bubbleW, bubbleY + bubbleH),
                6.0f * scale,
                6.0f * scale,
            };
            rt->FillRoundedRectangle(bubble, tooltipBg.Get());
            if (tooltipTf) {
                tooltipTf->SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER);
                rt->DrawTextW(tooltip.c_str(),
                              static_cast<UINT32>(tooltip.length()),
                              tooltipTf,
                              bubble.rect,
                              tooltipText.Get());
                tooltipTf->SetTextAlignment(DWRITE_TEXT_ALIGNMENT_LEADING);
            }
        }
    }

    rt->EndDraw();
}

int OverlayWindow::HitTestCard(int mouseY) const
{
    RECT rc{};
    GetClientRect(_hwnd, &rc);

    const float scale = _dpi / 96.0f;
    const float listTop = (Theme::HeaderHeight + Theme::SearchHeight + 2.0f) * scale;
    const float listH = rc.bottom - listTop - Theme::StatusHeight * scale;
    const float localY = mouseY - listTop + _scrollOffset;

    if (mouseY < listTop || mouseY > listTop + listH) return -1;

    const int idx = static_cast<int>(localY / (Theme::CardHeight * scale));
    if (idx < 0 || idx >= static_cast<int>(_items.size())) return -1;
    return idx;
}

bool OverlayWindow::IsSameActionHit(const ActionHit& a, const ActionHit& b) const
{
    return a.type == b.type && a.cardIdx == b.cardIdx && a.tagIdx == b.tagIdx;
}

void OverlayWindow::ResetTooltip(bool invalidate)
{
    KillTimer(_hwnd, kTooltipTimerId);
    const bool hadVisible = _tooltipVisible;
    _tooltipVisible = false;
    _tooltipAction = {};
    if (invalidate && hadVisible)
        InvalidateRect(_hwnd, nullptr, FALSE);
}

std::wstring OverlayWindow::GetTooltipText(const ActionHit& hit) const
{
    switch (hit.type) {
        case ActionTarget::ToggleAll:
            return _showAll ? L"현재 앱만 보기" : L"전체 목록 보기";
        case ActionTarget::Close:
            return L"닫기";
        case ActionTarget::Rename:
            return L"관리명 변경";
        case ActionTarget::Favorite:
            if (hit.cardIdx >= 0 && hit.cardIdx < static_cast<int>(_items.size()))
                return _items[hit.cardIdx].isFavorite ? L"즐겨찾기 해제" : L"즐겨찾기 추가";
            return L"즐겨찾기";
        case ActionTarget::Delete:
            return L"삭제";
        case ActionTarget::TagAdd:
            return L"태그 추가";
        case ActionTarget::TagRemove:
            return L"태그 삭제";
        case ActionTarget::TagChip:
            if (hit.cardIdx >= 0 && hit.cardIdx < static_cast<int>(_items.size())) {
                const auto tags = ParseTags(_items[hit.cardIdx].tags);
                if (hit.tagIdx >= 0 && hit.tagIdx < static_cast<int>(tags.size()))
                    return L"태그 수정: " + tags[hit.tagIdx];
            }
            return L"태그 수정";
        default:
            return L"";
    }
}

std::wstring OverlayWindow::GetHoverStatusText() const
{
    switch (_hoverAction.type) {
        case ActionTarget::ToggleAll:
            return _showAll ? L"현재 앱 목록만 보기" : L"전체 목록 보기";
        case ActionTarget::Close:
            return L"오버레이 닫기";
        case ActionTarget::Rename:
            return L"관리명 변경";
        case ActionTarget::Favorite:
            if (_hoverAction.cardIdx >= 0 && _hoverAction.cardIdx < static_cast<int>(_items.size()))
                return _items[_hoverAction.cardIdx].isFavorite ? L"즐겨찾기 해제" : L"즐겨찾기 추가";
            return L"즐겨찾기";
        case ActionTarget::Delete:
            return L"항목 삭제";
        case ActionTarget::TagAdd:
            return L"태그 추가";
        case ActionTarget::TagChip:
            return L"태그 수정";
        case ActionTarget::TagRemove:
            return L"태그 삭제";
        default:
            return L"";
    }
}

OverlayWindow::ActionHit OverlayWindow::HitTestAction(int x, int y) const
{
    ActionHit hit{};
    RECT rc{};
    GetClientRect(_hwnd, &rc);
    const float scale = _dpi / 96.0f;
    const float headerH = Theme::HeaderHeight * scale;

    const float toggleW = 68.0f * scale;
    const float toggleH = 22.0f * scale;
    const float toggleX = rc.right - toggleW - 36.0f * scale;
    const float toggleY = (headerH - toggleH) / 2.0f;
    if (x >= toggleX && x <= toggleX + toggleW &&
        y >= toggleY && y <= toggleY + toggleH) {
        hit.type = ActionTarget::ToggleAll;
        hit.rect = D2D1::RectF(toggleX, toggleY, toggleX + toggleW, toggleY + toggleH);
        return hit;
    }

    if (x >= rc.right - 32.0f * scale && y <= headerH) {
        hit.type = ActionTarget::Close;
        hit.rect = D2D1::RectF(static_cast<float>(rc.right) - 32.0f * scale, 0.0f,
                               static_cast<float>(rc.right), headerH);
        return hit;
    }

    const int idx = HitTestCard(y);
    if (idx < 0) return hit;

    const float listTop = (Theme::HeaderHeight + Theme::SearchHeight + 2.0f) * scale;
    const float cardH = Theme::CardHeight * scale;
    const float cardY = listTop + idx * cardH - _scrollOffset;
    const CardLayout layout = BuildCardLayout(
        _items[idx], 0.0f, cardY, static_cast<float>(rc.right), cardH, scale, true);

    hit.cardIdx = idx;
    hit.type = ActionTarget::Body;
    hit.rect = layout.background.rect;

    if (IsInlineEditorActive() && IsEditingItem(idx))
        return hit;

    if (PtInRectF(layout.renameRect, static_cast<float>(x), static_cast<float>(y))) {
        hit.type = ActionTarget::Rename;
        hit.rect = layout.renameRect;
        return hit;
    }
    if (PtInRectF(layout.favoriteRect, static_cast<float>(x), static_cast<float>(y))) {
        hit.type = ActionTarget::Favorite;
        hit.rect = layout.favoriteRect;
        return hit;
    }
    if (PtInRectF(layout.deleteRect, static_cast<float>(x), static_cast<float>(y))) {
        hit.type = ActionTarget::Delete;
        hit.rect = layout.deleteRect;
        return hit;
    }
    if (PtInRoundedRect(layout.tagAddBounds, static_cast<float>(x), static_cast<float>(y))) {
        hit.type = ActionTarget::TagAdd;
        hit.rect = layout.tagAddBounds.rect;
        return hit;
    }

    for (const auto& chip : layout.tagChips) {
        if (PtInRectF(chip.removeRect, static_cast<float>(x), static_cast<float>(y))) {
            hit.type = ActionTarget::TagRemove;
            hit.tagIdx = chip.tagIdx;
            hit.rect = chip.removeRect;
            return hit;
        }
        if (PtInRoundedRect(chip.bounds, static_cast<float>(x), static_cast<float>(y))) {
            hit.type = ActionTarget::TagChip;
            hit.tagIdx = chip.tagIdx;
            hit.rect = chip.bounds.rect;
            return hit;
        }
    }

    return hit;
}

void OverlayWindow::UpdateHoverState(int x, int y)
{
    if (!_mouseTracked) {
        TRACKMOUSEEVENT tme{ sizeof(tme), TME_LEAVE, _hwnd, 0 };
        TrackMouseEvent(&tme);
        _mouseTracked = true;
    }

    RECT clientRect{};
    GetClientRect(_hwnd, &clientRect);
    if (IsPointInResizeTopZone(y) || IsPointInResizeBottomZone(y, clientRect.bottom)) {
        _lastMousePos = { x, y };
        if (_hoverIdx != -1 || _hoverAction.type != ActionTarget::None) {
            _hoverIdx = -1;
            _hoverAction = {};
            ResetTooltip(false);
            InvalidateRect(_hwnd, nullptr, FALSE);
        }
        return;
    }

    const bool suppressHoverFx = IsInlineEditorActive();
    if (suppressHoverFx) {
        _lastMousePos = { x, y };
        if (_hoverIdx != -1 || _hoverAction.type != ActionTarget::None) {
            _hoverIdx = -1;
            _hoverAction = {};
            ResetTooltip(false);
            InvalidateRect(_hwnd, nullptr, FALSE);
        }
        return;
    }

    const int hoverIdx = suppressHoverFx ? -1 : HitTestCard(y);
    const ActionHit hit = HitTestAction(x, y);
    const bool hoverChanged = hoverIdx != _hoverIdx;
    const bool actionChanged = !IsSameActionHit(hit, _hoverAction);
    const bool mouseMoved = (_lastMousePos.x != x || _lastMousePos.y != y);

    _lastMousePos = { x, y };
    _hoverIdx = hoverIdx;
    _hoverAction = hit;

    if (actionChanged || mouseMoved) {
        ResetTooltip(true);
        if (!suppressHoverFx && hit.type != ActionTarget::None && hit.type != ActionTarget::Body) {
            SetTimer(_hwnd, kTooltipTimerId, 500, nullptr);
        }
    }

    if (hoverChanged || actionChanged)
        InvalidateRect(_hwnd, nullptr, FALSE);
}

void OverlayWindow::OnMouseMove(int x, int y)
{
    if (_dragMode != WindowDragMode::None) {
        POINT screen{};
        GetCursorPos(&screen);

        const int dx = screen.x - _dragStartScreen.x;
        const int dy = screen.y - _dragStartScreen.y;
        RECT updated = _dragStartRect;
        const int width = _dragStartRect.right - _dragStartRect.left;
        const int height = _dragStartRect.bottom - _dragStartRect.top;
        const int minHeight = MulDiv(kMinOverlayHeight, _dpi, 96);
        const RECT workArea = GetMonitorWorkAreaForPoint(screen);

        if (_dragMode == WindowDragMode::Move) {
            updated.left = _dragStartRect.left + dx;
            updated.top = _dragStartRect.top + dy;
            const int maxLeft = (std::max)(static_cast<int>(workArea.left),
                                           static_cast<int>(workArea.right) - width);
            const int maxTop = (std::max)(static_cast<int>(workArea.top),
                                          static_cast<int>(workArea.bottom) - height);
            updated.left = (std::clamp)(static_cast<int>(updated.left),
                                        static_cast<int>(workArea.left),
                                        maxLeft);
            updated.top = (std::clamp)(static_cast<int>(updated.top),
                                       static_cast<int>(workArea.top),
                                       maxTop);
            updated.right = updated.left + width;
            updated.bottom = updated.top + height;
        } else if (_dragMode == WindowDragMode::ResizeTop) {
            int newTop = _dragStartRect.top + dy;
            newTop = (std::max)(static_cast<int>(workArea.top), newTop);
            newTop = (std::min)(newTop, static_cast<int>(_dragStartRect.bottom) - minHeight);
            updated.top = newTop;
            updated.bottom = _dragStartRect.bottom;
        } else if (_dragMode == WindowDragMode::ResizeBottom) {
            int newBottom = _dragStartRect.bottom + dy;
            newBottom = (std::min)(static_cast<int>(workArea.bottom), newBottom);
            newBottom = (std::max)(newBottom, static_cast<int>(_dragStartRect.top) + minHeight);
            updated.bottom = newBottom;
            updated.top = _dragStartRect.top;
        }

        SetWindowPos(_hwnd, HWND_TOPMOST,
                     updated.left, updated.top,
                     updated.right - updated.left,
                     updated.bottom - updated.top,
                     SWP_NOACTIVATE);
        return;
    }

    UpdateHoverState(x, y);
}

void OverlayWindow::OnMouseLeave()
{
    _mouseTracked = false;
    if (IsInlineEditorActive()) {
        _lastMousePos = { -1, -1 };
        return;
    }
    _hoverIdx = -1;
    _hoverAction = {};
    _pressedAction = {};
    _lastMousePos = { -1, -1 };
    ResetTooltip(false);
    InvalidateRect(_hwnd, nullptr, FALSE);
}

void OverlayWindow::OnLButtonDown(int x, int y)
{
    if (IsInlineRenameActive() || IsInlineTagActive()) {
        HWND activePanel = IsInlineRenameActive() ? _hNamePanel : _hTagPanel;
        RECT panelRect{};
        if (activePanel && GetWindowRect(activePanel, &panelRect)) {
            POINT tl{ panelRect.left, panelRect.top };
            POINT br{ panelRect.right, panelRect.bottom };
            ScreenToClient(_hwnd, &tl);
            ScreenToClient(_hwnd, &br);
            const bool clickedInsidePanel =
                x >= tl.x && x <= br.x &&
                y >= tl.y && y <= br.y;
            if (!clickedInsidePanel) {
                if (IsInlineRenameActive())
                    CommitInlineRename(false);
                else
                    CommitInlineTagEdit();
                _pressedAction = {};
                ResetTooltip(false);
                InvalidateRect(_hwnd, nullptr, FALSE);
                return;
            }
        }
    }

    RECT clientRect{};
    GetClientRect(_hwnd, &clientRect);
    if (IsPointInResizeTopZone(y) || IsPointInResizeBottomZone(y, clientRect.bottom)) {
        _dragMode = IsPointInResizeTopZone(y)
            ? WindowDragMode::ResizeTop
            : WindowDragMode::ResizeBottom;
        GetCursorPos(&_dragStartScreen);
        GetWindowRect(_hwnd, &_dragStartRect);
        _pressedAction = {};
        ResetTooltip(false);
        SetCapture(_hwnd);
        return;
    }

    const float headerH = Theme::HeaderHeight * (_dpi / 96.0f);
    const ActionHit initialHit = HitTestAction(x, y);
    if (y >= 0 && y <= headerH && initialHit.type == ActionTarget::None) {
        _dragMode = WindowDragMode::Move;
        GetCursorPos(&_dragStartScreen);
        GetWindowRect(_hwnd, &_dragStartRect);
        _pressedAction = {};
        ResetTooltip(false);
        SetCapture(_hwnd);
        return;
    }

    _pressedAction = initialHit;
    if (_pressedAction.cardIdx >= 0)
        SetSelectedIndex(_pressedAction.cardIdx);

    ResetTooltip(false);
    if (_pressedAction.type != ActionTarget::None) {
        SetCapture(_hwnd);
        InvalidateRect(_hwnd, nullptr, FALSE);
    }
}

void OverlayWindow::OnLButtonUp(int x, int y)
{
    if (_dragMode != WindowDragMode::None) {
        if (GetCapture() == _hwnd)
            ReleaseCapture();
        _dragMode = WindowDragMode::None;
        SaveWindowBounds();
        UpdateHoverState(x, y);
        return;
    }

    const ActionHit pressed = _pressedAction;
    if (_pressedAction.type != ActionTarget::None)
        ReleaseCapture();
    _pressedAction = {};

    const ActionHit released = HitTestAction(x, y);
    if (!IsSameActionHit(pressed, released)) {
        InvalidateRect(_hwnd, nullptr, FALSE);
        return;
    }

    switch (released.type) {
        case ActionTarget::ToggleAll:
            _showAll = !_showAll;
            LoadItems();
            return;
        case ActionTarget::Close:
            Hide();
            return;
        case ActionTarget::Rename:
            BeginInlineRename(released.cardIdx);
            return;
        case ActionTarget::Favorite: {
            const int64_t itemId = _items[released.cardIdx].id;
            _repo.SetFavorite(itemId, !_items[released.cardIdx].isFavorite);
            LoadItems();
            const int refreshedIdx = FindItemIndexById(itemId);
            if (refreshedIdx >= 0) SetSelectedIndex(refreshedIdx);
            return;
        }
        case ActionTarget::Delete: {
            const std::wstring label = GetOverlayTitle(_items[released.cardIdx]);
            const std::wstring message = L"'" + label + L"' 항목을 삭제하시겠습니까?";
            const int confirm = MessageBoxW(
                _hwnd,
                message.c_str(),
                L"ClipEverything",
                MB_OKCANCEL | MB_ICONWARNING);
            if (confirm == IDOK) {
                const int64_t itemId = _items[released.cardIdx].id;
                EndInlineEditorSession();
                _repo.Delete(itemId);
                LoadItems();
            }
            return;
        }
        case ActionTarget::TagAdd:
            BeginInlineTagEdit(released.cardIdx, -1);
            return;
        case ActionTarget::TagChip:
            BeginInlineTagEdit(released.cardIdx, released.tagIdx);
            return;
        case ActionTarget::TagRemove: {
            const int64_t itemId = _items[released.cardIdx].id;
            auto tags = ParseTags(_items[released.cardIdx].tags);
            if (released.tagIdx >= 0 && released.tagIdx < static_cast<int>(tags.size()))
                tags.erase(tags.begin() + released.tagIdx);
            _repo.SetTags(itemId, JoinTags(tags));
            LoadItems();
            const int refreshedIdx = FindItemIndexById(itemId);
            if (refreshedIdx >= 0) SetSelectedIndex(refreshedIdx);
            return;
        }
        case ActionTarget::Body:
            if (released.cardIdx >= 0 && released.cardIdx < static_cast<int>(_items.size()))
                ExecutePaste(_items[released.cardIdx].id);
            return;
        case ActionTarget::None:
        default:
            InvalidateRect(_hwnd, nullptr, FALSE);
            return;
    }
}

void OverlayWindow::OnMouseWheel(int delta)
{
    if (IsInlineRenameActive())
        CommitInlineRename(false);
    else if (IsInlineTagActive())
        CommitInlineTagEdit();

    ResetTooltip(false);

    const float cardH = Theme::CardHeight * (_dpi / 96.0f);
    _scrollOffset -= (delta / static_cast<float>(WHEEL_DELTA)) * cardH * 3.0f;
    SyncScrollBar();
    InvalidateRect(_hwnd, nullptr, FALSE);
}

void OverlayWindow::OnVScroll(WPARAM code, int pos)
{
    (void)pos;
    SCROLLINFO si{ sizeof(si) };
    si.fMask = SIF_ALL;
    GetScrollInfo(_hwnd, SB_VERT, &si);

    const float scale = _dpi / 96.0f;
    const float lineStep = Theme::CardHeight * scale;
    const float pageStep = (std::max)(lineStep, GetListHeightPx() - lineStep);

    switch (code) {
        case SB_LINEUP:
            _scrollOffset -= lineStep;
            break;
        case SB_LINEDOWN:
            _scrollOffset += lineStep;
            break;
        case SB_PAGEUP:
            _scrollOffset -= pageStep;
            break;
        case SB_PAGEDOWN:
            _scrollOffset += pageStep;
            break;
        case SB_TOP:
            _scrollOffset = 0.0f;
            break;
        case SB_BOTTOM:
            _scrollOffset = GetMaxScrollPx();
            break;
        case SB_THUMBPOSITION:
        case SB_THUMBTRACK:
            _scrollOffset = static_cast<float>(si.nTrackPos);
            break;
        default:
            return;
    }

    SyncScrollBar();
    UpdateInlineEditorLayout();
    InvalidateRect(_hwnd, nullptr, FALSE);
}

void OverlayWindow::SetSelectedIndex(int idx)
{
    if (_items.empty()) return;
    _selectedIdx = max(0, min(idx, static_cast<int>(_items.size()) - 1));

    RECT rc{};
    GetClientRect(_hwnd, &rc);
    const float scale = _dpi / 96.0f;
    const float listTop = (Theme::HeaderHeight + Theme::SearchHeight + 2.0f) * scale;
    const float listH = rc.bottom - listTop - Theme::StatusHeight * scale;
    const float cardH = Theme::CardHeight * scale;
    const float itemTop = _selectedIdx * cardH;
    const float itemBottom = itemTop + cardH;

    if (itemTop < _scrollOffset) _scrollOffset = itemTop;
    if (itemBottom > _scrollOffset + listH) _scrollOffset = itemBottom - listH;

    SyncScrollBar();
    UpdateInlineEditorLayout();
    InvalidateRect(_hwnd, nullptr, FALSE);
}

void OverlayWindow::OnKeyDown(WPARAM vk)
{
    if (IsInlineRenameActive()) {
        switch (vk) {
            case VK_RETURN:
                CommitInlineRename(true);
                return;
            case VK_ESCAPE:
                CancelInlineRename(true);
                return;
        }
    }

    if (IsInlineTagActive()) {
        switch (vk) {
            case VK_RETURN:
                CommitInlineTagEdit();
                return;
            case VK_ESCAPE:
                CancelInlineTagEdit();
                return;
        }
    }

    switch (vk) {
        case VK_DOWN:
            SetSelectedIndex(_selectedIdx + 1);
            break;
        case VK_UP:
            SetSelectedIndex(_selectedIdx - 1);
            break;
        case VK_PRIOR:
            OnVScroll(SB_PAGEUP, 0);
            break;
        case VK_NEXT:
            OnVScroll(SB_PAGEDOWN, 0);
            break;
        case VK_RETURN:
            if (_selectedIdx >= 0 && _selectedIdx < static_cast<int>(_items.size()))
                ExecutePaste(_items[_selectedIdx].id);
            break;
        case VK_ESCAPE:
            Hide();
            break;
    }
}

void OverlayWindow::OnSearchChanged()
{
    LoadItems();
}

void OverlayWindow::OnActivate(bool active)
{
    if (!active && !_isClosing) {
        if (IsInlineRenameActive())
            CommitInlineRename(false);
        else if (IsInlineTagActive())
            CommitInlineTagEdit();
        Hide();
    }
}

void OverlayWindow::OnSize(int w, int h)
{
    D2DContext::Get().ResizeRenderTarget(_hwnd, w, h);
    if (_hSearch) {
        const int searchY = MulDiv((int)Theme::HeaderHeight, _dpi, 96);
        const int searchH = MulDiv((int)Theme::SearchHeight, _dpi, 96);
        SetWindowPos(_hSearch, nullptr,
                     MulDiv(12, _dpi, 96), searchY + MulDiv(6, _dpi, 96),
                     w - MulDiv(24, _dpi, 96), searchH - MulDiv(12, _dpi, 96),
                     SWP_NOZORDER);
    }
    SyncScrollBar();
    UpdateInlineEditorLayout();
}

void OverlayWindow::OnDpiChanged(int dpi, const RECT* suggested)
{
    _dpi = dpi;
    _imgCache.Clear();

    if (suggested) {
        SetWindowPos(_hwnd, nullptr,
                     suggested->left, suggested->top,
                     suggested->right - suggested->left,
                     suggested->bottom - suggested->top,
                     SWP_NOZORDER | SWP_NOACTIVATE);
    } else {
        const RECT bounds = GetPreferredBounds();
        SetWindowPos(_hwnd, nullptr,
                     bounds.left, bounds.top,
                     bounds.right - bounds.left,
                     bounds.bottom - bounds.top,
                     SWP_NOZORDER | SWP_NOACTIVATE);
    }

    SyncScrollBar();
    UpdateInlineEditorLayout();
}

void OverlayWindow::OnTimer(UINT_PTR timerId)
{
    if (timerId != kTooltipTimerId) return;
    if (IsInlineEditorActive()) {
        ResetTooltip(false);
        return;
    }

    KillTimer(_hwnd, kTooltipTimerId);
    _tooltipAction = _hoverAction;
    const std::wstring text = GetTooltipText(_tooltipAction);
    _tooltipVisible = !text.empty();
    InvalidateRect(_hwnd, nullptr, FALSE);
}

LRESULT OverlayWindow::OnSetCursor()
{
    POINT pt{};
    GetCursorPos(&pt);
    ScreenToClient(_hwnd, &pt);

    RECT clientRect{};
    GetClientRect(_hwnd, &clientRect);
    if (_dragMode == WindowDragMode::ResizeTop ||
        _dragMode == WindowDragMode::ResizeBottom ||
        IsPointInResizeTopZone(pt.y) ||
        IsPointInResizeBottomZone(pt.y, clientRect.bottom)) {
        SetCursor(LoadCursorW(nullptr, IDC_SIZENS));
        return TRUE;
    }

    const ActionHit hit = HitTestAction(pt.x, pt.y);
    if (hit.type != ActionTarget::None && hit.type != ActionTarget::Body) {
        SetCursor(LoadCursorW(nullptr, IDC_HAND));
        return TRUE;
    }

    const float headerH = Theme::HeaderHeight * (_dpi / 96.0f);
    if (pt.y >= 0 && pt.y <= headerH && hit.type == ActionTarget::None) {
        SetCursor(LoadCursorW(nullptr, IDC_SIZEALL));
        return TRUE;
    }

    SetCursor(LoadCursorW(nullptr, IDC_ARROW));
    return TRUE;
}

void OverlayWindow::ExecutePaste(int64_t itemId)
{
    Hide();

    struct PasteCtx {
        ClipboardService* svc;
        int64_t id;
    };

    auto* ctx = new PasteCtx{ &_svc, itemId };
    SetTimer(_hwnd, reinterpret_cast<UINT_PTR>(ctx), 80,
             [](HWND hwnd, UINT, UINT_PTR id, DWORD) {
                 auto* pasteCtx = reinterpret_cast<PasteCtx*>(id);
                 pasteCtx->svc->PasteSelectedItem(pasteCtx->id);
                 KillTimer(hwnd, id);
                 delete pasteCtx;
             });
}

LRESULT OverlayWindow::HandleMessage(UINT msg, WPARAM wp, LPARAM lp)
{
    switch (msg) {
        case WM_PAINT: {
            PAINTSTRUCT ps{};
            BeginPaint(_hwnd, &ps);
            OnPaint();
            EndPaint(_hwnd, &ps);
            return 0;
        }
        case WM_SIZE:
            OnSize(LOWORD(lp), HIWORD(lp));
            return 0;
        case WM_MOUSEMOVE:
            OnMouseMove(GET_X_LPARAM(lp), GET_Y_LPARAM(lp));
            return 0;
        case WM_MOUSELEAVE:
            OnMouseLeave();
            return 0;
        case WM_LBUTTONDOWN:
            OnLButtonDown(GET_X_LPARAM(lp), GET_Y_LPARAM(lp));
            return 0;
        case WM_LBUTTONUP:
            OnLButtonUp(GET_X_LPARAM(lp), GET_Y_LPARAM(lp));
            return 0;
        case WM_MOUSEWHEEL:
            OnMouseWheel(GET_WHEEL_DELTA_WPARAM(wp));
            return 0;
        case WM_VSCROLL:
            OnVScroll(LOWORD(wp), HIWORD(wp));
            return 0;
        case WM_KEYDOWN:
            OnKeyDown(wp);
            return 0;
        case WM_TIMER:
            OnTimer(wp);
            return 0;
        case WM_COMMAND:
            if (HIWORD(wp) == EN_CHANGE && reinterpret_cast<HWND>(lp) == _hSearch)
                OnSearchChanged();
            return 0;
        case WM_ACTIVATE:
            OnActivate(LOWORD(wp) != WA_INACTIVE);
            return 0;
        case WM_DPICHANGED:
            OnDpiChanged(HIWORD(wp), reinterpret_cast<const RECT*>(lp));
            return 0;
        case WM_SETCURSOR:
            if ((HWND)wp == _hwnd && LOWORD(lp) == HTCLIENT)
                return OnSetCursor();
            break;
        case WM_CAPTURECHANGED:
            _pressedAction = {};
            _dragMode = WindowDragMode::None;
            InvalidateRect(_hwnd, nullptr, FALSE);
            return 0;
        case WM_ERASEBKGND:
            return 1;
    }

    return DefWindowProcW(_hwnd, msg, wp, lp);
}
