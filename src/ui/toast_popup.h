#pragma once
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include "../data/models.h"

void ShowToastPopup(HINSTANCE hInst, const ClipboardItem& item);
bool RegisterToastClass(HINSTANCE hInst);
