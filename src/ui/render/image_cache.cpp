#include "image_cache.h"
#include "../../core/source_detector.h"
#include <shlwapi.h>

#pragma comment(lib, "shlwapi.lib")

ComPtr<ID2D1Bitmap> ImageCache::LoadPngBytes(ID2D1RenderTarget* rt,
                                              const uint8_t* data, size_t size)
{
    auto& ctx = D2DContext::Get();
    if (!ctx.wicFactory || !data || size == 0) return nullptr;

    ComPtr<IWICStream> stream;
    ctx.wicFactory->CreateStream(&stream);
    stream->InitializeFromMemory((BYTE*)data, (DWORD)size);

    ComPtr<IWICBitmapDecoder> decoder;
    ctx.wicFactory->CreateDecoderFromStream(stream.Get(), nullptr,
                                             WICDecodeMetadataCacheOnLoad, &decoder);
    if (!decoder) return nullptr;

    ComPtr<IWICBitmapFrameDecode> frame;
    decoder->GetFrame(0, &frame);
    if (!frame) return nullptr;

    ComPtr<IWICFormatConverter> conv;
    ctx.wicFactory->CreateFormatConverter(&conv);
    conv->Initialize(frame.Get(), GUID_WICPixelFormat32bppPBGRA,
                     WICBitmapDitherTypeNone, nullptr, 0.0,
                     WICBitmapPaletteTypeMedianCut);

    ComPtr<ID2D1Bitmap> bmp;
    rt->CreateBitmapFromWicBitmap(conv.Get(), nullptr, &bmp);
    return bmp;
}

ID2D1Bitmap* ImageCache::GetAppIcon(ID2D1RenderTarget* rt, const std::wstring& exePath)
{
    if (exePath.empty()) return nullptr;

    auto it = _icons.find(exePath);
    if (it != _icons.end()) return it->second.Get();

    // exePath → PNG bytes (source_detector)
    auto png = ExtractAppIconBytes(exePath);
    if (png.empty()) {
        _icons[exePath] = nullptr;
        return nullptr;
    }

    auto bmp = LoadPngBytes(rt, png.data(), png.size());
    _icons[exePath] = bmp;
    return bmp.Get();
}

ID2D1Bitmap* ImageCache::GetThumbnail(ID2D1RenderTarget* rt,
                                       const std::vector<uint8_t>& pngBytes,
                                       int64_t itemId)
{
    auto it = _thumbs.find(itemId);
    if (it != _thumbs.end()) return it->second.Get();

    if (pngBytes.empty()) { _thumbs[itemId] = nullptr; return nullptr; }
    auto bmp = LoadPngBytes(rt, pngBytes.data(), pngBytes.size());
    _thumbs[itemId] = bmp;
    return bmp.Get();
}

void ImageCache::Clear()
{
    _icons.clear();
    _thumbs.clear();
}
