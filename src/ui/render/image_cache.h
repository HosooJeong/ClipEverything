#pragma once
#include "d2d_context.h"
#include <string>
#include <vector>

class ImageCache {
public:
    // exePath로 앱 아이콘 로드 (캐시)
    ID2D1Bitmap* GetAppIcon(ID2D1RenderTarget* rt, const std::wstring& exePath);

    // PNG 바이트 → ID2D1Bitmap
    ID2D1Bitmap* GetThumbnail(ID2D1RenderTarget* rt,
                               const std::vector<uint8_t>& pngBytes,
                               int64_t itemId);
    void Clear();

private:
    std::unordered_map<std::wstring, ComPtr<ID2D1Bitmap>> _icons;
    std::unordered_map<int64_t,      ComPtr<ID2D1Bitmap>> _thumbs;

    ComPtr<ID2D1Bitmap> LoadPngBytes(ID2D1RenderTarget* rt,
                                      const uint8_t* data, size_t size);
};
