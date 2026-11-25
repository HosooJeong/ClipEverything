use super::{
    Input,
    paint::{self, Bitmap, Contrast, Font, Symbol},
};
use crate::{Command, Event, Settings};
use anyhow::{Result, ensure};
use clip_everything::{clipboard::wide, preferences::Shortcut, store::Item};
use std::{
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
                if msg == WM_CLOSE
                    || (w >> 16) == BN_CLICKED as usize
                        && matches!(id, 1 | 2 | 120 | 121 | 134 | 135)
                {
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
                if header.code == NM_CUSTOMDRAW
                    && matches!(header.idFrom, 1 | 2 | 120 | 121 | 134 | 135)
                {
                    let draw = &*(l as *const NMCUSTOMDRAW);
                    if draw.dwDrawStage == CDDS_PREPAINT {
                        draw_dialog_button(draw, header.idFrom as i32);
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
fn draw_dialog_button(draw: &NMCUSTOMDRAW, id: i32) {
    unsafe {
        let h = draw.hdr.hwndFrom;
        let dc = draw.hdc;
        let r = draw.rc;
        let dpi = windows_sys::Win32::UI::HiDpi::GetDpiForWindow(h).max(96) as i32;
        let px = |n: i32| (n * dpi + 48) / 96;
        let contrast = Contrast::read();
        let primary = id == 1;
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
pub struct Dialogs {
    hwnd: HWND,
    owner: HWND,
    bridge: Box<Bridge>,
    pub editing: Option<i64>,
    shot: &'static str,
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
            }),
            editing: None,
            shot: "keep",
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
                self.shot = "keep";
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
                "클립 등록 직후 이름과 미리보기 편집",
                "Edit name and preview after capture",
            ),
            (115, "클립 저장 알림 표시", "Show capture notifications"),
            (116, "로그인 시 실행", "Start at sign-in"),
            (117, "창을 닫으면 트레이에 유지", "Keep in tray on close"),
            (118, "표시 언어", "Language"),
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
        }
        self.visibility(true);
        Ok(())
    }
    pub fn editor(
        &mut self,
        i: &Item,
        preview: Option<&[u8]>,
        ko: bool,
        registration: bool,
    ) -> Result<()> {
        self.bridge.korean.set(ko);
        self.create(202)?;
        self.editing = Some(i.id);
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
        set(self.ctrl(131), &i.name);
        set(self.ctrl(133), &i.tags);
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
        self.shot = "replace";
        self.picture(Some(bytes));
    }
    pub fn command(&mut self, id: i32, ko: bool) -> Result<Option<Command>> {
        match id {
            2 => self.close(),
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
                if let Some(id) = self.editing {
                    return Ok(Some(Command::Snip { id }));
                }
            }
            135 => {
                self.shot = "clear";
                self.picture(None);
            }
            1 => {
                if let Some(id) = self.editing {
                    return Ok(Some(Command::Edit {
                        id,
                        name: text(self.ctrl(131)),
                        tags: text(self.ctrl(133)),
                        shot: self.shot.to_owned(),
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
