use super::paint::{Contrast, Font, rounded_panel, rounded_rect};
use crate::Event;
use anyhow::{Context, Result, ensure};
use clip_everything::clipboard::wide;
use std::{
    cell::{Cell, RefCell},
    mem::size_of,
    ptr::{null, null_mut},
};
use tao::event_loop::EventLoopProxy;
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::LibraryLoader::GetModuleHandleW,
    UI::{
        Controls::*, HiDpi::GetDpiForWindow, Input::KeyboardAndMouse::*, Shell::*,
        WindowsAndMessaging::*,
    },
};

const BUTTON_COUNT: usize = 3;
const WIDTH: i32 = 174;
const PAD: i32 = 7;
const ROW: i32 = 31;
const DIVIDER: i32 = 9;
const FRAME_INSET: i32 = 5;

pub(crate) enum MenuAnchor {
    Tray { rect: RECT, cursor: POINT },
    Point(POINT),
}

struct State {
    frame: HWND,
    proxy: EventLoopProxy<Event>,
    dpi: Cell<i32>,
    font: RefCell<Font>,
    contrast: Cell<Contrast>,
    actions: RefCell<Vec<Event>>,
    divider_y: Cell<Option<i32>>,
    hover: Cell<HWND>,
}
impl State {
    fn px(&self, n: i32) -> i32 {
        (n * self.dpi.get() + 48) / 96
    }
    fn color(&self, normal: u32, system: SYS_COLOR_INDEX) -> u32 {
        self.contrast.get().color(normal, system)
    }
    fn dismiss(&self, menu: HWND) {
        unsafe {
            ShowWindow(menu, SW_HIDE);
            ShowWindow(self.frame, SW_HIDE);
        }
    }
}

pub(crate) struct PopupMenu {
    hwnd: HWND,
    state: Box<State>,
}
impl PopupMenu {
    pub(crate) fn new(owner: HWND, proxy: EventLoopProxy<Event>) -> Result<Self> {
        unsafe {
            let instance = GetModuleHandleW(null());
            // Only the frame needs per-pixel transparency. Native buttons and
            // focus stay in the opaque menu window inside it.
            let frame = CreateWindowExW(
                WS_EX_LAYERED
                    | WS_EX_TRANSPARENT
                    | WS_EX_NOACTIVATE
                    | WS_EX_TOOLWINDOW
                    | WS_EX_TOPMOST,
                wide("STATIC").as_ptr(),
                null(),
                WS_POPUP,
                0,
                0,
                1,
                1,
                owner,
                null_mut(),
                instance,
                null(),
            );
            ensure!(!frame.is_null(), "팝업 메뉴 테두리를 만들 수 없습니다.");
            let hwnd = CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
                wide("STATIC").as_ptr(),
                wide("ClipEverything").as_ptr(),
                WS_POPUP | WS_CLIPCHILDREN,
                0,
                0,
                1,
                1,
                frame,
                null_mut(),
                instance,
                null(),
            );
            if hwnd.is_null() {
                DestroyWindow(frame);
            }
            ensure!(!hwnd.is_null(), "팝업 메뉴를 만들 수 없습니다.");
            let menu = Self {
                hwnd,
                state: Box::new(State {
                    frame,
                    proxy,
                    dpi: Cell::new(96),
                    font: RefCell::new(Font::new(13)),
                    contrast: Cell::new(Contrast::read()),
                    actions: RefCell::default(),
                    divider_y: Cell::new(None),
                    hover: Cell::new(null_mut()),
                }),
            };
            ensure!(
                SetWindowSubclass(
                    hwnd,
                    Some(popup_proc),
                    1,
                    &*menu.state as *const State as usize
                ) != 0,
                "팝업 메뉴를 연결할 수 없습니다."
            );
            for i in 0..BUTTON_COUNT {
                let button = CreateWindowExW(
                    0,
                    wide("BUTTON").as_ptr(),
                    null(),
                    WS_CHILD | WS_TABSTOP | BS_OWNERDRAW as u32,
                    0,
                    0,
                    1,
                    1,
                    hwnd,
                    (i + 1) as HMENU,
                    instance,
                    null(),
                );
                ensure!(
                    !button.is_null()
                        && SetWindowSubclass(
                            button,
                            Some(button_proc),
                            1,
                            &*menu.state as *const State as usize
                        ) != 0,
                    "팝업 메뉴 항목을 만들 수 없습니다."
                );
            }
            Ok(menu)
        }
    }

    pub(crate) fn show(
        &self,
        title: &str,
        anchor: MenuAnchor,
        entries: Vec<(&str, Event)>,
        divider_before: Option<usize>,
    ) -> Result<()> {
        let count = entries.len();
        ensure!(
            (1..=BUTTON_COUNT).contains(&count),
            "메뉴 항목 수가 올바르지 않습니다."
        );
        let (anchor, cursor, prefer_above) = match anchor {
            MenuAnchor::Tray { rect, cursor } => (rect, cursor, true),
            MenuAnchor::Point(p) => (
                RECT {
                    left: p.x,
                    top: p.y,
                    right: p.x,
                    bottom: p.y,
                },
                p,
                false,
            ),
        };
        unsafe {
            let h = self.hwnd;
            let mut monitor = MONITORINFO {
                cbSize: size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            if GetMonitorInfoW(
                MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST),
                &mut monitor,
            ) == 0
            {
                return Ok(());
            }
            let state = &self.state;
            let dpi = GetDpiForWindow(WindowFromPoint(cursor)).max(96) as i32;
            if state.dpi.replace(dpi) != dpi {
                *state.font.borrow_mut() = Font::new(state.px(13));
            }
            state.contrast.set(Contrast::read());
            state.hover.set(null_mut());
            SetWindowTextW(h, wide(title).as_ptr());
            let px = |n| state.px(n);
            let inset = px(FRAME_INSET);
            let divider = divider_before
                .filter(|i| (1..count).contains(i))
                .map(|i| i as i32);
            state
                .divider_y
                .set(divider.map(|i| px(PAD + i * ROW + DIVIDER / 2) - inset));
            let (width, height) = (
                px(WIDTH),
                px(2 * PAD + count as i32 * ROW + if divider.is_some() { DIVIDER } else { 0 }),
            );
            let work = monitor.rcWork;
            let x = anchor
                .left
                .clamp(work.left, (work.right - width).max(work.left));
            let above = anchor.top - px(6) - height;
            let below = anchor.bottom + px(6);
            let y = if prefer_above && above >= work.top
                || !prefer_above && below + height > work.bottom
            {
                above
            } else {
                below
            }
            .clamp(work.top, (work.bottom - height).max(work.top));
            let panel = rounded_panel(
                width,
                height,
                px(11),
                px(1),
                state.color(0xffffff, COLOR_MENU),
                state.color(0xe9e1d9, COLOR_MENUTEXT),
            )
            .context("팝업 메뉴 테두리를 그릴 수 없습니다.")?;
            ensure!(
                panel.update_layer(state.frame, POINT { x, y }),
                "팝업 메뉴 테두리를 표시할 수 없습니다."
            );
            let mut actions = Vec::with_capacity(count);
            for (i, (label, event)) in entries.into_iter().enumerate() {
                let button = GetDlgItem(h, i as i32 + 1);
                let row = PAD
                    + ROW * i as i32
                    + if divider.is_some_and(|n| i as i32 >= n) {
                        DIVIDER
                    } else {
                        0
                    };
                SetWindowTextW(button, wide(label).as_ptr());
                SetWindowPos(
                    button,
                    null_mut(),
                    px(PAD) - inset,
                    px(row) - inset,
                    width - 2 * px(PAD),
                    px(row + ROW) - px(row),
                    SWP_NOACTIVATE | SWP_NOZORDER | SWP_SHOWWINDOW,
                );
                actions.push(event);
            }
            for i in count..BUTTON_COUNT {
                ShowWindow(GetDlgItem(h, i as i32 + 1), SW_HIDE);
            }
            *state.actions.borrow_mut() = actions;
            ShowWindow(state.frame, SW_SHOWNOACTIVATE);
            SetWindowPos(
                h,
                HWND_TOPMOST,
                x + inset,
                y + inset,
                width - 2 * inset,
                height - 2 * inset,
                SWP_SHOWWINDOW | SWP_NOACTIVATE,
            );
            SetForegroundWindow(h);
            SetFocus(h);
            RedrawWindow(h, null(), null_mut(), RDW_INVALIDATE | RDW_ALLCHILDREN);
        }
        Ok(())
    }

    pub(crate) fn visible(&self) -> bool {
        unsafe { IsWindowVisible(self.hwnd) != 0 }
    }

    pub(crate) fn hide(&self) {
        self.state.dismiss(self.hwnd);
    }
}

impl Drop for PopupMenu {
    fn drop(&mut self) {
        unsafe {
            DestroyWindow(self.hwnd);
            DestroyWindow(self.state.frame);
        }
    }
}

unsafe fn key(menu: HWND, code: u16, state: &State) -> bool {
    unsafe {
        let count = state.actions.borrow().len() as i32;
        if count == 0 {
            return false;
        }
        let focused = GetFocus();
        let current = if GetParent(focused) == menu {
            GetDlgCtrlID(focused)
        } else {
            0
        };
        let next = match code {
            VK_ESCAPE => {
                state.dismiss(menu);
                return true;
            }
            VK_RETURN => {
                SendMessageW(GetDlgItem(menu, current.max(1)), BM_CLICK, 0, 0);
                return true;
            }
            VK_HOME => 1,
            VK_END => count,
            VK_UP | VK_DOWN | VK_TAB => {
                let back = code == VK_UP || code == VK_TAB && GetKeyState(VK_SHIFT as i32) < 0;
                if back {
                    if current <= 1 { count } else { current - 1 }
                } else {
                    current % count + 1
                }
            }
            _ => return false,
        };
        SetFocus(GetDlgItem(menu, next));
        true
    }
}

unsafe extern "system" fn button_proc(
    h: HWND,
    msg: u32,
    w: WPARAM,
    l: LPARAM,
    _: usize,
    data: usize,
) -> LRESULT {
    unsafe {
        let state = &*(data as *const State);
        match msg {
            WM_KEYDOWN if key(GetParent(h), w as u16, state) => return 0,
            WM_CHAR if matches!(w, 9 | 13 | 27) => return 0,
            WM_MOUSEMOVE if state.hover.get() != h => {
                let old = state.hover.replace(h);
                let mut info = TRACKMOUSEEVENT {
                    cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: h,
                    dwHoverTime: 0,
                };
                TrackMouseEvent(&mut info);
                if !old.is_null() {
                    InvalidateRect(old, null(), 0);
                }
                InvalidateRect(h, null(), 0);
            }
            WM_MOUSELEAVE if state.hover.get() == h => {
                state.hover.set(null_mut());
                InvalidateRect(h, null(), 0);
            }
            WM_SETFOCUS | WM_KILLFOCUS => {
                InvalidateRect(h, null(), 0);
            }
            _ => {}
        }
        DefSubclassProc(h, msg, w, l)
    }
}

unsafe extern "system" fn popup_proc(
    h: HWND,
    msg: u32,
    w: WPARAM,
    l: LPARAM,
    _: usize,
    data: usize,
) -> LRESULT {
    unsafe {
        let state = &*(data as *const State);
        let px = |n| state.px(n);
        let white = state.color(0xffffff, COLOR_MENU);
        match msg {
            WM_ACTIVATE if w as u16 as u32 == WA_INACTIVE => {
                state.dismiss(h);
            }
            WM_CANCELMODE | WM_CLOSE => {
                state.dismiss(h);
                return 0;
            }
            WM_KEYDOWN if key(h, w as u16, state) => return 0,
            WM_COMMAND if w >> 16 == BN_CLICKED as usize && l != 0 && IsWindowVisible(h) != 0 => {
                let id = i32::from(w as u16);
                if id == 0 || GetDlgItem(h, id) != l as HWND {
                    return DefSubclassProc(h, msg, w, l);
                }
                let index = id as usize - 1;
                let event = {
                    let mut actions = state.actions.borrow_mut();
                    (index < actions.len())
                        .then(|| std::mem::take(&mut *actions).swap_remove(index))
                };
                if let Some(event) = event {
                    state.dismiss(h);
                    let _ = state.proxy.send_event(event);
                }
                return 0;
            }
            WM_ERASEBKGND => return 1,
            WM_PAINT => {
                let mut paint = PAINTSTRUCT::default();
                let dc = BeginPaint(h, &mut paint);
                let mut r = RECT::default();
                GetClientRect(h, &mut r);
                let brush = CreateSolidBrush(white);
                FillRect(dc, &r, brush);
                DeleteObject(brush);
                if let Some(y) = state.divider_y.get() {
                    let margin = px(12) - px(FRAME_INSET);
                    let divider = RECT {
                        left: margin,
                        right: r.right - margin,
                        top: y,
                        bottom: y + px(1),
                    };
                    let brush = CreateSolidBrush(state.color(0xefe4d8, COLOR_MENUTEXT));
                    FillRect(dc, &divider, brush);
                    DeleteObject(brush);
                }
                EndPaint(h, &paint);
                return 0;
            }
            WM_DRAWITEM if l != 0 => {
                let item = &*(l as *const DRAWITEMSTRUCT);
                let hot = item.itemState & (ODS_SELECTED | ODS_FOCUS) != 0
                    || state.hover.get() == item.hwndItem;
                let brush = CreateSolidBrush(white);
                FillRect(item.hDC, &item.rcItem, brush);
                DeleteObject(brush);
                if hot {
                    let fill = state.color(0xfff2ea, COLOR_HIGHLIGHT);
                    rounded_rect(item.hDC, item.rcItem, fill, fill, px(1), px(10));
                }
                let mut label = [0u16; 64];
                let len = GetWindowTextW(item.hwndItem, label.as_mut_ptr(), label.len() as i32);
                let font = SelectObject(item.hDC, state.font.borrow().0);
                SetBkMode(item.hDC, TRANSPARENT as i32);
                SetTextColor(
                    item.hDC,
                    state.color(
                        0x302620,
                        if hot {
                            COLOR_HIGHLIGHTTEXT
                        } else {
                            COLOR_MENUTEXT
                        },
                    ),
                );
                let mut text = item.rcItem;
                text.left += px(12);
                DrawTextW(
                    item.hDC,
                    label.as_ptr(),
                    len,
                    &mut text,
                    DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
                );
                SelectObject(item.hDC, font);
                return 1;
            }
            _ => {}
        }
        DefSubclassProc(h, msg, w, l)
    }
}
