#include "clipboard_reader.h"
#include <bcrypt.h>
#include <shellapi.h>
#include <wincodec.h>
#include <shlwapi.h>
#include <algorithm>
#include <cwctype>
#include <cstring>
#include <iomanip>
#include <sstream>

#pragma comment(lib, "bcrypt.lib")
#pragma comment(lib, "windowscodecs.lib")
#pragma comment(lib, "shlwapi.lib")

namespace {

struct Sha256State {
    BCRYPT_ALG_HANDLE alg = nullptr;
    BCRYPT_HASH_HANDLE hash = nullptr;
    DWORD hashLen = 0;
    bool valid = false;

    Sha256State()
    {
        DWORD cbData = 0;
        if (BCryptOpenAlgorithmProvider(&alg, BCRYPT_SHA256_ALGORITHM, nullptr, 0) != 0)
            return;
        if (BCryptGetProperty(alg, BCRYPT_HASH_LENGTH, reinterpret_cast<PUCHAR>(&hashLen),
                              sizeof(hashLen), &cbData, 0) != 0)
            return;
        if (BCryptCreateHash(alg, &hash, nullptr, 0, nullptr, 0, 0) != 0)
            return;
        valid = true;
    }

    ~Sha256State()
    {
        if (hash) BCryptDestroyHash(hash);
        if (alg) BCryptCloseAlgorithmProvider(alg, 0);
    }

    bool Update(const void* data, size_t size)
    {
        if (!valid || !data || size == 0) return valid;
        return BCryptHashData(hash, reinterpret_cast<PUCHAR>(const_cast<void*>(data)),
                              static_cast<ULONG>(size), 0) == 0;
    }

    std::string Finish()
    {
        if (!valid) return {};

        std::vector<uint8_t> hashBuf(hashLen);
        if (BCryptFinishHash(hash, hashBuf.data(), hashLen, 0) != 0)
            return {};

        std::ostringstream ss;
        for (auto b : hashBuf) {
            ss << std::hex << std::setw(2) << std::setfill('0') << static_cast<int>(b);
        }
        return ss.str();
    }
};

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

static bool ShouldSkip(UINT fmt)
{
    // CF_BITMAP/CF_OEMTEXT는 CF_DIB/CF_TEXT에서 합성되는 포맷이라 원본만 저장
    switch (fmt) {
        case CF_BITMAP:
        case CF_OEMTEXT:
            return true;
    }
    return false;
}

// 비밀번호 매니저 등이 "클립보드 기록 도구는 저장하지 마라"고 알리는 표준 신호.
// 클립보드가 이미 열린 상태에서 호출해야 한다.
static bool HasSensitiveExclusionSignal()
{
    static const UINT fmtExclude =
        RegisterClipboardFormatW(L"ExcludeClipboardContentFromMonitorProcessing");
    static const UINT fmtIgnore =
        RegisterClipboardFormatW(L"Clipboard Viewer Ignore");
    static const UINT fmtCanInclude =
        RegisterClipboardFormatW(L"CanIncludeInClipboardHistory");

    if (IsClipboardFormatAvailable(fmtExclude) ||
        IsClipboardFormatAvailable(fmtIgnore))
        return true;

    // CanIncludeInClipboardHistory: DWORD 0이면 기록 제외 요청
    if (IsClipboardFormatAvailable(fmtCanInclude)) {
        if (HANDLE hData = GetClipboardData(fmtCanInclude)) {
            if (GlobalSize(hData) >= sizeof(DWORD)) {
                if (auto* value = static_cast<const DWORD*>(GlobalLock(hData))) {
                    const bool excluded = (*value == 0);
                    GlobalUnlock(hData);
                    return excluded;
                }
            }
        }
    }
    return false;
}

static std::string ComputeSha256(const void* data, size_t size)
{
    Sha256State sha;
    if (!sha.valid) return {};
    if (!sha.Update(data, size)) return {};
    return sha.Finish();
}

static std::wstring NormalizePathForHash(std::wstring path)
{
    std::replace(path.begin(), path.end(), L'/', L'\\');
    while (path.length() > 3 && !path.empty() && path.back() == L'\\')
        path.pop_back();
    std::transform(path.begin(), path.end(), path.begin(), [](wchar_t ch) {
        return static_cast<wchar_t>(towlower(ch));
    });
    return path;
}

static std::wstring ResolveFormatName(UINT fmt)
{
    if (fmt < 0xC000) {
        switch (fmt) {
            case CF_TEXT:         return L"Text";
            case CF_BITMAP:       return L"Bitmap";
            case CF_METAFILEPICT: return L"MetafilePicture";
            case CF_TIFF:         return L"TIFF";
            case CF_OEMTEXT:      return L"OEMText";
            case CF_DIB:          return L"DeviceIndependentBitmap";
            case CF_PALETTE:      return L"Palette";
            case CF_UNICODETEXT:  return L"UnicodeText";
            case CF_ENHMETAFILE:  return L"EnhancedMetafile";
            case CF_HDROP:        return L"FileDrop";
            case CF_LOCALE:       return L"Locale";
            case CF_DIBV5:        return L"DeviceIndependentBitmapV5";
            default: {
                wchar_t buf[64];
                swprintf_s(buf, L"Format%u", fmt);
                return buf;
            }
        }
    }

    wchar_t buf[256] = {};
    if (GetClipboardFormatNameW(fmt, buf, static_cast<int>(std::size(buf))) > 0)
        return buf;

    wchar_t fallback[64];
    swprintf_s(fallback, L"CustomFormat%u", fmt);
    return fallback;
}

static void DebugDumpFormats(const std::vector<RawClipboardFormat>& formats, const wchar_t* phase)
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

static bool CaptureGlobalMemoryFormat(HANDLE hData, std::vector<uint8_t>& out)
{
    SIZE_T sz = GlobalSize(hData);
    if (sz == 0) return false;

    void* ptr = GlobalLock(hData);
    if (!ptr) return false;

    out.resize(sz);
    memcpy(out.data(), ptr, sz);
    GlobalUnlock(hData);
    return true;
}

static bool CaptureEnhMetaFile(HANDLE hData, std::vector<uint8_t>& out)
{
    HENHMETAFILE hMeta = reinterpret_cast<HENHMETAFILE>(hData);
    UINT sz = GetEnhMetaFileBits(hMeta, 0, nullptr);
    if (sz == 0) return false;

    out.resize(sz);
    return GetEnhMetaFileBits(hMeta, sz, out.data()) == sz;
}

static bool CaptureMetaFilePict(HANDLE hData, std::vector<uint8_t>& out)
{
    HGLOBAL hMem = reinterpret_cast<HGLOBAL>(hData);
    auto* pict = static_cast<LPMETAFILEPICT>(GlobalLock(hMem));
    if (!pict || !pict->hMF) {
        if (pict) GlobalUnlock(hMem);
        return false;
    }

    UINT bitsSize = GetMetaFileBitsEx(pict->hMF, 0, nullptr);
    if (bitsSize == 0) {
        GlobalUnlock(hMem);
        return false;
    }

    SerializedMetaFilePictHeader header;
    header.mm = pict->mm;
    header.xExt = pict->xExt;
    header.yExt = pict->yExt;
    header.metaBitsSize = bitsSize;

    out.resize(sizeof(header) + bitsSize);
    memcpy(out.data(), &header, sizeof(header));
    bool ok = GetMetaFileBitsEx(pict->hMF, bitsSize, out.data() + sizeof(header)) == bitsSize;
    GlobalUnlock(hMem);
    return ok;
}

static bool CapturePalette(HANDLE hData, std::vector<uint8_t>& out)
{
    HPALETTE hPalette = reinterpret_cast<HPALETTE>(hData);
    UINT entryCount = GetPaletteEntries(hPalette, 0, 0, nullptr);
    if (entryCount == 0 || entryCount > 0xFFFF) return false;

    SerializedPaletteHeader header;
    header.entryCount = static_cast<uint16_t>(entryCount);

    out.resize(sizeof(header) + entryCount * sizeof(PALETTEENTRY));
    memcpy(out.data(), &header, sizeof(header));

    auto* entries = reinterpret_cast<LPPALETTEENTRY>(out.data() + sizeof(header));
    return GetPaletteEntries(hPalette, 0, entryCount, entries) == entryCount;
}

static std::optional<std::string> ComputeFileDropHash()
{
    HANDLE hData = GetClipboardData(CF_HDROP);
    if (!hData) return std::nullopt;

    HDROP hDrop = reinterpret_cast<HDROP>(hData);
    UINT fileCount = DragQueryFileW(hDrop, 0xFFFFFFFF, nullptr, 0);
    if (fileCount == 0) return std::nullopt;

    std::vector<std::wstring> paths;
    paths.reserve(fileCount);

    for (UINT i = 0; i < fileCount; ++i) {
        UINT len = DragQueryFileW(hDrop, i, nullptr, 0);
        if (len == 0) continue;

        std::vector<wchar_t> buffer(len + 1, L'\0');
        if (DragQueryFileW(hDrop, i, buffer.data(), len + 1) == 0) continue;

        paths.push_back(NormalizePathForHash(buffer.data()));
    }

    if (paths.empty()) return std::nullopt;

    std::sort(paths.begin(), paths.end());

    std::wstring joined = L"file-drop\n";
    for (const auto& path : paths) {
        joined += path;
        joined.push_back(L'\n');
    }

    return ComputeSha256(joined.data(), joined.size() * sizeof(wchar_t));
}

static std::string ComputeFormatsHash(const std::vector<RawClipboardFormat>& formats)
{
    Sha256State sha;
    if (!sha.valid) return {};

    uint32_t count = static_cast<uint32_t>(formats.size());
    if (!sha.Update(&count, sizeof(count))) return {};

    for (const auto& format : formats) {
        uint32_t formatId = format.formatId;
        uint32_t nameChars = static_cast<uint32_t>(format.formatName.size());
        uint64_t dataSize = static_cast<uint64_t>(format.data.size());

        if (!sha.Update(&formatId, sizeof(formatId))) return {};
        if (!sha.Update(&nameChars, sizeof(nameChars))) return {};
        if (nameChars > 0 &&
            !sha.Update(format.formatName.data(), nameChars * sizeof(wchar_t)))
            return {};
        if (!sha.Update(&dataSize, sizeof(dataSize))) return {};
        if (dataSize > 0 &&
            !sha.Update(format.data.data(), static_cast<size_t>(dataSize)))
            return {};
    }

    return sha.Finish();
}

// 클립보드 DIB는 다른 프로세스가 만든 비신뢰 데이터 — 헤더를 검증한 뒤 사용한다.
static bool ValidateDib(const uint8_t* dibData, size_t dibSize, DWORD& pixelOffset)
{
    if (!dibData || dibSize < sizeof(BITMAPINFOHEADER))
        return false;

    const BITMAPINFOHEADER* bih = reinterpret_cast<const BITMAPINFOHEADER*>(dibData);
    if (bih->biSize < sizeof(BITMAPINFOHEADER) || bih->biSize > dibSize)
        return false;
    if (bih->biWidth <= 0 || bih->biWidth > 32768 ||
        bih->biHeight == 0 || bih->biHeight > 32768 || bih->biHeight < -32768)
        return false;
    if (bih->biBitCount > 32)
        return false;

    uint64_t offset = bih->biSize;
    if (bih->biClrUsed > 0) {
        if (bih->biClrUsed > 256) return false;
        offset += static_cast<uint64_t>(bih->biClrUsed) * 4;
    } else if (bih->biBitCount > 0 && bih->biBitCount <= 8) {
        offset += (1ull << bih->biBitCount) * 4;
    }
    if (bih->biCompression == BI_BITFIELDS)
        offset += 12;

    if (offset >= dibSize)
        return false;

    pixelOffset = static_cast<DWORD>(offset);
    return true;
}

static std::vector<uint8_t> MakeThumbnail(const uint8_t* dibData, size_t dibSize)
{
    DWORD pixelOffset = 0;
    if (!ValidateDib(dibData, dibSize, pixelOffset))
        return {};

    IWICImagingFactory* pFactory = nullptr;
    if (FAILED(CoCreateInstance(CLSID_WICImagingFactory, nullptr, CLSCTX_INPROC_SERVER,
                                IID_IWICImagingFactory, reinterpret_cast<void**>(&pFactory))))
        return {};

    BITMAPINFO* bmi = reinterpret_cast<BITMAPINFO*>(const_cast<uint8_t*>(dibData));

    HDC hdc = GetDC(nullptr);
    void* bits = nullptr;
    HBITMAP hBmp = CreateDIBSection(hdc, bmi, DIB_RGB_COLORS, &bits, nullptr, 0);
    ReleaseDC(nullptr, hdc);
    if (!hBmp || !bits) {
        if (hBmp) DeleteObject(hBmp);
        pFactory->Release();
        return {};
    }

    memcpy(bits, dibData + pixelOffset, dibSize - pixelOffset);

    IWICBitmap* pBitmap = nullptr;
    pFactory->CreateBitmapFromHBITMAP(hBmp, nullptr, WICBitmapUseAlpha, &pBitmap);
    DeleteObject(hBmp);

    if (!pBitmap) {
        pFactory->Release();
        return {};
    }

    std::vector<uint8_t> png;
    IWICBitmapScaler* pScaler = nullptr;
    IStream* pStream = nullptr;
    IWICBitmapEncoder* pEnc = nullptr;
    IWICBitmapFrameEncode* pFrame = nullptr;

    bool ok = SUCCEEDED(pFactory->CreateBitmapScaler(&pScaler)) &&
              SUCCEEDED(pScaler->Initialize(pBitmap, 40, 40, WICBitmapInterpolationModeFant)) &&
              (pStream = SHCreateMemStream(nullptr, 0)) != nullptr &&
              SUCCEEDED(pFactory->CreateEncoder(GUID_ContainerFormatPng, nullptr, &pEnc)) &&
              SUCCEEDED(pEnc->Initialize(pStream, WICBitmapEncoderNoCache)) &&
              SUCCEEDED(pEnc->CreateNewFrame(&pFrame, nullptr)) &&
              SUCCEEDED(pFrame->Initialize(nullptr)) &&
              SUCCEEDED(pFrame->WriteSource(pScaler, nullptr)) &&
              SUCCEEDED(pFrame->Commit()) &&
              SUCCEEDED(pEnc->Commit());

    if (ok) {
        STATSTG stat{};
        if (SUCCEEDED(pStream->Stat(&stat, STATFLAG_NONAME)) && stat.cbSize.QuadPart > 0) {
            png.resize(static_cast<size_t>(stat.cbSize.QuadPart));
            LARGE_INTEGER li{};
            pStream->Seek(li, STREAM_SEEK_SET, nullptr);
            ULONG read = 0;
            if (FAILED(pStream->Read(png.data(), static_cast<ULONG>(png.size()), &read)) ||
                read != png.size())
                png.clear();
        }
    }

    if (pFrame)  pFrame->Release();
    if (pEnc)    pEnc->Release();
    if (pStream) pStream->Release();
    if (pScaler) pScaler->Release();
    pBitmap->Release();
    pFactory->Release();
    return png;
}

static ContentType DetectContentType(const std::vector<RawClipboardFormat>& fmts)
{
    bool hasText = false, hasDib = false;
    for (const auto& f : fmts) {
        const auto& name = f.formatName;
        if (name.find(L"Biff") != std::wstring::npos ||
            name.find(L"XML Spreadsheet") != std::wstring::npos)
            return ContentType::Excel;
        if (name.find(L"HWP") != std::wstring::npos)
            return ContentType::Hwp;
        if (name == L"HTML Format")
            return ContentType::Html;
        if (name.find(L"Rich Text") != std::wstring::npos ||
            name.find(L"RTF") != std::wstring::npos)
            return ContentType::RichText;
        if (f.formatId == CF_DIB || f.formatId == CF_DIBV5 || f.formatId == CF_TIFF)
            hasDib = true;
        if (f.formatId == CF_UNICODETEXT || f.formatId == CF_TEXT)
            hasText = true;
    }
    if (hasDib) return ContentType::Image;
    if (hasText) return ContentType::Text;
    return ContentType::Other;
}

} // namespace

std::optional<ClipboardSnapshot> TakeSnapshot(bool* excludedSensitive)
{
    if (excludedSensitive) *excludedSensitive = false;

    if (!OpenClipboard(nullptr)) return std::nullopt;

    if (HasSensitiveExclusionSignal()) {
        CloseClipboard();
        if (excludedSensitive) *excludedSensitive = true;
        return std::nullopt;
    }

    ClipboardSnapshot snap;
    std::vector<uint8_t> dibData;
    std::optional<std::string> fileDropHash = ComputeFileDropHash();

    UINT fmt = 0;
    while ((fmt = EnumClipboardFormats(fmt)) != 0) {
        if (ShouldSkip(fmt)) continue;

        HANDLE hData = GetClipboardData(fmt);
        if (!hData) continue;

        RawClipboardFormat rf;
        rf.formatId = fmt;
        rf.formatName = ResolveFormatName(fmt);

        bool captured = false;
        switch (fmt) {
            case CF_ENHMETAFILE:
                captured = CaptureEnhMetaFile(hData, rf.data);
                break;
            case CF_METAFILEPICT:
                captured = CaptureMetaFilePict(hData, rf.data);
                break;
            case CF_PALETTE:
                captured = CapturePalette(hData, rf.data);
                break;
            default:
                captured = CaptureGlobalMemoryFormat(hData, rf.data);
                break;
        }
        if (!captured) continue;

        if (fmt == CF_DIB) {
            dibData = rf.data;
        } else if (fmt == CF_DIBV5) {
            if (dibData.empty()) dibData = rf.data;
        }

        snap.formats.push_back(std::move(rf));
    }

    CloseClipboard();

    if (snap.formats.empty()) return std::nullopt;

    if (fileDropHash && !fileDropHash->empty()) {
        snap.contentHash = *fileDropHash;
    } else {
        snap.contentHash = ComputeFormatsHash(snap.formats);
    }

    snap.contentType = DetectContentType(snap.formats);

    if (snap.contentType == ContentType::Image && !dibData.empty())
        snap.thumbnail = MakeThumbnail(dibData.data(), dibData.size());

    DebugDumpFormats(snap.formats, L"Captured");
    return snap;
}
