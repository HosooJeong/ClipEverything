#include "clipboard_writer.h"
#include <cstring>
#include <sstream>

namespace {

struct SerializedMetaFilePictHeader {
    int32_t mm = 0;
    int32_t xExt = 0;
    int32_t yExt = 0;
    uint32_t metaBitsSize = 0;
};

struct SerializedPaletteHeader {
    uint16_t version = 0x0300;
    uint16_t entryCount = 0;
};

static UINT ResolveFormatId(UINT fmtId, const std::wstring& fmtName)
{
    if (fmtId >= 0xC000 && !fmtName.empty())
        return RegisterClipboardFormatW(fmtName.c_str());
    return fmtId;
}

static void DebugDumpFormats(const std::vector<ClipFormat>& formats, const wchar_t* phase)
{
    std::wostringstream oss;
    oss << L"[ClipEverything] " << phase << L" formats(" << formats.size() << L"): ";
    for (size_t i = 0; i < formats.size(); ++i) {
        if (i > 0) oss << L", ";
        oss << formats[i].formatName << L"(" << formats[i].formatId << L")";
    }
    oss << L"\n";
    OutputDebugStringW(oss.str().c_str());
}

static bool RestoreGlobalMemoryFormat(UINT fmtId, const std::vector<uint8_t>& data)
{
    HGLOBAL hMem = GlobalAlloc(GMEM_MOVEABLE, data.size());
    if (!hMem) return false;

    void* ptr = GlobalLock(hMem);
    if (!ptr) {
        GlobalFree(hMem);
        return false;
    }

    memcpy(ptr, data.data(), data.size());
    GlobalUnlock(hMem);

    if (!SetClipboardData(fmtId, hMem)) {
        GlobalFree(hMem);
        return false;
    }
    return true;
}

static bool RestoreEnhMetaFile(UINT fmtId, const std::vector<uint8_t>& data)
{
    HENHMETAFILE hMeta = SetEnhMetaFileBits(static_cast<UINT>(data.size()), data.data());
    if (!hMeta) return false;

    if (!SetClipboardData(fmtId, hMeta)) {
        DeleteEnhMetaFile(hMeta);
        return false;
    }
    return true;
}

static bool RestoreMetaFilePict(UINT fmtId, const std::vector<uint8_t>& data)
{
    if (data.size() < sizeof(SerializedMetaFilePictHeader)) return false;

    SerializedMetaFilePictHeader header{};
    memcpy(&header, data.data(), sizeof(header));

    size_t expectedSize = sizeof(header) + header.metaBitsSize;
    if (header.metaBitsSize == 0 || expectedSize != data.size()) return false;

    HMETAFILE hMeta = SetMetaFileBitsEx(header.metaBitsSize, data.data() + sizeof(header));
    if (!hMeta) return false;

    HGLOBAL hMem = GlobalAlloc(GMEM_MOVEABLE, sizeof(METAFILEPICT));
    if (!hMem) {
        DeleteMetaFile(hMeta);
        return false;
    }

    auto* pict = static_cast<LPMETAFILEPICT>(GlobalLock(hMem));
    if (!pict) {
        DeleteMetaFile(hMeta);
        GlobalFree(hMem);
        return false;
    }

    pict->mm = header.mm;
    pict->xExt = header.xExt;
    pict->yExt = header.yExt;
    pict->hMF = hMeta;
    GlobalUnlock(hMem);

    if (!SetClipboardData(fmtId, hMem)) {
        DeleteMetaFile(hMeta);
        GlobalFree(hMem);
        return false;
    }
    return true;
}

static bool RestorePalette(UINT fmtId, const std::vector<uint8_t>& data)
{
    if (data.size() < sizeof(SerializedPaletteHeader)) return false;

    SerializedPaletteHeader header{};
    memcpy(&header, data.data(), sizeof(header));

    size_t expectedSize = sizeof(header) + static_cast<size_t>(header.entryCount) * sizeof(PALETTEENTRY);
    if (header.entryCount == 0 || expectedSize != data.size()) return false;

    size_t logPaletteSize = sizeof(LOGPALETTE) +
        (static_cast<size_t>(header.entryCount) - 1) * sizeof(PALETTEENTRY);
    std::vector<uint8_t> paletteBytes(logPaletteSize, 0);
    auto* logPalette = reinterpret_cast<LOGPALETTE*>(paletteBytes.data());
    logPalette->palVersion = header.version;
    logPalette->palNumEntries = header.entryCount;

    memcpy(logPalette->palPalEntry,
           data.data() + sizeof(header),
           static_cast<size_t>(header.entryCount) * sizeof(PALETTEENTRY));

    HPALETTE hPalette = CreatePalette(logPalette);
    if (!hPalette) return false;

    if (!SetClipboardData(fmtId, hPalette)) {
        DeleteObject(hPalette);
        return false;
    }
    return true;
}

static bool RestoreSingleFormat(const ClipFormat& fmt)
{
    UINT fmtId = ResolveFormatId(fmt.formatId, fmt.formatName);
    if (fmtId == 0) return false;

    switch (fmt.formatId) {
        case CF_ENHMETAFILE:
            return RestoreEnhMetaFile(fmtId, fmt.data);
        case CF_METAFILEPICT:
            return RestoreMetaFilePict(fmtId, fmt.data);
        case CF_PALETTE:
            return RestorePalette(fmtId, fmt.data);
        default:
            return RestoreGlobalMemoryFormat(fmtId, fmt.data);
    }
}

} // namespace

bool RestoreToClipboard(const std::vector<ClipFormat>& formats)
{
    DebugDumpFormats(formats, L"Restoring");

    for (int attempt = 0; attempt < 3; ++attempt) {
        if (attempt > 0) Sleep(50);

        if (!OpenClipboard(nullptr)) continue;

        EmptyClipboard();

        bool allOk = true;
        for (const auto& fmt : formats) {
            if (!RestoreSingleFormat(fmt))
                allOk = false;
        }

        CloseClipboard();
        if (allOk) return true;
    }
    return false;
}
