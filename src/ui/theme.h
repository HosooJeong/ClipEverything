#pragma once
#include <d2d1.h>
#include <d2d1helper.h>

namespace Theme {
    // 색상
    inline D2D1_COLOR_F Accent()       { return D2D1::ColorF(0x0078D4); }
    inline D2D1_COLOR_F AccentHover()  { return D2D1::ColorF(0x106EBE); }
    inline D2D1_COLOR_F Surface()      { return D2D1::ColorF(0xF3F3F3); }
    inline D2D1_COLOR_F WindowChrome() { return D2D1::ColorF(0xF7F9FC, 1.0f); }
    inline D2D1_COLOR_F HeaderSurface(){ return D2D1::ColorF(0xFBFCFE, 1.0f); }
    inline D2D1_COLOR_F SearchSurface(){ return D2D1::ColorF(0xFFFFFF, 1.0f); }
    inline D2D1_COLOR_F ContentSurface(){ return D2D1::ColorF(0xFAFBFC, 1.0f); }
    inline D2D1_COLOR_F StatusSurface(){ return D2D1::ColorF(0xF7F8FA, 1.0f); }
    inline D2D1_COLOR_F CardSurface()  { return D2D1::ColorF(0xF5F6F7, 1.0f); }
    inline D2D1_COLOR_F CardHoverSurface() { return D2D1::ColorF(0xEFF2F6, 1.0f); }
    inline D2D1_COLOR_F CardSelectedSurface() { return D2D1::ColorF(0xE8EEF5, 1.0f); }
    inline D2D1_COLOR_F CardEditingSurface() { return D2D1::ColorF(0xF5F6F7, 1.0f); }
    inline D2D1_COLOR_F TextPrimary()  { return D2D1::ColorF(0x1A1A1A); }
    inline D2D1_COLOR_F TextSecond()   { return D2D1::ColorF(0x605E5C, 1.0f); }
    inline D2D1_COLOR_F BorderSubtle() { return D2D1::ColorF(0xE0E0E0); }
    inline D2D1_COLOR_F Favorite()     { return D2D1::ColorF(0xFFB900); }
    inline D2D1_COLOR_F HoverOverlay() { return D2D1::ColorF(0x000000, 0.06f); }
    inline D2D1_COLOR_F SelectBlue()   { return D2D1::ColorF(0x0078D4, 0.12f); }
    inline D2D1_COLOR_F TagBg()        { return D2D1::ColorF(0xE8F0FE); }
    inline D2D1_COLOR_F TagText()      { return D2D1::ColorF(0x1A73E8); }
    inline D2D1_COLOR_F TagBorder()    { return D2D1::ColorF(0xC5D9FB); }
    inline D2D1_COLOR_F TagHoverBg()   { return D2D1::ColorF(0xD8E8FF); }
    inline D2D1_COLOR_F ActionHoverBg(){ return D2D1::ColorF(0x0078D4, 0.12f); }
    inline D2D1_COLOR_F ActionActive() { return D2D1::ColorF(0x005A9E); }
    inline D2D1_COLOR_F Danger()       { return D2D1::ColorF(0xC42B1C); }
    inline D2D1_COLOR_F DangerBg()     { return D2D1::ColorF(0xC42B1C, 0.12f); }
    inline D2D1_COLOR_F TooltipBg()    { return D2D1::ColorF(0x1F1F1F, 0.96f); }
    inline D2D1_COLOR_F TooltipText()  { return D2D1::ColorF(0xFFFFFF); }

    // 팝업 배경 (Acrylic 폴백 시)
    inline D2D1_COLOR_F OverlayBg()   { return D2D1::ColorF(0xF2F2F2, 1.0f); }
    // 토스트 배경 (어두운)
    inline D2D1_COLOR_F ToastBg()     { return D2D1::ColorF(0x1C1C1C, 0.88f); }
    inline D2D1_COLOR_F ToastText()   { return D2D1::ColorF(0xFFFFFF); }
    inline D2D1_COLOR_F ToastSub()    { return D2D1::ColorF(0xAAAAAA); }

    // 라운드 반경
    constexpr float CornerRadius = 12.0f;
    constexpr float CardCorner   =  6.0f;
    constexpr float TagCorner    =  3.0f;

    // 카드 높이
    constexpr float CardHeight   = 56.0f;
    constexpr float HeaderHeight = 44.0f;
    constexpr float SearchHeight = 40.0f;
    constexpr float StatusHeight = 28.0f;

    // 폰트 이름
    constexpr const wchar_t* FluentFont = L"Segoe UI Variable";
    constexpr const wchar_t* IconFont   = L"Segoe MDL2 Assets";
    constexpr const wchar_t* FallbackFont = L"Segoe UI";
}
