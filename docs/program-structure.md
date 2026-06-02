# ClipEverything 프로그램 구조 정리

기준 시점: 2026-04-07 현재 워크스페이스 기준

## 개요

이 프로젝트는 Win32 API와 Direct2D를 사용하는 네이티브 Windows 클립보드 관리 프로그램이다.
웹 프론트엔드나 서버가 분리된 구조가 아니라, 하나의 데스크톱 프로세스 안에서 핫키 등록, 클립보드 캡처/복원, SQLite 저장, 오버레이 UI, 트레이 UI를 함께 처리한다.

핵심 진입점은 `src/main.cpp`이며, 여기서 대부분의 서비스와 윈도우를 초기화한다.

## 최상위 구조

```text
ClipEverything-bycodex
├─ src
│  ├─ main.cpp
│  ├─ core
│  ├─ data
│  ├─ services
│  └─ ui
│     └─ render
├─ resources
├─ third_party
├─ build.ps1
├─ build.bat
└─ package-*.ps1
```

## 계층별 역할

### 1. 진입점

- `src/main.cpp`
- 싱글 인스턴스 mutex 생성
- COM STA 초기화
- Direct2D/WIC/DirectWrite 초기화
- `Repository`, `ClipboardService`, `HotkeyManager`, `TrayService`, `OverlayWindow` 생성 및 연결
- 메시지 전용 host window 생성 후 Win32 메시지 루프 실행

## 2. core

운영체제와 직접 맞닿는 저수준 계층이다.

- `src/core/hotkey_manager.*`
  글로벌 핫키 등록과 `WM_HOTKEY` 처리 담당
- `src/core/source_detector.*`
  현재 포그라운드 창, 프로세스명, exe 경로 추출 담당
- `src/core/clipboard_reader.*`
  현재 클립보드 포맷을 읽어 `ClipboardSnapshot` 생성
  SHA-256 해시 계산, 콘텐츠 타입 판별, 이미지 썸네일 생성 포함
- `src/core/clipboard_writer.*`
  저장된 포맷들을 다시 클립보드에 복원

## 3. data

SQLite 기반 저장소 계층이다.

- `src/data/models.h`
  `ClipboardItem`, `ClipboardFormat` 등 UI와 저장소가 공유하는 모델 정의
- `src/data/repository.*`
  `%APPDATA%\ClipEverything\clips.db` 파일을 열고 스키마를 생성
  클립 저장, 중복 해시 확인, 목록 조회, 포맷 조회, 이름 변경, 즐겨찾기, 태그, 삭제를 담당

DB 구조는 크게 두 테이블로 나뉜다.

- `ClipboardItems`
  항목 메타데이터 저장
- `ClipboardFormats`
  항목별 실제 클립보드 포맷 바이너리 저장

## 4. services

애플리케이션 동작을 오케스트레이션하는 중간 계층이다.

- `src/services/clipboard_service.*`
  복사 핫키와 붙여넣기 핫키 흐름을 묶는 핵심 서비스
- `src/services/app_settings.*`
  `%APPDATA%\ClipEverything\settings.json` 기반 설정 로드/저장
- `src/services/startup_service.*`
  `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` 레지스트리에 자동 시작 등록/해제
- `src/services/tray_service.*`
  시스템 트레이 아이콘, 우클릭 메뉴, 풍선 알림 처리
- `src/services/toast_service.*`
  토스트 팝업 호출용 얇은 래퍼

## 5. ui

사용자에게 보이는 Win32 UI 계층이다.

- `src/ui/overlay_window.*`
  메인 오버레이 UI
  검색창, 카드 목록, 컨텍스트 메뉴, 선택 후 붙여넣기 실행 담당
- `src/ui/settings_window.*`
  시작 프로그램, 토스트 알림, 핫키 변경, 전체 삭제 설정창
- `src/ui/rename_dialog.*`
  항목 이름 변경 다이얼로그
- `src/ui/tag_dialog.*`
  태그 편집 다이얼로그
- `src/ui/toast_popup.*`
  우하단 토스트 표시용 레이어드 윈도우
- `src/ui/render/d2d_context.*`
  Direct2D/DirectWrite/WIC 렌더링 컨텍스트 관리
- `src/ui/render/image_cache.*`
  앱 아이콘과 썸네일 캐시

## 6. resources

- `resources/ClipEverything.rc`
- `resources/resource.h`
- `resources/app.ico`

아이콘, 메뉴/명령 ID, 리소스 컴파일 대상이 이 계층에 있다.

## 7. third_party

- `third_party/sqlite3.c`
- `third_party/sqlite3.h`
- `third_party/json.hpp`

외부 의존성은 비교적 단순하며, SQLite와 nlohmann/json을 직접 포함해 빌드한다.

## 주요 실행 흐름

### 1. 복사 핫키 흐름

1. `HotkeyManager`가 글로벌 복사 핫키를 감지한다.
2. `ClipboardService::OnCopyHotkey()`가 현재 활성 창 정보를 캡처한다.
3. 내부에서 `Ctrl+C`를 시뮬레이션한다.
4. `TakeSnapshot()`으로 현재 클립보드 포맷들을 읽는다.
5. `Repository::SaveOrUpdate()`로 DB에 저장하거나, 같은 해시가 있으면 `LastCopiedAt`만 갱신한다.
6. 저장 완료 후 오버레이 갱신 또는 토스트 표시 콜백을 호출한다.

### 2. 붙여넣기 핫키 흐름

1. `HotkeyManager`가 글로벌 붙여넣기 핫키를 감지한다.
2. `ClipboardService::OnPasteHotkey()`가 붙여넣을 대상 창 정보를 저장한다.
3. `OverlayWindow::ShowAndRefresh()`가 오버레이를 띄우고 항목 목록을 불러온다.
4. 사용자가 항목을 선택하면 `OverlayWindow::ExecutePaste()`가 실행된다.
5. `ClipboardService::PasteSelectedItem()`이 DB에서 포맷을 가져와 클립보드에 복원한다.
6. 이전 활성 창으로 포커스를 돌린 뒤 `Ctrl+V`를 시뮬레이션한다.

### 3. 트레이 및 설정 흐름

- 트레이 아이콘 더블클릭 시 오버레이를 연다.
- 트레이 메뉴에서 설정창을 열 수 있다.
- 설정창에서는 자동 시작, 토스트 표시 여부, 복사/붙여넣기 핫키 변경, 전체 기록 삭제를 처리한다.

## 저장 및 설정 위치

- 클립 DB: `%APPDATA%\ClipEverything\clips.db`
- 설정 파일: `%APPDATA%\ClipEverything\settings.json`
- 자동 시작 등록: `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\ClipEverything`

즉, 실행 파일 근처에 데이터를 두는 구조가 아니라 사용자 프로필 영역을 사용하는 구조다.

## 빌드 방식

- 메인 빌드 스크립트는 `build.ps1`
- Visual Studio Build Tools의 `vcvars64.bat`를 먼저 로드한 뒤 `cl.exe`를 직접 호출
- `src`의 `.cpp` 파일들과 `third_party/sqlite3.c`를 함께 컴파일
- 리소스는 `rc.exe`로 별도 컴파일 후 링크
- 결과물은 기본적으로 `build/ClipEverything.exe`, `build/ClipEverything.res`

즉, CMake 기반 프로젝트가 아니라 단일 PowerShell 빌드 스크립트 중심 구조다.

## 현재 구조상 관찰사항

- 소스 계층 분리는 `core -> data/services -> ui`로 비교적 명확하다.
- 루트 디렉터리에 `.obj` 파일이 많이 남아 있어 소스와 빌드 산출물이 다소 섞여 있다.
- 토스트 팝업 구현과 `RegisterToastClass()` 함수는 존재하지만, 현재 코드 검색 기준으로 명시적 호출부는 보이지 않았다.
- 전반적으로 "핫키 기반 캡처/복원 엔진 + SQLite 저장소 + Win32 오버레이 UI" 조합으로 이해하면 구조 파악이 쉽다.
