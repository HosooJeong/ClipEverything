#pragma once
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <functional>
#include "../services/app_settings.h"
#include "../core/hotkey_manager.h"
#include "../data/repository.h"

bool RegisterSettingsClass(HINSTANCE hInst);

void ShowSettingsWindow(HINSTANCE hInst, HWND hParent,
                        AppSettings& settings, Repository& repo,
                        std::function<void(const HotkeyConfig&)> onHotkeyChanged);
