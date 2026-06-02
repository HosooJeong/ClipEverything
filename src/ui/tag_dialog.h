#pragma once
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <string>

bool RegisterTagClass(HINSTANCE hInst);
// 반환값: 저장된 태그 문자열 (취소 시 빈 문자열, isCancelled=true)
std::wstring ShowTagDialog(HINSTANCE hInst, HWND hParent,
                            const std::wstring& currentTags, bool& isCancelled);
