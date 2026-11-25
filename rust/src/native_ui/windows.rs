use super::{
    Input,
    grid::{GRID_TOP, Shape},
    paint::{self, Bitmap, Face, PreviewKey},
};
use crate::Event;
use clip_everything::{board::Cell as Slot, clipboard::wide};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    mem::size_of,
    ptr::{null, null_mut},
    rc::Rc,
};
use tao::event_loop::EventLoopProxy;
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::LibraryLoader::GetModuleHandleW,
    UI::{Controls::*, Input::KeyboardAndMouse::*, Shell::*, WindowsAndMessaging::*},
};

pub const SEARCH: i32 = 30;
pub const SETTINGS: i32 = 31;
pub const SMALLER: i32 = 32;
pub const TOGGLE_SIDEBAR: i32 = 33;
pub const LARGER: i32 = 34;
pub const EMPTY_MESSAGE: i32 = 40;
pub const HELP: i32 = 41;
pub const CATEGORY_SCROLL: i32 = 39;
pub const PREVIOUS: i32 = 37;
pub const NEXT: i32 = 38;
pub const TAB: i32 = 100;
pub const TILE: i32 = 10000;

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Record {
    pub item: Option<i64>,
    pub cell: Option<Slot>,
}
#[derive(Clone)]
pub struct Binding {
    pub record: Record,
    pub app: Option<String>,
    pub rev: i64,
    pub index: usize,
}
struct Gesture {
    source: Binding,
    x: i32,
    y: i32,
    threshold: i32,
    moved: bool,
}
const COPY_TIMER: usize = 0xCE11;
pub struct Bridge {
    pub korean: Cell<bool>,
    copied: Cell<Option<(i64, std::time::Instant)>>,
    contrast: Cell<paint::Contrast>,
    pub proxy: EventLoopProxy<Event>,
    pub faces: RefCell<HashMap<usize, Face>>,
    pub bindings: RefCell<HashMap<usize, Binding>>,
    pub icons: RefCell<HashMap<i64, Rc<Bitmap>>>,
    pub previews: RefCell<HashMap<PreviewKey, Option<Bitmap>>>,
    pub font: Cell<HFONT>,
    pub dpi: Cell<i32>,
    pub rail_width: Cell<i32>,
    pub grid_shape: Cell<Option<Shape>>,
    pub anchor_right: Cell<bool>,
    pub search_bounds: Cell<RECT>,
    pub hover: Cell<usize>,
    drop_target: Cell<usize>,
    gesture: RefCell<Option<Gesture>>,
    ime: Cell<bool>,
    pub tip: Cell<HWND>,
}
impl Bridge {
    pub fn new(proxy: EventLoopProxy<Event>) -> Self {
        Self {
            proxy,
            korean: Cell::new(false),
            copied: Cell::new(None),
            contrast: Cell::new(paint::Contrast::read()),
            faces: Default::default(),
            bindings: Default::default(),
            icons: Default::default(),
            previews: Default::default(),
            font: Cell::new(null_mut()),
            dpi: Cell::new(96),
            rail_width: Cell::new(0),
            grid_shape: Cell::new(None),
            anchor_right: Cell::new(false),
            search_bounds: Cell::new(RECT::default()),
            hover: Cell::new(0),
            drop_target: Cell::new(0),
            gesture: Default::default(),
            ime: Cell::new(false),
            tip: Cell::new(null_mut()),
        }
    }
    pub fn item_label<'a>(&self, id: Option<i64>, label: &'a str) -> std::borrow::Cow<'a, str> {
        if id.is_some() && id == self.copied.get().map(|(id, _)| id) {
            let status = if self.korean.get() {
                "클립보드에 복사했습니다."
            } else {
                "Copied to clipboard."
            };
            format!("{label} — {status}").into()
        } else {
            label.into()
        }
    }
    fn repaint_item(&self, id: i64) {
        for (&hwnd, b) in self.bindings.borrow().iter() {
            if b.record.item == Some(id) {
                if let Some(face) = self.faces.borrow().get(&hwnd) {
                    super::dialogs::set(hwnd as HWND, &self.item_label(Some(id), &face.text));
                }
                unsafe {
                    InvalidateRect(hwnd as HWND, null(), 0);
                }
            }
        }
    }
    pub fn clear_copy(&self, h: HWND) {
        unsafe {
            KillTimer(h, COPY_TIMER);
        }
        if let Some((id, _)) = self.copied.take() {
            self.repaint_item(id);
        }
    }
    pub fn copy_succeeded(&self, h: HWND, id: i64) {
        self.clear_copy(h);
        let duration = std::time::Duration::from_millis(850);
        if unsafe { SetTimer(h, COPY_TIMER, duration.as_millis() as u32, None) } != 0 {
            self.copied
                .set(Some((id, std::time::Instant::now() + duration)));
            self.repaint_item(id);
        }
    }
    pub(super) fn event(&self, e: Input) {
        let _ = self.proxy.send_event(Event::Ui(e));
    }
    fn mark(cell: &Cell<usize>, next: usize) {
        let old = cell.replace(next);
        if old != next {
            unsafe {
                for h in [old, next] {
                    if h != 0 {
                        InvalidateRect(h as HWND, null(), 0);
                    }
                }
            }
        }
    }
    pub fn cancel(&self) {
        let gest = self.gesture.borrow_mut().take();
        if gest.is_some() {
            unsafe {
                ReleaseCapture();
            }
        }
        Self::mark(&self.drop_target, 0);
        self.hide_tip();
    }
    pub fn hide_tip(&self) {
        let t = self.tip.get();
        if !t.is_null() {
            unsafe {
                ShowWindow(t, SW_HIDE);
            }
        }
    }
}
pub fn child(
    parent: HWND,
    id: i32,
    class: &str,
    label: &str,
    style: u32,
    bridge: &Bridge,
) -> anyhow::Result<HWND> {
    unsafe {
        let h = CreateWindowExW(
            0,
            wide(class).as_ptr(),
            wide(label).as_ptr(),
            WS_CHILD | style,
            0,
            0,
            0,
            0,
            parent,
            id as isize as HMENU,
            GetModuleHandleW(null()),
            null(),
        );
        anyhow::ensure!(
            !h.is_null(),
            "Windows 컨트롤을 만들 수 없습니다: {}",
            std::io::Error::last_os_error()
        );
        if SetWindowSubclass(h, Some(child_proc), 2, bridge as *const Bridge as usize) == 0 {
            DestroyWindow(h);
            anyhow::bail!("컨트롤 입력을 연결할 수 없습니다.");
        }
        SendMessageW(h, WM_SETFONT, bridge.font.get() as usize, 0);
        Ok(h)
    }
}
// Commit all child placements in one window-manager update so the controls
// travel with the frame during a slide.
pub struct PlacementBatch(Vec<(HWND, i32, i32, i32, i32)>);
impl PlacementBatch {
    pub fn new() -> Self {
        Self(Vec::new())
    }
    pub fn position(&mut self, h: HWND, x: i32, y: i32, w: i32, height: i32) {
        self.0.push((h, x, y, w.max(1), height.max(1)));
    }
    pub fn apply(mut self, parent: HWND) {
        unsafe {
            let mut origin = POINT::default();
            ClientToScreen(parent, &mut origin);
            self.0.retain(|&(h, x, y, w, height)| {
                let mut rect = RECT::default();
                GetWindowLongW(h, GWL_STYLE) as u32 & WS_VISIBLE == 0
                    || GetWindowRect(h, &mut rect) == 0
                    || (
                        rect.left - origin.x,
                        rect.top - origin.y,
                        rect.right - rect.left,
                        rect.bottom - rect.top,
                    ) != (x, y, w, height)
            });
            if self.0.is_empty() {
                return;
            }
            let flags = SWP_NOACTIVATE | SWP_NOZORDER | SWP_SHOWWINDOW;
            let mut batch = BeginDeferWindowPos(self.0.len() as i32);
            for &(h, x, y, w, height) in &self.0 {
                if batch.is_null() {
                    break;
                }
                batch = DeferWindowPos(batch, h, null_mut(), x, y, w, height, flags);
            }
            if !batch.is_null() && EndDeferWindowPos(batch) != 0 {
                return;
            }
            for (h, x, y, w, height) in self.0 {
                SetWindowPos(h, null_mut(), x, y, w, height, flags);
            }
        }
    }
}
fn point() -> POINT {
    let mut p = POINT::default();
    unsafe {
        GetCursorPos(&mut p);
    }
    p
}
fn mouse_position(h: HWND, l: LPARAM) -> POINT {
    let mut p = POINT {
        x: l as i16 as i32,
        y: (l >> 16) as i16 as i32,
    };
    unsafe {
        ClientToScreen(h, &mut p);
    }
    p
}
fn track(h: HWND) {
    unsafe {
        let mut info = TRACKMOUSEEVENT {
            cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
            dwFlags: TME_HOVER | TME_LEAVE,
            hwndTrack: h,
            dwHoverTime: 350,
        };
        TrackMouseEvent(&mut info);
    }
}
pub(super) unsafe extern "system" fn child_proc(
    h: HWND,
    msg: u32,
    w: WPARAM,
    l: LPARAM,
    _: usize,
    data: usize,
) -> LRESULT {
    unsafe {
        let b = &*(data as *const Bridge);
        let id = GetDlgCtrlID(h);
        let tile = id >= TILE;
        match msg {
            WM_IME_STARTCOMPOSITION if id == SEARCH => b.ime.set(true),
            WM_IME_ENDCOMPOSITION if id == SEARCH => b.ime.set(false),
            WM_KEYDOWN if !b.ime.get() => {
                let key = w as u16;
                if (TAB..TILE).contains(&id)
                    && matches!(key, VK_UP | VK_DOWN | VK_RIGHT | VK_RETURN)
                {
                    b.event(Input::CategoryKey(id, key));
                    return 0;
                }
                let ctrl = GetKeyState(VK_CONTROL as i32) < 0;
                if ctrl && matches!(key, 0x46 | 0x4b)
                    || key == VK_ESCAPE
                    || ((tile || id == SEARCH) && matches!(key, VK_UP | VK_DOWN | VK_RETURN))
                    || tile
                        && matches!(
                            key,
                            VK_LEFT | VK_RIGHT | VK_HOME | VK_END | VK_PRIOR | VK_NEXT
                        )
                {
                    if key == VK_ESCAPE && b.gesture.borrow().is_some() {
                        b.cancel();
                        return 0;
                    }
                    b.event(Input::Key(key));
                    return 0;
                }
                if key == VK_TAB {
                    b.event(Input::TabFocus(GetKeyState(VK_SHIFT as i32) < 0));
                    return 0;
                }
            }
            WM_CHAR if id == SEARCH && matches!(w, 13 | 27) => return 0,
            WM_CONTEXTMENU if tile => {
                b.cancel();
                let mut p = point();
                if l == -1 {
                    let mut r = RECT::default();
                    GetWindowRect(h, &mut r);
                    p = POINT {
                        x: r.left + 6,
                        y: r.bottom,
                    };
                }
                if let Some(r) = b.bindings.borrow().get(&(h as usize)) {
                    b.event(Input::Context(Some(r.clone()), p.x, p.y));
                }
                return 0;
            }
            WM_MOUSEMOVE => {
                track(h);
                Bridge::mark(&b.hover, h as usize);
                if tile {
                    let p = mouse_position(h, l);
                    let mut drag = b.gesture.borrow_mut();
                    if let Some(g) = drag.as_mut() {
                        g.moved |= (p.x - g.x).abs().max((p.y - g.y).abs()) >= g.threshold;
                        if g.moved {
                            let dest = WindowFromPoint(p);
                            Bridge::mark(
                                &b.drop_target,
                                if b.bindings.borrow().get(&(dest as usize)).is_some_and(|to| {
                                    g.source.record.item.is_some()
                                        && g.source.app.is_some()
                                        && to.app == g.source.app
                                        && to.record.cell.is_some()
                                        && to.record.cell != g.source.record.cell
                                }) {
                                    dest as usize
                                } else {
                                    0
                                },
                            );
                        }
                        return 0;
                    }
                }
            }
            WM_MOUSELEAVE => {
                if b.hover.get() == h as usize {
                    Bridge::mark(&b.hover, 0);
                    b.hide_tip();
                }
            }
            WM_MOUSEHOVER
                if (TAB..TILE).contains(&id)
                    || matches!(id, SMALLER | LARGER | TOGGLE_SIDEBAR | HELP) =>
            {
                b.event(Input::Hover(h as isize, None));
                return 0;
            }
            WM_MOUSEHOVER if tile => {
                if b.gesture.borrow().is_none()
                    && let Some(binding) = b.bindings.borrow().get(&(h as usize))
                    && let Some(id) = binding.record.item
                {
                    b.event(Input::Hover(h as isize, Some(id)));
                }
                return 0;
            }
            WM_LBUTTONDOWN | WM_LBUTTONDBLCLK if tile => {
                b.cancel();
                let p = mouse_position(h, l);
                if let Some(binding) = b.bindings.borrow().get(&(h as usize)).cloned() {
                    SetFocus(h);
                    let threshold = (6 * b.dpi.get() / 96).max(4);
                    *b.gesture.borrow_mut() = Some(Gesture {
                        source: binding,
                        x: p.x,
                        y: p.y,
                        threshold,
                        moved: false,
                    });
                    SetCapture(h);
                    InvalidateRect(h, null(), 0);
                }
                return 0;
            }
            WM_LBUTTONUP if tile => {
                let gest = b.gesture.borrow_mut().take();
                if let Some(mut g) = gest {
                    let p = mouse_position(h, l);
                    g.moved |= (p.x - g.x).abs().max((p.y - g.y).abs()) >= g.threshold;
                    let dest = WindowFromPoint(p);
                    let binding = b.bindings.borrow().get(&(dest as usize)).cloned();
                    ReleaseCapture();
                    InvalidateRect(h, null(), 0);
                    Bridge::mark(&b.drop_target, 0);
                    Bridge::mark(
                        &b.hover,
                        if b.faces.borrow().contains_key(&(dest as usize)) {
                            dest as usize
                        } else {
                            0
                        },
                    );
                    if let Some(to) = binding {
                        if g.moved {
                            if let (Some(app), Some(id), Some(cell)) =
                                (g.source.app.clone(), g.source.record.item, to.record.cell)
                                && to.app.as_ref() == Some(&app)
                                && g.source.record.cell != Some(cell)
                            {
                                b.event(Input::Drop {
                                    app,
                                    rev: g.source.rev,
                                    id,
                                    to: cell,
                                });
                            }
                        } else if dest == h {
                            b.event(Input::Pick(to));
                        }
                    }
                }
                return 0;
            }
            WM_CAPTURECHANGED | WM_CANCELMODE if tile => {
                b.gesture.borrow_mut().take();
                Bridge::mark(&b.drop_target, 0);
                b.hide_tip();
            }
            WM_SETFOCUS | WM_KILLFOCUS if id == SEARCH => {
                InvalidateRect(GetParent(h), &b.search_bounds.get(), 1);
            }
            WM_SETFOCUS if tile => {
                b.event(Input::TileFocused(h as isize));
            }
            WM_MOUSEWHEEL => {
                b.hide_tip();
                b.event(Input::Wheel((w >> 16) as i16 as i32));
                return 0;
            }
            _ => {}
        }
        DefSubclassProc(h, msg, w, l)
    }
}
pub unsafe extern "system" fn parent_proc(
    h: HWND,
    msg: u32,
    w: WPARAM,
    l: LPARAM,
    _: usize,
    data: usize,
) -> LRESULT {
    unsafe {
        let b = &*(data as *const Bridge);
        match msg {
            WM_SIZING if l != 0 => {
                if let Some(shape) = b.grid_shape.get() {
                    let mut outer = RECT::default();
                    let mut client = RECT::default();
                    if GetWindowRect(h, &mut outer) != 0 && GetClientRect(h, &mut client) != 0 {
                        let nonclient = (
                            (outer.right - outer.left) - client.right,
                            (outer.bottom - outer.top) - client.bottom,
                        );
                        shape.snap_frame(&mut *(l as *mut RECT), w as u32, nonclient);
                        return 1;
                    }
                }
            }
            WM_CONTEXTMENU if w == h as usize && l != -1 => {
                let x = l as i16 as i32;
                let y = (l >> 16) as i16 as i32;
                let mut p = POINT { x, y };
                ScreenToClient(h, &mut p);
                let grid_top = (GRID_TOP * b.dpi.get() + 48) / 96;
                if p.x >= b.search_bounds.get().left && p.y >= grid_top {
                    b.cancel();
                    b.event(Input::Context(None, x, y));
                    return 0;
                }
            }
            WM_NCCALCSIZE if w != 0 && l != 0 && b.anchor_right.get() => {
                let params = l as *mut NCCALCSIZE_PARAMS;
                let old = (*params).rgrc[2];
                // Let Windows calculate the native caption and frame first. Then
                // preserve the common client area from its right edge, so pixels
                // and child HWNDs stay together during the resize itself.
                let result = DefSubclassProc(h, msg, w, l);
                let new = (*params).rgrc[0];
                let width = (old.right - old.left).min(new.right - new.left);
                let height = (old.bottom - old.top).min(new.bottom - new.top);
                if width <= 0 || height <= 0 {
                    return result;
                }
                (*params).rgrc[1] = RECT {
                    left: new.right - width,
                    top: new.top,
                    right: new.right,
                    bottom: new.top + height,
                };
                (*params).rgrc[2] = RECT {
                    left: old.right - width,
                    top: old.top,
                    right: old.right,
                    bottom: old.top + height,
                };
                let dx = (new.right - new.left) - (old.right - old.left);
                let mut search = b.search_bounds.get();
                search.left += dx;
                search.right += dx;
                b.search_bounds.set(search);
                b.rail_width.set(b.rail_width.get() + dx);
                return WVR_VALIDRECTS as LRESULT;
            }
            WM_SETTINGCHANGE | WM_SYSCOLORCHANGE | WM_THEMECHANGED => {
                b.contrast.set(paint::Contrast::read());
                RedrawWindow(
                    h,
                    null(),
                    null_mut(),
                    RDW_INVALIDATE | RDW_ERASE | RDW_ALLCHILDREN,
                );
            }
            WM_TIMER if w == COPY_TIMER => {
                if b.copied
                    .get()
                    .is_some_and(|(_, end)| std::time::Instant::now() >= end)
                {
                    b.clear_copy(h);
                }
                return 0;
            }
            WM_TIMER if w == super::grid::SLIDE_TIMER => {
                let _ = b.proxy.send_event(Event::SidebarFrame);
                return 0;
            }
            WM_VSCROLL if l != 0 && GetDlgCtrlID(l as HWND) == CATEGORY_SCROLL => {
                let mut info = SCROLLINFO {
                    cbSize: size_of::<SCROLLINFO>() as u32,
                    fMask: SIF_TRACKPOS,
                    ..Default::default()
                };
                GetScrollInfo(l as HWND, SB_CTL, &mut info);
                b.event(Input::CategoryScroll((w & 0xffff) as i32, info.nTrackPos));
                return 0;
            }
            WM_COMMAND => {
                let id = (w & 0xffff) as i32;
                let code = (w >> 16) as u32;
                if id == SEARCH && code == EN_CHANGE {
                    b.event(Input::Search);
                    return 0;
                }
                if code == BN_CLICKED && l != 0 {
                    if id >= TILE {
                        if let Some(r) = b.bindings.borrow().get(&(l as usize)) {
                            b.event(Input::Pick(r.clone()));
                        }
                    } else {
                        b.event(Input::Control(id));
                    }
                    return 0;
                }
            }
            WM_DRAWITEM if l != 0 => {
                let d = &*(l as *const DRAWITEMSTRUCT);
                let key = d.hwndItem as usize;
                if let Some(face) = b.faces.borrow().get(&key) {
                    let icons = b.icons.borrow();
                    let previews = b.previews.borrow();
                    let image = face
                        .preview
                        .and_then(|key| previews.get(&key)?.as_ref())
                        .or_else(|| face.icon.and_then(|id| icons.get(&id)).map(Rc::as_ref));
                    paint::draw(
                        d.hDC,
                        d.rcItem,
                        face,
                        image,
                        b.font.get(),
                        b.dpi.get(),
                        paint::ButtonState {
                            disabled: d.itemState & ODS_DISABLED != 0,
                            hover: b.hover.get() == key,
                            pressed: d.itemState & ODS_SELECTED != 0 || GetCapture() == d.hwndItem,
                            focus: d.itemState & ODS_FOCUS != 0,
                            drop_here: b.drop_target.get() == key,
                            copied: b.copied.get().is_some_and(|(id, _)| {
                                b.bindings
                                    .borrow()
                                    .get(&key)
                                    .is_some_and(|r| r.record.item == Some(id))
                            }),
                            contrast: b.contrast.get(),
                        },
                    );
                    return 1;
                }
            }
            WM_LBUTTONDOWN => {
                let point = POINT {
                    x: l as i16 as i32,
                    y: (l >> 16) as i16 as i32,
                };
                if PtInRect(&b.search_bounds.get(), point) != 0 {
                    SetFocus(GetDlgItem(h, SEARCH));
                    return 0;
                }
            }
            WM_ERASEBKGND => {
                let mut r = RECT::default();
                GetClientRect(h, &mut r);
                let brush = CreateSolidBrush(b.contrast.get().color(paint::BG, COLOR_WINDOW));
                FillRect(w as HDC, &r, brush);
                DeleteObject(brush);
                r.right = b.rail_width.get();
                let rail = CreateSolidBrush(b.contrast.get().color(paint::RAIL_BG, COLOR_WINDOW));
                FillRect(w as HDC, &r, rail);
                DeleteObject(rail);
                r.left = r.right - 1;
                let edge = CreateSolidBrush(b.contrast.get().color(0xe3dbd5, COLOR_WINDOWTEXT));
                FillRect(w as HDC, &r, edge);
                DeleteObject(edge);
                paint::search_field(
                    w as HDC,
                    b.search_bounds.get(),
                    b.dpi.get(),
                    GetFocus() == GetDlgItem(h, SEARCH),
                    b.contrast.get(),
                );
                return 1;
            }
            WM_MOUSEWHEEL => {
                b.event(Input::Wheel((w >> 16) as i16 as i32));
                return 0;
            }
            WM_KEYDOWN
                if matches!(
                    w as u16,
                    VK_ESCAPE | VK_RETURN | VK_LEFT | VK_RIGHT | VK_UP | VK_DOWN
                ) =>
            {
                b.event(Input::Key(w as u16));
                return 0;
            }
            _ => {}
        }
        DefSubclassProc(h, msg, w, l)
    }
}
