#include "localization.h"

#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <iterator>

namespace {

struct Entry {
    const wchar_t* ko;
    const wchar_t* en;
};

// Str enum과 같은 순서
constexpr Entry kStrings[] = {
    // 공통/오류
    { L"오류", L"Error" },
    { L"Direct2D 초기화에 실패했습니다.", L"Failed to initialize Direct2D." },
    { L"데이터베이스 초기화에 실패했습니다.", L"Failed to initialize the database." },
    { L"호스트 창 생성에 실패했습니다.", L"Failed to create the host window." },
    { L"단축키 충돌", L"Hotkey conflict" },
    { L"{} 등록 실패. 설정에서 단축키를 변경해주세요.",
      L"Failed to register {}. Change the hotkey in Settings." },

    // 토스트
    { L"{}에서 복사됨", L"Copied from {}" },
    { L"저장하지 않음", L"Not saved" },
    { L"민감 항목 신호가 있어 기록에서 제외했습니다.",
      L"Excluded from history due to a sensitive-content signal." },

    // 트레이 메뉴
    { L"클립보드 열기", L"Open clipboard" },
    { L"설정", L"Settings" },
    { L"도움말", L"Help" },
    { L"앱 종료", L"Exit" },

    // 오버레이
    { L"검색...", L"Search..." },
    { L"관리명 입력", L"Enter a name" },
    { L"+ 태그", L"+ Tag" },
    { L"관리명 없음", L"Untitled" },
    { L"전체 포함", L"All apps" },
    { L"클립보드 항목이 없습니다.", L"No clipboard items." },
    { L"총 {}개", L"{} items" },
    { L"관리명 입력  Enter 저장  Esc 취소", L"Name  Enter save  Esc cancel" },
    { L"태그 입력  Enter 추가  Esc 취소", L"Tag  Enter add  Esc cancel" },
    { L"항목 삭제 확인  Esc 취소", L"Confirm delete  Esc cancel" },
    { L"Enter 붙여넣기", L"Enter to paste" },
    { L"삭제할까요?", L"Delete?" },
    { L"삭제", L"Delete" },
    { L"취소", L"Cancel" },

    // 툴팁
    { L"현재 앱만 보기", L"Show current app only" },
    { L"전체 목록 보기", L"Show all items" },
    { L"닫기", L"Close" },
    { L"관리명 변경", L"Rename" },
    { L"즐겨찾기 추가", L"Add favorite" },
    { L"즐겨찾기 해제", L"Remove favorite" },
    { L"즐겨찾기", L"Favorite" },
    { L"삭제", L"Delete" },
    { L"삭제 확정", L"Confirm delete" },
    { L"삭제 취소", L"Cancel delete" },
    { L"태그 추가", L"Add tag" },
    { L"태그 삭제", L"Remove tag" },
    { L"태그 수정", L"Edit tag" },
    { L"태그 수정: {}", L"Edit tag: {}" },

    // 호버 상태바
    { L"현재 앱 목록만 보기", L"Show current app only" },
    { L"전체 목록 보기", L"Show all items" },
    { L"오버레이 닫기", L"Close overlay" },
    { L"항목 삭제", L"Delete item" },
    { L"이 항목을 영구 삭제", L"Permanently delete this item" },

    // 설정 창
    { L"ClipEverything 설정", L"ClipEverything Settings" },
    { L"일반", L"General" },
    { L"Windows 시작 시 자동 실행", L"Run at Windows startup" },
    { L"복사 시 토스트 알림", L"Show toast on copy" },
    { L"복사 후 오버레이 열고 이름 입력", L"Open overlay to name item after copy" },
    { L"언어", L"Language" },
    { L"시스템 기본값", L"System default" },
    { L"단축키", L"Hotkeys" },
    { L"복사", L"Copy" },
    { L"붙여넣기", L"Paste" },
    { L"초기화", L"Reset" },
    { L"데이터 관리", L"Data" },
    { L"저장된 클립 기록만 삭제합니다.\r\n앱 설정은 유지됩니다.",
      L"Deletes saved clip history only.\r\nApp settings are kept." },
    { L"모든 클립 삭제", L"Delete all clips" },
    { L"단축키 변경은 닫기 시 적용됩니다.", L"Hotkey changes apply when closing." },
    { L"닫기", L"Close" },
    { L"키를 누르세요...", L"Press a key..." },
    { L"모든 클립보드 기록을 삭제하시겠습니까?", L"Delete all clipboard history?" },
    { L"확인", L"Confirm" },

    // 도움말
    { L"ClipEverything 도움말", L"ClipEverything Help" },
    { L"닫기", L"Close" },
    {
        // 한국어 도움말 본문
        L"ClipEverything은 복사한 텍스트, 파일, 이미지 등의 기록을 저장해 두었다가 다시 빠르게 붙여넣을 수 있도록 도와주는 앱입니다.\r\n\r\n"
        L"[기본 단축키]\r\n"
        L"- 복사: {copy}\r\n"
        L"- 붙여넣기: {paste}\r\n\r\n"
        L"[복사와 저장]\r\n"
        L"- 복사할 내용을 선택한 뒤 복사 단축키를 누르면 현재 클립보드 내용이 기록으로 저장됩니다.\r\n"
        L"- 복사 직후 오버레이가 열리며, 방금 저장한 항목의 관리명을 바로 입력할 수 있습니다. (설정에서 끌 수 있습니다)\r\n"
        L"- 같은 내용이면 새 항목을 만들지 않고 기존 항목의 최근 복사 시각과 출처만 갱신합니다.\r\n"
        L"- 비밀번호 매니저 등이 저장 제외 신호를 보낸 항목은 기록하지 않습니다.\r\n\r\n"
        L"[오버레이 사용]\r\n"
        L"- 붙여넣기 단축키를 누르면 오버레이가 열립니다.\r\n"
        L"- 기본적으로 현재 프로그램 기준 목록을 보여 주며, 우측 상단의 '전체 포함'으로 전체 기록을 볼 수 있습니다.\r\n"
        L"- 검색창에서는 관리명, 복사한 내용, 프로그램명, #태그로 검색할 수 있습니다.\r\n"
        L"- 항목을 클릭하거나 Enter를 누르면 붙여넣기하고, Esc를 누르면 오버레이를 닫습니다.\r\n"
        L"- 항목이 많으면 세로 스크롤로 이동할 수 있습니다.\r\n"
        L"- 오버레이는 헤더를 드래그해서 이동할 수 있고, 상단/하단 가장자리로 세로 크기를 조절할 수 있습니다.\r\n\r\n"
        L"[항목에서 할 수 있는 일]\r\n"
        L"- 관리명 수정\r\n"
        L"- 즐겨찾기 추가 또는 해제\r\n"
        L"- 태그 추가, 수정, 삭제\r\n"
        L"- 항목 삭제\r\n"
        L"- 즐겨찾기 항목은 목록 상단에 먼저 표시됩니다.\r\n\r\n"
        L"[설정]\r\n"
        L"- Windows 시작 시 자동 실행\r\n"
        L"- 복사 시 토스트 알림\r\n"
        L"- 복사 후 오버레이 열기\r\n"
        L"- 언어 (시스템 기본값/한국어/English)\r\n"
        L"- 복사/붙여넣기 단축키 변경 및 초기화\r\n"
        L"- '모든 클립 삭제'는 저장된 기록만 지우며 앱 설정은 유지합니다.\r\n"
        L"- 단축키 변경은 설정 창을 닫을 때 적용됩니다.\r\n\r\n"
        L"[트레이]\r\n"
        L"- 트레이 아이콘 더블클릭 또는 '클립보드 열기'로 오버레이를 열 수 있습니다.\r\n"
        L"- 트레이 메뉴에서 설정, 도움말, 앱 종료를 사용할 수 있습니다.\r\n",

        // English help body
        L"ClipEverything keeps a history of copied text, files, and images so you can quickly paste them again.\r\n\r\n"
        L"[Default hotkeys]\r\n"
        L"- Copy: {copy}\r\n"
        L"- Paste: {paste}\r\n\r\n"
        L"[Copying and saving]\r\n"
        L"- Select content and press the copy hotkey to save the current clipboard to history.\r\n"
        L"- Right after copying, the overlay opens so you can name the saved item immediately. (Can be disabled in Settings)\r\n"
        L"- Copying the same content again updates the existing item's last-copied time and source instead of creating a new one.\r\n"
        L"- Items flagged with a sensitive-content signal (e.g. by password managers) are never recorded.\r\n\r\n"
        L"[Using the overlay]\r\n"
        L"- Press the paste hotkey to open the overlay.\r\n"
        L"- The list is scoped to the current app by default; use 'All apps' in the top right to see the full history.\r\n"
        L"- The search box matches names, copied content, app names, and #tags.\r\n"
        L"- Click an item or press Enter to paste; press Esc to close the overlay.\r\n"
        L"- Scroll vertically when there are many items.\r\n"
        L"- Drag the header to move the overlay, and drag the top/bottom edge to resize it.\r\n\r\n"
        L"[Per-item actions]\r\n"
        L"- Rename\r\n"
        L"- Add or remove favorite\r\n"
        L"- Add, edit, and remove tags\r\n"
        L"- Delete item\r\n"
        L"- Favorites are shown at the top of the list.\r\n\r\n"
        L"[Settings]\r\n"
        L"- Run at Windows startup\r\n"
        L"- Show toast on copy\r\n"
        L"- Open overlay after copy\r\n"
        L"- Language (System default/한국어/English)\r\n"
        L"- Change or reset the copy/paste hotkeys\r\n"
        L"- 'Delete all clips' removes saved history only; app settings are kept.\r\n"
        L"- Hotkey changes apply when the settings window is closed.\r\n\r\n"
        L"[Tray]\r\n"
        L"- Double-click the tray icon or choose 'Open clipboard' to open the overlay.\r\n"
        L"- The tray menu provides Settings, Help, and Exit.\r\n",
    },
};

static_assert(static_cast<size_t>(Str::Count) == std::size(kStrings),
              "kStrings must match the Str enum");

bool g_korean = true;

bool IsSystemKorean()
{
    return PRIMARYLANGID(GetUserDefaultUILanguage()) == LANG_KOREAN;
}

} // namespace

AppLanguage ParseLanguageSetting(const std::wstring& value)
{
    if (value == L"ko") return AppLanguage::Korean;
    if (value == L"en") return AppLanguage::English;
    return AppLanguage::System;
}

std::wstring LanguageSettingToString(AppLanguage lang)
{
    switch (lang) {
        case AppLanguage::Korean:  return L"ko";
        case AppLanguage::English: return L"en";
        default:                   return L"system";
    }
}

void SetAppLanguage(AppLanguage lang)
{
    switch (lang) {
        case AppLanguage::Korean:  g_korean = true;  break;
        case AppLanguage::English: g_korean = false; break;
        default:                   g_korean = IsSystemKorean(); break;
    }
}

bool IsKoreanActive()
{
    return g_korean;
}

const wchar_t* Tr(Str id)
{
    const auto idx = static_cast<size_t>(id);
    if (idx >= std::size(kStrings)) return L"";
    return g_korean ? kStrings[idx].ko : kStrings[idx].en;
}

std::wstring ReplaceAll(std::wstring text,
                        const std::wstring& token,
                        const std::wstring& value)
{
    if (token.empty()) return text;
    size_t pos = 0;
    while ((pos = text.find(token, pos)) != std::wstring::npos) {
        text.replace(pos, token.length(), value);
        pos += value.length();
    }
    return text;
}

std::wstring TrFmt(Str id, const std::wstring& arg)
{
    std::wstring text = Tr(id);
    const size_t pos = text.find(L"{}");
    if (pos != std::wstring::npos)
        text.replace(pos, 2, arg);
    return text;
}
