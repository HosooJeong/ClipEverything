use super::{
    Input,
    paint::{self, Bitmap, Contrast, Font, Symbol},
};
use crate::{Command, Event, PreviewChange, Settings};
use anyhow::{Result, ensure};
use clip_everything::{
    clipboard::wide,
    preferences::{Shortcut, TileContent},
    store::Item,
};
use std::{
    borrow::Cow,
    cell::{Cell, RefCell},
    ptr::null_mut,
};
use tao::event_loop::EventLoopProxy;
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::LibraryLoader::GetModuleHandleW,
    UI::{Controls::*, Input::KeyboardAndMouse::EnableWindow, WindowsAndMessaging::*},
};

const HELP_USAGE: i32 = 141;
const HELP_LICENSE: i32 = 142;
const HELP_NOTICES: i32 = 143;
const HELP_TEXT: i32 = 144;
const PROJECT_LICENSE: &str = include_str!("../../../LICENSE");
const THIRD_PARTY_NOTICES: &str = include_str!("../../THIRD-PARTY-NOTICES.txt");

thread_local! {static ACTIVE:Cell<HWND>=const{Cell::new(null_mut())};}
pub fn translate(msg: *const std::ffi::c_void) -> bool {
    ACTIVE.with(|d| {
        let hwnd = d.get();
        !hwnd.is_null()
            && unsafe {
                IsWindowVisible(hwnd) != 0
                    && {
                        let target = (*msg.cast::<MSG>()).hwnd;
                        target == hwnd || IsChild(hwnd, target) != 0
                    }
                    && IsDialogMessageW(hwnd, msg.cast()) != 0
            }
    })
}
pub fn text(hwnd: HWND) -> String {
    unsafe {
        let mut buf = vec![0; GetWindowTextLengthW(hwnd).max(0) as usize + 1];
        let n = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
        String::from_utf16_lossy(&buf[..n.max(0) as usize])
    }
}
pub fn set(hwnd: HWND, value: &str) {
    unsafe {
        SetWindowTextW(hwnd, wide(value).as_ptr());
    }
}
struct Bridge {
    proxy: EventLoopProxy<Event>,
    pic: RefCell<Option<Bitmap>>,
    korean: Cell<bool>,
    help_page: Cell<Option<i32>>,
}
fn dialog_button(id: i32) -> bool {
    matches!(
        id,
        1 | 2 | 120 | 121 | 134 | 135 | HELP_USAGE..=HELP_NOTICES
    )
}
unsafe extern "system" fn procedure(h: HWND, msg: u32, w: WPARAM, l: LPARAM) -> isize {
    unsafe {
        if msg == WM_INITDIALOG {
            SetWindowLongPtrW(h, GWLP_USERDATA, l);
            return 1;
        }
        let ptr = GetWindowLongPtrW(h, GWLP_USERDATA) as *const Bridge;
        if ptr.is_null() {
            return 0;
        }
        let bridge = &*ptr;
        match msg {
            WM_CLOSE | WM_COMMAND => {
                let id = if msg == WM_CLOSE {
                    2
                } else {
                    (w & 0xffff) as i32
                };
                if msg == WM_CLOSE || (w >> 16) == BN_CLICKED as usize && dialog_button(id) {
                    let _ = bridge
                        .proxy
                        .send_event(Event::Ui(Input::Dialog(h as isize, id)));
                    return 1;
                }
            }
            WM_CTLCOLORDLG | WM_CTLCOLORSTATIC => {
                SetTextColor(w as HDC, GetSysColor(COLOR_WINDOWTEXT));
                SetBkColor(w as HDC, GetSysColor(COLOR_WINDOW));
                return GetSysColorBrush(COLOR_WINDOW) as isize;
            }
            WM_NOTIFY if l != 0 => {
                let header = &*(l as *const NMHDR);
                let id = header.idFrom as i32;
                if header.code == NM_CUSTOMDRAW && dialog_button(id) {
                    let draw = &*(l as *const NMCUSTOMDRAW);
                    if draw.dwDrawStage == CDDS_PREPAINT {
                        draw_dialog_button(draw, id, id == 1 || bridge.help_page.get() == Some(id));
                        SetWindowLongPtrW(h, DWLP_MSGRESULT as i32, CDRF_SKIPDEFAULT as isize);
                        return 1;
                    }
                }
            }
            WM_DRAWITEM => {
                let draw = &*(l as *const DRAWITEMSTRUCT);
                if (122..=124).contains(&draw.CtlID) {
                    draw_section_heading(draw);
                    return 1;
                }
                if draw.CtlID == 136 {
                    let dc = draw.hDC;
                    let r = draw.rcItem;
                    let dpi = windows_sys::Win32::UI::HiDpi::GetDpiForWindow(h).max(96) as i32;
                    let px = |n: i32| (n * dpi + 48) / 96;
                    let theme = Contrast::read();
                    FillRect(dc, &r, GetSysColorBrush(COLOR_WINDOW));
                    paint::rounded_rect(
                        dc,
                        r,
                        theme.color(paint::BG, COLOR_WINDOW),
                        theme.color(0xe4dbd2, COLOR_WINDOWTEXT),
                        px(1),
                        px(12),
                    );
                    if let Some(img) = bridge.pic.borrow().as_ref() {
                        let ratio = ((r.right - r.left - 2 * px(6)) as f64 / img.width as f64)
                            .min((r.bottom - r.top - 2 * px(6)) as f64 / img.height as f64)
                            .min(1.);
                        let (w, h) = (
                            (ratio * img.width as f64) as i32,
                            (ratio * img.height as f64) as i32,
                        );
                        img.draw(
                            dc,
                            (r.left + r.right - w) / 2,
                            (r.top + r.bottom - h) / 2,
                            w,
                            h,
                        );
                    } else {
                        let mid = (r.top + r.bottom) / 2;
                        Symbol::Capture.paint(
                            dc,
                            (r.left + r.right - px(24)) / 2,
                            mid - px(23),
                            px(24),
                            theme.color(0xa58266, COLOR_WINDOWTEXT),
                        );
                        let mut line = RECT {
                            left: r.left,
                            top: mid + px(5),
                            right: r.right,
                            bottom: r.bottom,
                        };
                        let font = SendMessageW(h, WM_GETFONT, 0, 0) as HFONT;
                        let old = SelectObject(dc, font);
                        SetBkMode(dc, TRANSPARENT as i32);
                        SetTextColor(dc, theme.color(0x796856, COLOR_WINDOWTEXT));
                        let label = if bridge.korean.get() {
                            "캡처한 영역이 여기에 표시됩니다."
                        } else {
                            "Your capture appears here."
                        };
                        DrawTextW(
                            dc,
                            wide(label).as_ptr(),
                            -1,
                            &mut line,
                            DT_CENTER | DT_SINGLELINE | DT_NOPREFIX,
                        );
                        SelectObject(dc, old);
                    }
                    return 1;
                }
            }
            _ => {}
        }
        0
    }
}
fn draw_section_heading(draw: &DRAWITEMSTRUCT) {
    unsafe {
        let dc = draw.hDC;
        let dpi = windows_sys::Win32::UI::HiDpi::GetDpiForWindow(draw.hwndItem).max(96) as i32;
        let px = |n: i32| (n * dpi + 48) / 96;
        let theme = Contrast::read();
        let font = Font::weighted(px(13), 600);
        let previous = SelectObject(dc, font.0);
        let label = wide(&text(draw.hwndItem));
        let mut r = draw.rcItem;
        FillRect(dc, &r, GetSysColorBrush(COLOR_WINDOW));
        SetBkMode(dc, TRANSPARENT as i32);
        SetTextColor(dc, theme.color(0x79553a, COLOR_WINDOWTEXT));
        DrawTextW(
            dc,
            label.as_ptr(),
            -1,
            &mut r,
            DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
        );
        let mut extent = SIZE::default();
        GetTextExtentPoint32W(dc, label.as_ptr(), (label.len() - 1) as i32, &mut extent);
        r.left += extent.cx + px(8);
        r.top = (r.top + r.bottom) / 2;
        r.bottom = r.top + px(1);
        let brush = CreateSolidBrush(theme.color(0xeee7df, COLOR_WINDOWTEXT));
        FillRect(dc, &r, brush);
        DeleteObject(brush);
        SelectObject(dc, previous);
    }
}
fn draw_dialog_button(draw: &NMCUSTOMDRAW, id: i32, primary: bool) {
    unsafe {
        let h = draw.hdr.hwndFrom;
        let dc = draw.hdc;
        let r = draw.rc;
        let dpi = windows_sys::Win32::UI::HiDpi::GetDpiForWindow(h).max(96) as i32;
        let px = |n: i32| (n * dpi + 48) / 96;
        let contrast = Contrast::read();
        let pressed = draw.uItemState & CDIS_SELECTED != 0;
        let hover = draw.uItemState & CDIS_HOT != 0;
        let bg = if primary {
            if pressed {
                0x93430e
            } else if hover {
                0xb55615
            } else {
                0xc65a17
            }
        } else if hover || pressed {
            0xfff2ea
        } else {
            0xffffff
        };
        let bg = contrast.color(
            bg,
            if primary {
                COLOR_HIGHLIGHT
            } else {
                COLOR_WINDOW
            },
        );
        let ink = contrast.color(
            if primary {
                0xffffff
            } else if id == 120 {
                0x3e388f
            } else {
                0x493729
            },
            if primary {
                COLOR_HIGHLIGHTTEXT
            } else {
                COLOR_WINDOWTEXT
            },
        );
        FillRect(dc, &r, GetSysColorBrush(COLOR_WINDOW));
        paint::rounded_rect(
            dc,
            r,
            bg,
            if primary {
                contrast.color(bg, COLOR_WINDOWTEXT)
            } else {
                contrast.color(0xe3dbd5, COLOR_WINDOWTEXT)
            },
            px(1),
            px(10),
        );
        let mut line = r;
        let label = wide(&text(h));
        let old = SelectObject(dc, SendMessageW(h, WM_GETFONT, 0, 0) as HFONT);
        SetBkMode(dc, TRANSPARENT as i32);
        SetTextColor(dc, ink);
        if id == 134 {
            let mut extent = SIZE::default();
            GetTextExtentPoint32W(dc, label.as_ptr(), (label.len() - 1) as i32, &mut extent);
            let icon = px(15);
            let gap = px(5);
            let x = ((r.right - extent.cx - icon - gap) / 2).max(px(2));
            Symbol::Capture.paint(dc, x, (r.bottom - icon) / 2, icon, ink);
            line.left = x + icon + gap;
            DrawTextW(
                dc,
                label.as_ptr(),
                -1,
                &mut line,
                DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
            );
        } else {
            DrawTextW(
                dc,
                label.as_ptr(),
                -1,
                &mut line,
                DT_CENTER | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
            );
        }
        if draw.uItemState & CDIS_FOCUS != 0 {
            let focus = RECT {
                left: r.left + px(4),
                top: r.top + px(4),
                right: r.right - px(4),
                bottom: r.bottom - px(4),
            };
            DrawFocusRect(dc, &focus);
        }
        SelectObject(dc, old);
    }
}
enum Editor {
    Registration,
    Existing(i64),
}

pub struct Dialogs {
    hwnd: HWND,
    owner: HWND,
    bridge: Box<Bridge>,
    editing: Option<Editor>,
    shot: PreviewChange,
}
impl Dialogs {
    pub fn new(owner: HWND, proxy: EventLoopProxy<Event>) -> Self {
        Self {
            hwnd: null_mut(),
            owner,
            bridge: Box::new(Bridge {
                proxy,
                pic: RefCell::new(None),
                korean: Cell::new(false),
                help_page: Cell::new(None),
            }),
            editing: None,
            shot: PreviewChange::Keep,
        }
    }
    pub fn active(&self) -> bool {
        !self.hwnd.is_null()
    }
    pub fn matches(&self, h: isize) -> bool {
        self.active() && self.hwnd as isize == h
    }
    fn ctrl(&self, id: i32) -> HWND {
        unsafe { GetDlgItem(self.hwnd, id) }
    }
    fn create(&mut self, id: usize) -> Result<()> {
        self.close();
        unsafe {
            self.hwnd = CreateDialogParamW(
                GetModuleHandleW(std::ptr::null()),
                id as *const u16,
                self.owner,
                Some(procedure),
                (&*self.bridge as *const Bridge) as isize,
            );
            ensure!(
                !self.hwnd.is_null(),
                "대화 상자를 열 수 없습니다: {}",
                std::io::Error::last_os_error()
            );
            ACTIVE.with(|v| v.set(self.hwnd));
            EnableWindow(self.owner, 0);
        }
        Ok(())
    }
    pub fn close(&mut self) {
        if self.active() {
            unsafe {
                ACTIVE.with(|v| v.set(null_mut()));
                EnableWindow(self.owner, 1);
                DestroyWindow(self.hwnd);
                self.hwnd = null_mut();
                self.editing = None;
                self.bridge.help_page.set(None);
                self.shot = PreviewChange::Keep;
                *self.bridge.pic.borrow_mut() = None;
            }
        }
    }
    pub fn visibility(&self, show: bool) {
        if self.active() {
            unsafe {
                ShowWindow(self.hwnd, if show { SW_SHOW } else { SW_HIDE });
                if show {
                    SetForegroundWindow(self.hwnd);
                }
            }
        }
    }
    pub fn settings(&mut self, s: &Settings, ko: bool) -> Result<()> {
        self.create(201)?;
        set(
            self.hwnd,
            if ko {
                "ClipEverything 설정"
            } else {
                "ClipEverything settings"
            },
        );
        for (id, k, e) in [
            (122, "단축키", "Shortcuts"),
            (123, "일반", "General"),
            (124, "앱 관리", "App management"),
            (110, "선택 영역 등록", "Capture selection"),
            (112, "버튼판 열기", "Open board"),
            (
                114,
                "등록할 때 이름과 미리보기 편집",
                "Review name and preview before saving",
            ),
            (115, "클립 저장 알림 표시", "Show capture notifications"),
            (116, "로그인 시 실행", "Start at sign-in"),
            (117, "창을 닫으면 트레이에 유지", "Keep in tray on close"),
            (118, "표시 언어", "Language"),
            (125, "버튼에 표시할 내용", "Button content"),
            (120, "전체 기록 삭제", "Clear history"),
            (121, "앱 종료", "Quit app"),
            (1, "저장", "Save"),
            (2, "취소", "Cancel"),
        ] {
            set(self.ctrl(id), if ko { k } else { e });
        }
        let [a, b] = s.keys();
        set(self.ctrl(111), &a.label());
        set(self.ctrl(113), &b.label());
        unsafe {
            for (id, v) in [
                (114, s.open_after),
                (115, s.notify),
                (116, s.startup),
                (117, s.close_to_tray),
            ] {
                CheckDlgButton(self.hwnd, id, v as u32);
            }
            let combo = self.ctrl(119);
            for v in [
                if ko {
                    "시스템 기본값"
                } else {
                    "System default"
                },
                "한국어",
                "English",
            ] {
                SendMessageW(combo, CB_ADDSTRING, 0, wide(v).as_ptr() as isize);
            }
            SendMessageW(
                combo,
                CB_SETCURSEL,
                match s.language.as_str() {
                    "ko" => 1,
                    "en" => 2,
                    _ => 0,
                },
                0,
            );
            let content = self.ctrl(126);
            for (k, en) in [
                ("아이콘", "Icon"),
                ("미리보기", "Preview"),
                ("표시 안 함", "None"),
            ] {
                SendMessageW(
                    content,
                    CB_ADDSTRING,
                    0,
                    wide(if ko { k } else { en }).as_ptr() as isize,
                );
            }
            SendMessageW(
                content,
                CB_SETCURSEL,
                match s.tile_content {
                    TileContent::Icon => 0,
                    TileContent::Preview => 1,
                    TileContent::None => 2,
                },
                0,
            );
        }
        self.visibility(true);
        Ok(())
    }
    pub fn help(&mut self, settings: &Settings, ko: bool) -> Result<()> {
        self.create(203)?;
        set(
            self.hwnd,
            if ko {
                "ClipEverything 도움말"
            } else {
                "ClipEverything help"
            },
        );
        for (id, k, e) in [
            (HELP_USAGE, "사용 방법", "How to use"),
            (HELP_LICENSE, "저작권·라이선스", "Copyright and license"),
            (HELP_NOTICES, "오픈소스 고지", "Open source notices"),
            (2, "닫기", "Close"),
        ] {
            set(self.ctrl(id), if ko { k } else { e });
        }
        let body = self.ctrl(HELP_TEXT);
        unsafe {
            let dpi = windows_sys::Win32::UI::HiDpi::GetDpiForWindow(self.hwnd).max(96) as usize;
            let pad = (8 * dpi + 48) / 96;
            SendMessageW(
                body,
                EM_SETMARGINS,
                (EC_LEFTMARGIN | EC_RIGHTMARGIN) as usize,
                (pad | pad << 16) as isize,
            );
        }
        self.help_page(HELP_USAGE, settings, ko);
        self.visibility(true);
        Ok(())
    }
    fn help_page(&self, id: i32, settings: &Settings, ko: bool) {
        let content: Cow<'_, str> = match id {
            HELP_LICENSE => PROJECT_LICENSE.into(),
            HELP_NOTICES => THIRD_PARTY_NOTICES.into(),
            _ => help_text(settings, ko).into(),
        };
        self.bridge.help_page.set(Some(id));
        let body = self.ctrl(HELP_TEXT);
        set(body, &content.replace("\r\n", "\n").replace('\n', "\r\n"));
        unsafe {
            CheckRadioButton(self.hwnd, HELP_USAGE, HELP_NOTICES, id);
            for button in HELP_USAGE..=HELP_NOTICES {
                InvalidateRect(self.ctrl(button), std::ptr::null(), 0);
            }
            SendMessageW(body, EM_SETSEL, 0, 0);
            SendMessageW(body, WM_VSCROLL, SB_TOP as usize, 0);
        }
    }
    pub fn editor(
        &mut self,
        item: Option<&Item>,
        preview: Option<&[u8]>,
        ko: bool,
        registration: bool,
    ) -> Result<()> {
        self.bridge.korean.set(ko);
        self.create(202)?;
        self.editing = if registration {
            Some(Editor::Registration)
        } else {
            item.map(|item| Editor::Existing(item.id))
        };
        set(
            self.hwnd,
            match (ko, registration) {
                (true, true) => "클립 등록",
                (true, false) => "클립 수정",
                (false, true) => "Register clip",
                (false, false) => "Edit clip",
            },
        );
        for (id, k, e) in [
            (130, "이름", "Name"),
            (137, "미리보기", "Preview"),
            (132, "태그", "Tags"),
            (134, "영역 캡처", "Capture area"),
            (135, "삭제", "Remove"),
            (1, "저장", "Save"),
            (2, "취소", "Cancel"),
        ] {
            set(self.ctrl(id), if ko { k } else { e });
        }
        if let Some(item) = item {
            set(self.ctrl(131), &item.name);
            set(self.ctrl(133), &item.tags);
        }
        self.picture(preview);
        self.visibility(true);
        unsafe {
            windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus(self.ctrl(131));
        }
        Ok(())
    }
    fn picture(&self, bytes: Option<&[u8]>) {
        *self.bridge.pic.borrow_mut() = bytes.and_then(|b| Bitmap::decode(b, 640, 250));
        unsafe {
            InvalidateRect(self.ctrl(136), std::ptr::null(), 1);
            ShowWindow(
                self.ctrl(135),
                if self.bridge.pic.borrow().is_some() {
                    SW_SHOWNA
                } else {
                    SW_HIDE
                },
            );
        }
    }
    pub fn captured(&mut self, bytes: &[u8]) {
        self.shot = PreviewChange::Replace;
        self.picture(Some(bytes));
    }
    pub fn set_saving(&self, saving: bool) {
        unsafe {
            for id in [1, 2, 131, 133, 134, 135] {
                EnableWindow(self.ctrl(id), i32::from(!saving));
            }
        }
    }
    pub fn command(&mut self, id: i32, settings: &Settings, ko: bool) -> Result<Option<Command>> {
        match id {
            2 => return Ok(Some(Command::CancelDialog)),
            HELP_USAGE..=HELP_NOTICES if self.bridge.help_page.get().is_some() => {
                self.help_page(id, settings, ko);
            }
            120 => {
                if confirm(
                    self.hwnd,
                    if ko {
                        "전체 클립 기록을 삭제하시겠습니까?"
                    } else {
                        "Delete all clips?"
                    },
                ) {
                    self.close();
                    return Ok(Some(Command::Clear));
                }
            }
            121 => return Ok(Some(Command::Quit)),
            134 => {
                if self.editing.is_some() {
                    return Ok(Some(Command::Snip));
                }
            }
            135 => {
                self.shot = PreviewChange::Clear;
                self.picture(None);
            }
            1 => {
                if let Some(editor) = &self.editing {
                    let name = text(self.ctrl(131));
                    let tags = text(self.ctrl(133));
                    let shot = self.shot;
                    return Ok(Some(match *editor {
                        Editor::Registration => Command::Register { name, tags, shot },
                        Editor::Existing(id) => Command::Edit {
                            id,
                            name,
                            tags,
                            shot,
                        },
                    }));
                }
                let capture = Shortcut::parse(&text(self.ctrl(111)))?;
                let board = Shortcut::parse(&text(self.ctrl(113)))?;
                let checked = |id| unsafe { IsDlgButtonChecked(self.hwnd, id) != 0 };
                let language = match unsafe { SendMessageW(self.ctrl(119), CB_GETCURSEL, 0, 0) } {
                    1 => "ko",
                    2 => "en",
                    _ => "system",
                };
                let tile_content = match unsafe { SendMessageW(self.ctrl(126), CB_GETCURSEL, 0, 0) }
                {
                    1 => TileContent::Preview,
                    2 => TileContent::None,
                    _ => TileContent::Icon,
                };
                return Ok(Some(Command::Settings {
                    value: Settings {
                        copy_mods: capture.mods,
                        copy_vk: capture.vk,
                        paste_mods: board.mods,
                        paste_vk: board.vk,
                        language: language.into(),
                        open_after: checked(114),
                        notify: checked(115),
                        startup: checked(116),
                        close_to_tray: checked(117),
                        tile_content,
                    },
                }));
            }
            _ => {}
        }
        Ok(None)
    }
    pub fn error(&self, s: &str) {
        unsafe {
            MessageBoxW(
                self.hwnd,
                wide(s).as_ptr(),
                wide("ClipEverything").as_ptr(),
                MB_OK | MB_ICONWARNING,
            );
        }
    }
}

fn help_text(settings: &Settings, ko: bool) -> String {
    let [capture, board] = settings.keys().map(Shortcut::label);
    let version = env!("CARGO_PKG_VERSION");
    if ko {
        format!(
            "ClipEverything {version}\n\n\
클립 등록\n\
1. 저장할 문구, 표, 이미지 또는 파일을 선택하고 {capture}를 누릅니다.\n\
2. 등록창에서 이름을 입력하고 저장합니다. 태그를 붙이거나 영역 캡처로 미리보기를 만들 수 있습니다.\n\n\
다시 붙여넣기\n\
1. 붙여넣을 위치에 커서를 두고 {board}를 누릅니다.\n\
2. 버튼판에서 클립을 클릭하거나 방향키로 선택하고 Enter를 누릅니다.\n\
트레이에서 버튼판을 연 경우에는 클립을 눌러 복사한 뒤 원하는 창에서 Ctrl+V를 누릅니다.\n\n\
버튼판 관리\n\
왼쪽에서 프로그램을 고릅니다. Ctrl+F로 이름이나 태그를 검색하고, 버튼에 마우스를 올려 미리보기를 확인합니다.\n\
버튼을 끌어 위치를 바꿉니다. 우클릭 메뉴에서 이름 수정, 삭제, 자동 정렬을 할 수 있습니다.\n\n\
설정에서 단축키와 트레이 동작을 바꿀 수 있습니다. 버튼 이름 위에는 아이콘이나 미리보기를 표시하거나, 이름만 표시할 수 있습니다."
        )
    } else {
        format!(
            "ClipEverything {version}\n\n\
Save a clip\n\
1. Select the text, table, image, or files and press {capture}.\n\
2. Name your clip and save it. You can add tags or capture a region for the preview.\n\n\
Paste a saved clip\n\
1. Put your cursor where you want to paste and press {board}.\n\
2. Click a clip, or select one with the arrow keys and press Enter.\n\
If you opened the board from the tray, click a clip to copy it and press Ctrl+V in the destination window.\n\n\
Manage the board\n\
Select a program on the left. Use Ctrl+F to search names or tags, and hover over a clip to preview it.\n\
Drag clips to move them. Right-click to rename, delete, or auto arrange them.\n\n\
Open Settings to change shortcuts, tray behavior, or what appears above each button name."
        )
    }
}
pub fn confirm(h: HWND, s: &str) -> bool {
    unsafe {
        MessageBoxW(
            h,
            wide(s).as_ptr(),
            wide("ClipEverything").as_ptr(),
            MB_YESNO | MB_ICONQUESTION | MB_DEFBUTTON2,
        ) == IDYES
    }
}
impl Drop for Dialogs {
    fn drop(&mut self) {
        self.close();
    }
}
