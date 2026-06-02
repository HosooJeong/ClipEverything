#include "clipboard_service.h"
#include "../core/clipboard_reader.h"
#include "../core/clipboard_writer.h"

ClipboardService::ClipboardService(Repository& repo, AppSettings& settings)
    : _repo(repo), _settings(settings)
{}

void ClipboardService::SimulateCtrl(WORD vk)
{
    // 단축키 핸들러에서 호출 시 어떤 모디파이어든 눌려있을 수 있음.
    // 8가지 모디파이어를 전부 해제한 뒤 순수 Ctrl+vk를 주입.
    INPUT inputs[12] = {};
    int i = 0;

    auto ku = [&](WORD k) {
        inputs[i].type = INPUT_KEYBOARD;
        inputs[i].ki.wVk = k;
        inputs[i].ki.dwFlags = KEYEVENTF_KEYUP;
        i++;
    };
    auto kd = [&](WORD k) {
        inputs[i].type = INPUT_KEYBOARD;
        inputs[i].ki.wVk = k;
        i++;
    };

    ku(VK_LWIN);      // 0
    ku(VK_RWIN);      // 1
    ku(VK_LCONTROL);  // 2
    ku(VK_RCONTROL);  // 3
    ku(VK_LMENU);     // 4  Alt 해제 (핵심)
    ku(VK_RMENU);     // 5
    ku(VK_LSHIFT);    // 6
    ku(VK_RSHIFT);    // 7
    kd(VK_CONTROL);   // 8  Ctrl down
    kd(vk);           // 9  key down
    ku(vk);           // 10 key up
    ku(VK_CONTROL);   // 11 Ctrl up

    SendInput(12, inputs, sizeof(INPUT));
}

void ClipboardService::OnCopyHotkey()
{
    // 1. 핫키 수신 시점의 포어그라운드 창 캡처
    SourceInfo src = CaptureSource();

    // 2. Win/Ctrl 물리 키 해제 대기 후 Ctrl+C 시뮬레이션
    //    (SimulateCtrl 내부에서 먼저 LWin/Ctrl UP을 주입하지만,
    //     SendInput이 비동기이므로 20ms 여유를 준다)
    Sleep(20);
    SimulateCtrlC();

    // 3. 클립보드 변경 대기
    Sleep(100);

    // 4. 클립보드 스냅샷 (STA 메인 스레드에서 실행 중)
    auto snap = TakeSnapshot();
    if (!snap) return;

    // 5. DB 저장
    int64_t itemId = _repo.SaveOrUpdate(*snap, src);

    // 6. UI 알림
    if (OnItemCaptured) OnItemCaptured(itemId, src);
}

void ClipboardService::OnPasteHotkey()
{
    // 팝업 열기 전 포어그라운드 창 캡처
    _pendingTarget = CaptureSource();

    if (OnPasteRequested) OnPasteRequested();
}

void ClipboardService::PasteSelectedItem(int64_t itemId)
{
    // 1. 포맷 로드
    auto fmts = _repo.GetFormats(itemId);
    if (fmts.empty()) return;

    // ClipFormat 변환
    std::vector<ClipFormat> clipFmts;
    for (auto& f : fmts)
        clipFmts.push_back({ f.formatId, f.formatName, f.data });

    // 2. 클립보드 복원
    if (!RestoreToClipboard(clipFmts)) return;

    // 3. 원래 창 포커스 복원
    if (_pendingTarget.hwnd)
        SetForegroundWindow(_pendingTarget.hwnd);
    Sleep(50);

    // 4. Ctrl+V 시뮬레이션
    SimulateCtrlV();
}
