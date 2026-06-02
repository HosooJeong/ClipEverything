# ClipEverything

[English](README.md) | 한국어

ClipEverything은 Windows용 클립보드 기록 관리 프로그램입니다.

평소에는 트레이에 있다가, 지정한 단축키로 복사한 내용을 저장하고 다시 빠르게 찾아 붙여넣는 것을 목표로 만들고 있습니다. 텍스트만 다루는 간단한 클립보드 앱이 아니라, Windows 클립보드 포맷을 최대한 그대로 저장하고 복원하는 쪽에 초점을 두었습니다.

## 컨셉

ClipEverything의 핵심 컨셉은 “지금 사용 중인 프로그램에 따라 달라지는 클립보드 기록”입니다.

일반적인 클립보드 매니저는 모든 기록을 하나의 긴 목록으로 보여줍니다. 간단한 텍스트 조각만 저장할 때는 괜찮지만, 브라우저 링크, Excel 범위, 이미지, 문서 조각, 파일 경로, 서식 있는 텍스트가 섞이기 시작하면 목록이 금방 지저분해집니다. ClipEverything은 클립이 어느 프로그램에서 복사되었는지, 어떤 종류의 내용인지, 그리고 지금 어느 프로그램에 붙여넣으려는지를 함께 다루는 방향으로 만들고 있습니다.

예를 들어 Excel에서 복사한 항목은 스프레드시트 관련 포맷을 유지하고, 이미지 클립은 썸네일로 보여주며, HTML이나 서식 있는 텍스트는 단순 문자열만이 아니라 원래 클립보드 포맷까지 저장하려고 합니다. 붙여넣기 오버레이를 열면 현재 대상 프로그램의 컨텍스트를 우선해서 목록을 좁히고, 필요할 때 전체 기록으로 전환할 수 있습니다.

장기적으로는 글쓰기, 웹 브라우징, 스프레드시트 작업, 이미지 작업, 파일 이동처럼 작업 종류가 달라질 때 클립보드 경험도 같이 달라지는 적응형 클립보드 매니저를 목표로 합니다.

아직은 완성품이라기보다, 실제 사용 가능한 프로토타입에 가깝습니다. 구조나 동작 방식에 대한 피드백을 받기 위해 공개하는 저장소입니다.

## 현재 지원 범위

- Windows 네이티브 앱
- 글로벌 복사/붙여넣기 단축키
- 클립보드 기록 저장 및 중복 감지
- 텍스트, 파일, 이미지 등 여러 클립보드 포맷 저장
- 소스 프로그램 추적 및 현재 프로그램 컨텍스트 기반 기록 필터링
- 텍스트, 리치 텍스트, HTML, Excel 계열, HWP 계열, 이미지 콘텐츠 타입 감지
- SQLite 기반 로컬 저장소
- 오버레이 UI에서 검색, 선택, 붙여넣기
- 항목 이름 변경, 즐겨찾기, 태그 편집
- 시스템 트레이 메뉴
- 자동 시작 설정
- 포터블 패키지와 간단한 설치 파일 생성 스크립트

## 지원하지 않는 것

- macOS / Linux
- 클라우드 동기화
- 암호화 저장
- 정식 테스트 스위트
- CI 빌드

클립보드 기록에는 민감한 내용이 들어갈 수 있습니다. 현재 저장소는 로컬 SQLite DB를 사용하지만, 별도의 암호화는 아직 없습니다.

## 빌드 환경

현재 빌드는 Windows와 MSVC 기준입니다.

필요한 도구:

- Windows 10 또는 Windows 11
- Visual Studio 2022 Build Tools
- MSVC C++ 빌드 도구
- Windows SDK
- PowerShell

빌드 스크립트는 기본적으로 아래 경로의 `vcvars64.bat`를 사용합니다.

```powershell
C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat
```

Visual Studio가 다른 경로에 설치되어 있다면 [build.ps1](build.ps1)의 `$vcvars` 값을 수정해야 합니다.

## 빌드

```powershell
.\build.ps1
```

또는 배치 파일로 실행할 수도 있습니다.

```bat
build.bat
```

빌드 결과는 아래에 생성됩니다.

```text
build\ClipEverything.exe
```

오브젝트 파일은 `build\obj` 아래에 생성되며, 빌드 산출물은 git에 포함하지 않습니다.

## 실행

빌드 후 다음 파일을 실행합니다.

```text
build\ClipEverything.exe
```

기본 데이터 위치:

```text
%APPDATA%\ClipEverything
```

여기에 설정 파일과 클립보드 DB가 저장됩니다.

## 패키징

포터블 ZIP:

```powershell
.\package-portable.ps1
```

설치 파일:

```powershell
.\package-installer.ps1
```

패키징 결과는 `dist` 폴더 아래에 생성됩니다.

주의: 포터블 패키지 스크립트는 현재 사용자의 `settings.json`, `clips.db`가 있으면 같이 복사합니다. 공개 배포용 ZIP을 만들 때는 `portable-data` 안에 개인 클립보드 기록이 들어가지 않았는지 확인해야 합니다.

## 구조

```text
src
├─ core       Windows 핫키, 클립보드 읽기/쓰기, 소스 앱 감지
├─ data       SQLite 저장소와 모델
├─ services   설정, 트레이, 시작 프로그램, 클립보드 흐름
└─ ui         Win32/Direct2D 기반 UI
```

조금 더 자세한 설명은 [docs/program-structure.md](docs/program-structure.md)에 정리해 두었습니다.

## 피드백 받고 싶은 부분

- Windows 클립보드 포맷 저장/복원 방식이 괜찮은지
- `core`, `data`, `services`, `ui` 분리가 유지보수하기 좋은지
- SQLite 스키마와 중복 감지 방식이 적절한지
- Win32 UI 코드가 너무 커지고 있는지
- 클립보드 기록 앱으로서 보안/프라이버시 측면에서 빠진 부분이 있는지

## 라이선스

MIT License입니다. 자세한 내용은 [LICENSE](LICENSE)를 참고하세요.
