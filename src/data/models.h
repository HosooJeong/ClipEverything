#pragma once
#include <string>
#include <vector>
#include <optional>
#include <cstdint>
#include "../core/clipboard_reader.h"

struct ClipboardItem {
    int64_t      id            = 0;
    std::optional<std::wstring> name;
    std::wstring sourceApp;
    std::wstring sourceWindow;
    std::wstring exePath;
    std::string  contentHash;
    ContentType  contentType   = ContentType::Other;
    std::wstring tags;
    bool         isFavorite    = false;
    std::wstring createdAt;    // ISO 8601 UTC
    std::wstring lastCopiedAt;
    std::vector<uint8_t> thumbnail; // 40x40 PNG, 없으면 빈 벡터

    // 표시용 이름 (name이 있으면 name, 없으면 sourceWindow)
    std::wstring DisplayName() const {
        if (name && !name->empty()) return *name;
        if (!sourceWindow.empty()) return sourceWindow;
        return sourceApp;
    }

    // Segoe Fluent Icons 글리프
    const wchar_t* ContentTypeIcon() const {
        switch (contentType) {
            case ContentType::Image:    return L"\uEB9F"; // Photo
            case ContentType::Excel:    return L"\uF000"; // Table
            case ContentType::Hwp:      return L"\uE8A5"; // Document
            case ContentType::Html:     return L"\uE774"; // Globe
            case ContentType::RichText: return L"\uE8D2"; // Font
            default:                    return L"\uE8C1"; // Page
        }
    }
};

struct ClipboardFormat {
    int64_t      id     = 0;
    int64_t      itemId = 0;
    uint32_t     formatId;
    std::wstring formatName;
    std::vector<uint8_t> data;
};
