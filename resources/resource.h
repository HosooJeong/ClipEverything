#pragma once

// 아이콘
#define IDI_APP                 101

// 트레이 메뉴 명령 ID
#define ID_TRAY_OPEN            201
#define ID_TRAY_SETTINGS        202
#define ID_TRAY_HELP            203
#define ID_TRAY_EXIT            204

// 컨텍스트 메뉴 (카드 우클릭)
#define ID_CARD_RENAME          301
#define ID_CARD_FAVORITE        302
#define ID_CARD_TAG             303
#define ID_CARD_DELETE          304

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

// 다이얼로그 컨트롤
#define IDC_RENAME_EDIT         501
#define IDC_RENAME_OK           502
#define IDC_RENAME_CANCEL       503

#define IDC_TAG_EDIT            601
#define IDC_TAG_SAVE            602
#define IDC_TAG_CLEAR           603
#define IDC_TAG_CANCEL          604

// 사용자 메시지
#define WM_APP_TRAY             (WM_APP + 1)   // 0x8001 - 트레이 콜백
#define WM_APP_SHOW_OVERLAY     (WM_APP + 2)   // 팝업 열기 요청
#define WM_APP_SAVE_CLIP        (WM_APP + 3)   // 워커: DB 저장
#define WM_APP_CLIP_SAVED       (WM_APP + 4)   // 워커 → 메인: 저장 완료
