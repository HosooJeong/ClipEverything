#pragma once

// 아이콘
#define IDI_APP                 101

// 트레이 메뉴 명령 ID
#define ID_TRAY_OPEN            201
#define ID_TRAY_SETTINGS        202
#define ID_TRAY_HELP            203
#define ID_TRAY_EXIT            204

// 설정 창 컨트롤
#define IDC_STARTUP_CHECK       401
#define IDC_TOAST_CHECK         402
#define IDC_HOTKEY_COPY         403
#define IDC_HOTKEY_PASTE        404
#define IDC_CLEAR_ALL           405
#define IDC_SETTINGS_CLOSE      406
#define IDC_CONFLICT_LABEL      407
#define IDC_RESET_COPY          408
#define IDC_RESET_PASTE         409
#define IDC_OPEN_OVERLAY_CHECK  410
#define IDC_LANGUAGE_COMBO      411

// 사용자 메시지
#define WM_APP_TRAY             (WM_APP + 1)   // 0x8001 - 트레이 콜백
#define WM_APP_SHOW_OVERLAY     (WM_APP + 2)   // 팝업 열기 요청
#define WM_APP_CLIP_SAVED       (WM_APP + 4)   // 저장 완료 → 토스트 표시
