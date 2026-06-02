#pragma once
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <string>
#include "../data/models.h"

// 우하단 토스트 팝업 표시 (Layered Window)
void ShowToast(HINSTANCE hInst, const ClipboardItem& item);
