#pragma once

#define WIN32_LEAN_AND_MEAN
#include <windows.h>

struct AppSettings;

bool RegisterHelpClass(HINSTANCE hInst);
void ShowHelpWindow(HINSTANCE hInst, HWND hParent, const AppSettings& settings);
