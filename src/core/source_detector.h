#pragma once
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <string>
#include <vector>

struct SourceInfo {
    HWND         hwnd        = nullptr;
    std::wstring processName;
    std::wstring windowTitle;
    std::wstring exePath;
};

// 현재 포어그라운드 창 정보 캡처
SourceInfo CaptureSource();

// exe 경로 → PNG 아이콘 바이트 (실패 시 빈 벡터)
std::vector<uint8_t> ExtractAppIconBytes(const std::wstring& exePath);
