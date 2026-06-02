#include "source_detector.h"
#include <psapi.h>
#include <shellapi.h>
#include <wincodec.h>
#include <shlwapi.h>
#include <vector>

#pragma comment(lib, "psapi.lib")
#pragma comment(lib, "shell32.lib")
#pragma comment(lib, "windowscodecs.lib")
#pragma comment(lib, "shlwapi.lib")

SourceInfo CaptureSource()
{
    SourceInfo info;
    info.hwnd = GetForegroundWindow();
    if (!info.hwnd) return info;

    // 창 제목
    wchar_t title[512] = {};
    GetWindowTextW(info.hwnd, title, 512);
    info.windowTitle = title;

    // 프로세스 ID → 경로
    DWORD pid = 0;
    GetWindowThreadProcessId(info.hwnd, &pid);

    HANDLE hProc = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, FALSE, pid);
    if (hProc) {
        wchar_t path[MAX_PATH] = {};
        DWORD sz = MAX_PATH;
        if (QueryFullProcessImageNameW(hProc, 0, path, &sz)) {
            info.exePath = path;
            // 파일명만 프로세스명으로 사용
            wchar_t* name = PathFindFileNameW(path);
            if (name) {
                std::wstring fullName = name;
                // .exe 확장자 제거
                auto dot = fullName.rfind(L'.');
                if (dot != std::wstring::npos)
                    info.processName = fullName.substr(0, dot);
                else
                    info.processName = fullName;
            }
        }
        CloseHandle(hProc);
    }

    return info;
}

std::vector<uint8_t> ExtractAppIconBytes(const std::wstring& exePath)
{
    if (exePath.empty()) return {};

    SHFILEINFOW sfi = {};
    SHGetFileInfoW(exePath.c_str(), 0, &sfi, sizeof(sfi),
                   SHGFI_ICON | SHGFI_SMALLICON);
    if (!sfi.hIcon) return {};

    // HICON → WIC → PNG bytes
    IWICImagingFactory* pFac = nullptr;
    if (FAILED(CoCreateInstance(CLSID_WICImagingFactory, nullptr, CLSCTX_INPROC_SERVER,
                                IID_IWICImagingFactory, (void**)&pFac))) {
        DestroyIcon(sfi.hIcon);
        return {};
    }

    IWICBitmap* pBmp = nullptr;
    pFac->CreateBitmapFromHICON(sfi.hIcon, &pBmp);
    DestroyIcon(sfi.hIcon);

    if (!pBmp) { pFac->Release(); return {}; }

    IStream* pStream = SHCreateMemStream(nullptr, 0);
    IWICBitmapEncoder* pEnc = nullptr;
    IWICBitmapFrameEncode* pFrame = nullptr;
    pFac->CreateEncoder(GUID_ContainerFormatPng, nullptr, &pEnc);
    pEnc->Initialize(pStream, WICBitmapEncoderNoCache);
    pEnc->CreateNewFrame(&pFrame, nullptr);
    pFrame->Initialize(nullptr);
    pFrame->WriteSource(pBmp, nullptr);
    pFrame->Commit();
    pEnc->Commit();

    STATSTG stat{};
    pStream->Stat(&stat, STATFLAG_NONAME);
    std::vector<uint8_t> png(static_cast<size_t>(stat.cbSize.QuadPart));
    LARGE_INTEGER li{}; pStream->Seek(li, STREAM_SEEK_SET, nullptr);
    ULONG read = 0; pStream->Read(png.data(), (ULONG)png.size(), &read);

    pFrame->Release(); pEnc->Release(); pStream->Release();
    pBmp->Release(); pFac->Release();
    return png;
}
