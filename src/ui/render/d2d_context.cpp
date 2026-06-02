#include "d2d_context.h"

#pragma comment(lib, "d2d1.lib")
#pragma comment(lib, "dwrite.lib")
#pragma comment(lib, "windowscodecs.lib")

D2DContext& D2DContext::Get()
{
    static D2DContext inst;
    return inst;
}

bool D2DContext::Initialize()
{
    // Direct2D Factory (멀티스레드 안전)
    if (FAILED(D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED,
                                  IID_PPV_ARGS(&d2dFactory))))
        return false;

    // DirectWrite Factory
    if (FAILED(DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED,
                                    __uuidof(IDWriteFactory3),
                                    reinterpret_cast<IUnknown**>(dwFactory.GetAddressOf()))))
        return false;

    // WIC Factory
    if (FAILED(CoCreateInstance(CLSID_WICImagingFactory2, nullptr, CLSCTX_INPROC_SERVER,
                                 IID_PPV_ARGS(&wicFactory))))
        return false;

    return true;
}

void D2DContext::Dispose()
{
    _tfCache.clear();
    _rts.clear();
    wicFactory.Reset();
    dwFactory.Reset();
    d2dFactory.Reset();
}

ID2D1HwndRenderTarget* D2DContext::GetRenderTarget(HWND hwnd, int w, int h)
{
    auto it = _rts.find(hwnd);
    if (it != _rts.end()) return it->second.Get();

    D2D1_RENDER_TARGET_PROPERTIES rtProps = D2D1::RenderTargetProperties(
        D2D1_RENDER_TARGET_TYPE_DEFAULT,
        D2D1::PixelFormat(DXGI_FORMAT_B8G8R8A8_UNORM, D2D1_ALPHA_MODE_PREMULTIPLIED));

    D2D1_HWND_RENDER_TARGET_PROPERTIES hwndProps = D2D1::HwndRenderTargetProperties(
        hwnd, D2D1::SizeU(w, h));

    ComPtr<ID2D1HwndRenderTarget> rt;
    if (FAILED(d2dFactory->CreateHwndRenderTarget(rtProps, hwndProps, &rt)))
        return nullptr;

    rt->SetDpi(96.0f, 96.0f); // DPI는 창에서 개별 조정
    _rts[hwnd] = rt;
    return rt.Get();
}

void D2DContext::ReleaseRenderTarget(HWND hwnd)
{
    _rts.erase(hwnd);
}

void D2DContext::ResizeRenderTarget(HWND hwnd, int w, int h)
{
    auto it = _rts.find(hwnd);
    if (it != _rts.end())
        it->second->Resize(D2D1::SizeU(w, h));
}

IDWriteTextFormat* D2DContext::GetTextFormat(const wchar_t* fontName, float size, DWRITE_FONT_WEIGHT weight)
{
    TfKey key{ fontName, size, weight };
    auto it = _tfCache.find(key);
    if (it != _tfCache.end()) return it->second.Get();

    ComPtr<IDWriteTextFormat> tf;
    HRESULT hr = dwFactory->CreateTextFormat(
        fontName, nullptr, weight,
        DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_STRETCH_NORMAL,
        size, L"ko-KR", &tf);

    if (FAILED(hr)) {
        // 폴백: Segoe UI
        dwFactory->CreateTextFormat(
            L"Segoe UI", nullptr, weight,
            DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_STRETCH_NORMAL,
            size, L"ko-KR", &tf);
    }

    if (tf) {
        tf->SetTextAlignment(DWRITE_TEXT_ALIGNMENT_LEADING);
        tf->SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER);
        tf->SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP);
    }

    _tfCache[key] = tf;
    return tf.Get();
}

ComPtr<ID2D1DCRenderTarget> D2DContext::CreateDCRenderTarget()
{
    D2D1_RENDER_TARGET_PROPERTIES props = D2D1::RenderTargetProperties(
        D2D1_RENDER_TARGET_TYPE_DEFAULT,
        D2D1::PixelFormat(DXGI_FORMAT_B8G8R8A8_UNORM, D2D1_ALPHA_MODE_PREMULTIPLIED));

    ComPtr<ID2D1DCRenderTarget> rt;
    d2dFactory->CreateDCRenderTarget(&props, &rt);
    return rt;
}
