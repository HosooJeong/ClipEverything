#pragma once
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <vector>
#include <string>

struct ClipFormat {
    UINT             formatId;
    std::wstring     formatName;
    std::vector<uint8_t> data;
};

// STA 스레드에서만 호출. 성공 시 true.
bool RestoreToClipboard(const std::vector<ClipFormat>& formats);
