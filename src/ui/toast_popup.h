#pragma once
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <string>
#include "../data/models.h"

void ShowToastPopup(HINSTANCE hInst, const ClipboardItem& item);
void ShowToastMessage(HINSTANCE hInst, const std::wstring& title, const std::wstring& subtitle);
bool RegisterToastClass(HINSTANCE hInst);
