use clip_everything::clipboard::wide;
use std::{
    cell::Cell,
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::{Dwm::DwmFlush, Gdi::*},
    System::LibraryLoader::GetModuleHandleW,
    UI::Shell::{DefSubclassProc, SetWindowSubclass},
    UI::WindowsAndMessaging::*,
};

// DWM may publish a freshly resized top-level surface before its child HWNDs
// finish painting. Keep the unchanged pane in an independent composition surface
// for the slide; the real controls remain in place and receive all input.
#[derive(Default)]
pub struct SlideSurface {
    hwnd: Cell<HWND>,
    size: Cell<SIZE>,
}
impl SlideSurface {
    pub fn hide(&self) {
        let h = self.hwnd.get();
        if !h.is_null() {
            unsafe {
                ShowWindow(h, SW_HIDE);
            }
        }
    }

    pub fn finish(&self) {
        unsafe {
            let layer = self.hwnd.get();
            if !layer.is_null() && IsWindowVisible(layer) != 0 {
                GdiFlush();
                if SetTimer(layer, 1, 40, None) == 0 {
                    self.hide();
                }
            }
        }
    }
    pub fn capture(&self, owner: HWND, rail: i32) {
        unsafe {
            if IsWindowVisible(owner) == 0 || IsIconic(owner) != 0 || IsZoomed(owner) != 0 {
                self.hide();
                return;
            }
            let mut client = RECT::default();
            GetClientRect(owner, &mut client);
            let size = SIZE {
                cx: client.right - rail,
                cy: client.bottom,
            };
            if size.cx <= 0 || size.cy <= 0 {
                self.hide();
                return;
            }
            let dc = GetDC(owner);
            if dc.is_null() {
                self.hide();
                return;
            }
            let mem = CreateCompatibleDC(dc);
            let bitmap = CreateCompatibleBitmap(dc, client.right, client.bottom);
            ReleaseDC(owner, dc);
            let mut rendered = false;
            if !mem.is_null() && !bitmap.is_null() {
                let old = SelectObject(mem, bitmap);
                // Print only our native client (including its children), even when
                // another window overlaps it. Never sample the desktop.
                {
                    SendMessageW(owner, WM_ERASEBKGND, mem as usize, 0);
                    let mut child = GetWindow(owner, GW_CHILD);
                    while !child.is_null() {
                        if IsWindowVisible(child) != 0 {
                            let mut r = RECT::default();
                            GetWindowRect(child, &mut r);
                            let mut origin = POINT {
                                x: r.left,
                                y: r.top,
                            };
                            ScreenToClient(owner, &mut origin);
                            let saved = SaveDC(mem);
                            SetViewportOrgEx(mem, origin.x, origin.y, null_mut());
                            IntersectClipRect(mem, 0, 0, r.right - r.left, r.bottom - r.top);
                            SendMessageW(
                                child,
                                WM_PRINT,
                                mem as usize,
                                (PRF_CLIENT | PRF_NONCLIENT | PRF_ERASEBKGND) as isize,
                            );
                            RestoreDC(mem, saved);
                        }
                        child = GetWindow(child, GW_HWNDNEXT);
                    }
                    let mut layer = self.hwnd.get();
                    if layer.is_null() {
                        layer = CreateWindowExW(
                            WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                            wide("STATIC").as_ptr(),
                            null(),
                            WS_POPUP,
                            0,
                            0,
                            1,
                            1,
                            owner,
                            null_mut(),
                            GetModuleHandleW(null()),
                            null(),
                        );
                        if !layer.is_null() && SetWindowSubclass(layer, Some(layer_proc), 1, 0) == 0
                        {
                            DestroyWindow(layer);
                            layer = null_mut();
                        }
                        self.hwnd.set(layer);
                    }
                    if !layer.is_null() {
                        KillTimer(layer, 1);
                    }
                    let mut pos = POINT { x: rail, y: 0 };
                    ClientToScreen(owner, &mut pos);
                    if !layer.is_null()
                        && UpdateLayeredWindow(
                            layer,
                            null_mut(),
                            &pos,
                            &size,
                            mem,
                            &POINT { x: rail, y: 0 },
                            0,
                            null(),
                            ULW_OPAQUE,
                        ) != 0
                    {
                        self.size.set(size);
                        // Keep the pane immediately above its owner, including
                        // when the user switches to a different foreground app.
                        let above = GetWindow(owner, GW_HWNDPREV);
                        SetWindowPos(
                            layer,
                            above,
                            0,
                            0,
                            0,
                            0,
                            SWP_NOMOVE
                                | SWP_NOSIZE
                                | SWP_NOACTIVATE
                                | SWP_SHOWWINDOW
                                | if above == layer { SWP_NOZORDER } else { 0 },
                        );
                        GdiFlush();
                        DwmFlush();
                        rendered = true;
                    }
                }
                SelectObject(mem, old);
            }
            if !bitmap.is_null() {
                DeleteObject(bitmap);
            }
            if !mem.is_null() {
                DeleteDC(mem);
            }
            if !rendered {
                self.hide();
            }
        }
    }
    pub fn align(&self, owner: HWND, rail: i32) {
        unsafe {
            let h = self.hwnd.get();
            if h.is_null() || IsWindowVisible(h) == 0 {
                return;
            }
            let mut client = RECT::default();
            GetClientRect(owner, &mut client);
            let size = self.size.get();
            if IsZoomed(owner) != 0 || client.right - rail != size.cx || client.bottom != size.cy {
                self.hide();
                return;
            }
            let mut pos = POINT { x: rail, y: 0 };
            ClientToScreen(owner, &mut pos);
            let mut rect = RECT::default();
            GetWindowRect(h, &mut rect);
            if (rect.left, rect.top) != (pos.x, pos.y) {
                SetWindowPos(
                    h,
                    null_mut(),
                    pos.x,
                    pos.y,
                    0,
                    0,
                    SWP_NOACTIVATE | SWP_NOZORDER | SWP_NOSIZE,
                );
            }
        }
    }
}
unsafe extern "system" fn layer_proc(
    h: HWND,
    msg: u32,
    w: WPARAM,
    l: LPARAM,
    _: usize,
    _data: usize,
) -> LRESULT {
    unsafe {
        if msg == WM_TIMER && w == 1 {
            KillTimer(h, 1);
            ShowWindow(h, SW_HIDE);
            return 0;
        }
        DefSubclassProc(h, msg, w, l)
    }
}
impl Drop for SlideSurface {
    fn drop(&mut self) {
        let h = self.hwnd.get();
        if !h.is_null() {
            unsafe {
                DestroyWindow(h);
            }
        }
    }
}
