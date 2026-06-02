#pragma once
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <d2d1_1.h>
#include <dwrite_3.h>
#include <wincodec.h>
#include <wrl/client.h>
#include <unordered_map>
#include <string>

using Microsoft::WRL::ComPtr;

class D2DContext {
public:
    static D2DContext& Get();

    bool Initialize();
    void Dispose();

    // 창별 HwndRenderTarget 생성/반환
    ID2D1HwndRenderTarget* GetRenderTarget(HWND hwnd, int w, int h);
    void ReleaseRenderTarget(HWND hwnd);
    void ResizeRenderTarget(HWND hwnd, int w, int h);

    // 텍스트 포맷 캐시
    IDWriteTextFormat* GetTextFormat(const wchar_t* fontName, float size,
                                     DWRITE_FONT_WEIGHT weight = DWRITE_FONT_WEIGHT_NORMAL);

    // DCRenderTarget (Layered Window용)
    ComPtr<ID2D1DCRenderTarget> CreateDCRenderTarget();

    ComPtr<ID2D1Factory1>      d2dFactory;
    ComPtr<IDWriteFactory3>    dwFactory;
    ComPtr<IWICImagingFactory2> wicFactory;

private:
    D2DContext() = default;

    struct RtKey { HWND hwnd; };
    std::unordered_map<HWND, ComPtr<ID2D1HwndRenderTarget>> _rts;

    struct TfKey {
        std::wstring font;
        float size;
        DWRITE_FONT_WEIGHT weight;
        bool operator==(const TfKey& o) const {
            return font == o.font && size == o.size && weight == o.weight;
        }
    };
    struct TfKeyHash {
        size_t operator()(const TfKey& k) const {
            return std::hash<std::wstring>{}(k.font)
                 ^ std::hash<float>{}(k.size)
                 ^ std::hash<int>{}((int)k.weight);
        }
    };
    std::unordered_map<TfKey, ComPtr<IDWriteTextFormat>, TfKeyHash> _tfCache;
};
