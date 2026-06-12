#pragma once
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <functional>
#include <string>
#include "../core/source_detector.h"
#include "../data/repository.h"
#include "app_settings.h"

class ClipboardService {
public:
    // 콜백
    std::function<void()>                          OnPasteRequested;
    std::function<void(int64_t, const SourceInfo&)> OnItemCaptured;
    std::function<void()>                          OnSensitiveSkipped; // 비밀번호 매니저 등 저장 제외

    ClipboardService(Repository& repo, AppSettings& settings);

    // HotkeyManager 콜백으로 등록
    void OnCopyHotkey();
    void OnPasteHotkey();

    // OverlayWindow에서 항목 선택 시 호출
    void PasteSelectedItem(int64_t itemId);

    const SourceInfo& GetPendingTarget() const { return _pendingTarget; }

private:
    Repository&   _repo;
    AppSettings&  _settings;
    SourceInfo    _pendingTarget;

    static void SimulateCtrl(WORD vk);
    static void SimulateCtrlC() { SimulateCtrl(0x43); }
    static void SimulateCtrlV() { SimulateCtrl(0x56); }
};
