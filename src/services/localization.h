#pragma once
#include <string>

// 앱 표시 언어. System은 OS UI 언어를 따라간다 (한국어면 ko, 그 외 en).
enum class AppLanguage { System, Korean, English };

// UI 문자열 ID. kStrings 테이블과 순서가 일치해야 한다.
enum class Str {
    // 공통/오류
    ErrorTitle,
    ErrorD2DInit,
    ErrorDbInit,
    ErrorHostWindow,
    HotkeyConflictTitle,
    HotkeyConflictMsgFmt,      // {} = "복사라벨/붙여넣기라벨"

    // 토스트
    ToastCopiedFromFmt,        // {} = 원본 앱 이름
    ToastNotSavedTitle,
    ToastNotSavedSub,

    // 트레이 메뉴
    TrayOpen,
    TraySettings,
    TrayHelp,
    TrayExit,

    // 오버레이
    OverlaySearchHint,
    OverlayNameHint,
    OverlayTagAdd,
    OverlayNoName,
    OverlayToggleAll,
    OverlayEmpty,
    OverlayCountFmt,           // {} = 항목 수
    OverlayStatusRename,
    OverlayStatusTag,
    OverlayStatusDelete,
    OverlayStatusPaste,
    ConfirmAsk,
    ConfirmDeleteBtn,
    ConfirmCancelBtn,

    // 툴팁
    TipShowCurrentOnly,
    TipShowAll,
    TipClose,
    TipRename,
    TipFavoriteAdd,
    TipFavoriteRemove,
    TipFavorite,
    TipDelete,
    TipConfirmDelete,
    TipCancelDelete,
    TipTagAdd,
    TipTagRemove,
    TipTagEdit,
    TipTagEditFmt,             // {} = 태그

    // 호버 상태바
    HoverShowCurrentOnly,
    HoverShowAll,
    HoverCloseOverlay,
    HoverDeleteItem,
    HoverConfirmDelete,

    // 설정 창
    SettingsTitle,
    SettingsGeneral,
    SettingsStartup,
    SettingsToast,
    SettingsOpenOverlay,
    SettingsLanguage,
    SettingsLangSystem,
    SettingsHotkeys,
    SettingsCopy,
    SettingsPaste,
    SettingsReset,
    SettingsData,
    SettingsDataNote,
    SettingsClearAll,
    SettingsFooterNote,
    SettingsCloseBtn,
    SettingsPressKey,
    SettingsClearConfirm,
    SettingsConfirmTitle,

    // 도움말
    HelpTitle,
    HelpClose,
    HelpBody,                  // {copy}/{paste} 플레이스홀더 포함

    Count,
};

// 설정 문자열("system"/"ko"/"en") 변환
AppLanguage ParseLanguageSetting(const std::wstring& value);
std::wstring LanguageSettingToString(AppLanguage lang);

// 활성 언어 적용/조회 (System은 OS UI 언어로 해석)
void SetAppLanguage(AppLanguage lang);
bool IsKoreanActive();

const wchar_t* Tr(Str id);

// Tr(id)의 첫 "{}"를 arg로 치환
std::wstring TrFmt(Str id, const std::wstring& arg);

// 임의 패턴의 모든 occurrences 치환 (도움말 본문 등)
std::wstring ReplaceAll(std::wstring text,
                        const std::wstring& token,
                        const std::wstring& value);
