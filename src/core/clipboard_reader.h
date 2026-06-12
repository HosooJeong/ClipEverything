#pragma once
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <string>
#include <vector>
#include <optional>

enum class ContentType { Text, RichText, Html, Excel, Hwp, Image, Other };

struct RawClipboardFormat {
    UINT             formatId;
    std::wstring     formatName;
    std::vector<uint8_t> data;
};

struct ClipboardSnapshot {
    std::vector<RawClipboardFormat> formats;
    std::string  contentHash;   // SHA256 hex (중복 감지용)
    ContentType  contentType = ContentType::Other;
    std::vector<uint8_t> thumbnail; // 40x40 PNG (이미지 클립만)
    std::wstring previewText;       // CF_UNICODETEXT 앞부분 (표시/검색용, 최대 512자)
};

// STA 스레드(메인 스레드)에서만 호출 가능.
// 비밀번호 매니저 등이 저장 제외 신호 포맷을 올린 경우 nullopt를 반환하고
// excludedSensitive(전달 시)를 true로 설정한다.
std::optional<ClipboardSnapshot> TakeSnapshot(bool* excludedSensitive = nullptr);
