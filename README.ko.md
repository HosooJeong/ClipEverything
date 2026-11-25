# ClipEverything

[English](README.md) · 한국어

ClipEverything은 자주 붙여넣는 문구, 서식, 표, 이미지, 파일 선택을 저장해 다시 사용하는 Windows x64 네이티브 앱입니다. 프로그램별 버튼판에 항목을 배치하고, 이름이나 영역 스크린샷으로 구분할 수 있습니다. 원본 클립보드 형식은 로컬 SQLite 보관함에 저장하며, 붙여넣기 결과는 대상 프로그램이 지원하는 형식에 따라 달라집니다.

## 설치 및 실행

[Windows용 최신 버전 다운로드](https://github.com/HosooJeong/ClipEverything/releases/latest)

| 파일 | 사용 방법 |
| --- | --- |
| `ClipEverything.msi` | 현재 사용자용으로 설치합니다. 설치 폴더, 바로가기, 자동 시작, 트레이 동작, 기본 단축키를 선택할 수 있습니다. |
| `ClipEverything.exe` | Windows 사용자 프로필의 보관함을 사용해 바로 실행합니다. |
| `ClipEverything-portable.zip` | 쓰기 가능한 폴더에 전체 압축을 풀고 실행합니다. 동봉된 `portable.flag`를 통해 실행 파일 옆에 기록을 저장합니다. |

## 사용 방법

- 원본 프로그램에서 내용을 선택하고 **Ctrl+Alt+C**를 눌러 등록합니다. 이름과 태그를 입력하거나 영역을 캡처해 미리보기로 저장할 수 있습니다.
- 붙여넣을 프로그램에서 **Ctrl+Alt+V**를 눌러 버튼판을 엽니다. 항목을 클릭하거나 방향키로 선택한 뒤 **Enter**를 누릅니다.
- 트레이에서 버튼판을 열면 수동 복사로 동작합니다. 항목에 체크가 표시되면 원하는 창에서 **Ctrl+V**로 붙여넣습니다.
- 버튼을 드래그해 배치하고 가장자리의 **+**로 행과 열을 추가합니다. **Ctrl+F**로 선택한 프로그램 안에서 제목과 태그를 검색합니다.

기본 보관함은 이전 버전과 호환되는 `%APPDATA%\ClipEverythingRust`를 사용합니다. 포터블 보관함은 실행 파일 옆의 `portable-data-rust`에 저장됩니다.

## 빌드

Windows x64에서 Rust MSVC 툴체인, Visual Studio C++ Build Tools, Windows SDK를 준비합니다.

```powershell
cd rust
cargo test --workspace --locked --release
.\package.ps1
```

MSI를 만들려면 .NET SDK를 설치한 뒤 같은 폴더에서 다음 명령을 실행합니다.

```powershell
dotnet tool install --tool-path target/pack-tools wix --version 4.0.6
.\package.ps1 -Installer
```

배포 파일은 `rust/dist/`에 생성됩니다. 설치 옵션, 이전 보관함 가져오기, 소스 구성은 [프로젝트 안내](rust/README.md)에서 확인할 수 있습니다.
