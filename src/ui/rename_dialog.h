#pragma once
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <string>

// 이름 변경 모달 다이얼로그. 반환값: 새 이름 (취소 시 빈 문자열)
std::wstring ShowRenameDialog(HINSTANCE hInst, HWND hParent, const std::wstring& current);
bool RegisterRenameClass(HINSTANCE hInst);
