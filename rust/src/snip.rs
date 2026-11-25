use anyhow::{Result, ensure};
use clip_everything::clipboard::wide;
use image::{DynamicImage, ImageFormat, RgbImage};
use std::{
    io::Cursor,
    mem::size_of,
    ptr::{null, null_mut},
    thread,
    time::Duration,
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::LibraryLoader::GetModuleHandleW,
    UI::{
        Input::KeyboardAndMouse::{ReleaseCapture, SetCapture, VK_ESCAPE},
        WindowsAndMessaging::*,
    },
};

#[derive(Default)]
struct Selection {
    start: Option<POINT>,
    end: POINT,
    done: bool,
    korean: bool,
}
impl Selection {
    fn bounds(&self) -> Option<RECT> {
        let p = self.start?;
        Some(RECT {
            left: p.x.min(self.end.x),
            top: p.y.min(self.end.y),
            right: p.x.max(self.end.x),
            bottom: p.y.max(self.end.y),
        })
    }
}
fn mouse(hwnd: HWND) -> POINT {
    unsafe {
        let mut p = POINT::default();
        GetCursorPos(&mut p);
        ScreenToClient(hwnd, &mut p);
        p
    }
}
unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        if msg == WM_NCCREATE {
            let create = &*(lp as *const CREATESTRUCTW);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
        }
        let state = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Selection;
        if state.is_null() {
            return DefWindowProcW(hwnd, msg, wp, lp);
        }
        match msg {
            WM_LBUTTONDOWN => {
                (*state).end = mouse(hwnd);
                (*state).start = Some((*state).end);
                SetCapture(hwnd);
                0
            }
            WM_MOUSEMOVE if (*state).start.is_some() => {
                (*state).end = mouse(hwnd);
                InvalidateRect(hwnd, null(), 0);
                0
            }
            WM_LBUTTONUP if (*state).start.is_some() => {
                (*state).end = mouse(hwnd);
                (*state).done = true;
                ReleaseCapture();
                DestroyWindow(hwnd);
                0
            }
            WM_KEYDOWN if wp == VK_ESCAPE as usize => {
                DestroyWindow(hwnd);
                0
            }
            WM_RBUTTONDOWN | WM_CLOSE | WM_KILLFOCUS => {
                DestroyWindow(hwnd);
                0
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                0
            }
            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                let dc = BeginPaint(hwnd, &mut ps);
                FillRect(dc, &ps.rcPaint, GetStockObject(BLACK_BRUSH) as HBRUSH);
                if let Some(rect) = (*state).bounds() {
                    FrameRect(dc, &rect, GetStockObject(WHITE_BRUSH) as HBRUSH);
                }
                SetTextColor(dc, 0xffffff);
                SetBkMode(dc, TRANSPARENT as i32);
                let hint = wide(if (*state).korean {
                    "미리보기 영역을 드래그  ·  Esc 취소"
                } else {
                    "Drag a preview area  ·  Esc to cancel"
                });
                let mut cursor = POINT::default();
                GetCursorPos(&mut cursor);
                let monitor = MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST);
                let mut info = MONITORINFO {
                    cbSize: size_of::<MONITORINFO>() as u32,
                    ..Default::default()
                };
                GetMonitorInfoW(monitor, &mut info);
                let mut label = POINT {
                    x: info.rcWork.left + 24,
                    y: info.rcWork.top + 24,
                };
                ScreenToClient(hwnd, &mut label);
                TextOutW(dc, label.x, label.y, hint.as_ptr(), (hint.len() - 1) as i32);
                EndPaint(hwnd, &ps);
                0
            }
            _ => DefWindowProcW(hwnd, msg, wp, lp),
        }
    }
}
// Only the explicit selection is copied. The selector is closed before accessing pixels.
pub fn select(korean: bool) -> Result<Option<Vec<u8>>> {
    unsafe {
        let x = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let y = GetSystemMetrics(SM_YVIRTUALSCREEN);
        let w = GetSystemMetrics(SM_CXVIRTUALSCREEN);
        let h = GetSystemMetrics(SM_CYVIRTUALSCREEN);
        let class = wide("ClipEverythingRustSnip");
        let instance = GetModuleHandleW(null());
        let wc = WNDCLASSW {
            lpfnWndProc: Some(proc),
            hInstance: instance,
            hCursor: LoadCursorW(null_mut(), IDC_CROSS),
            lpszClassName: class.as_ptr(),
            ..Default::default()
        };
        RegisterClassW(&wc);
        let mut s = Selection {
            korean,
            ..Default::default()
        };
        let win = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_LAYERED,
            class.as_ptr(),
            wide("ClipEverything · Select area").as_ptr(),
            WS_POPUP,
            x,
            y,
            w,
            h,
            null_mut(),
            null_mut(),
            instance,
            (&mut s as *mut Selection).cast(),
        );
        ensure!(!win.is_null(), "Cannot open screen selection");
        SetLayeredWindowAttributes(win, 0, 100, LWA_ALPHA);
        ShowWindow(win, SW_SHOW);
        SetForegroundWindow(win);
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        if !s.done {
            return Ok(None);
        }
        let rect = s.bounds().unwrap();
        let (width, height) = (rect.right - rect.left, rect.bottom - rect.top);
        if width < 2 || height < 2 {
            return Ok(None);
        }
        thread::sleep(Duration::from_millis(100));
        let rgb = capture(x + rect.left, y + rect.top, width, height)?;
        let image = DynamicImage::ImageRgb8(rgb);
        let preview = if width > 960 || height > 640 {
            image.thumbnail(960, 640)
        } else {
            image
        };
        let mut buf = Cursor::new(Vec::new());
        preview.write_to(&mut buf, ImageFormat::Png)?;
        Ok(Some(buf.into_inner()))
    }
}
fn capture(x: i32, y: i32, w: i32, h: i32) -> Result<RgbImage> {
    unsafe {
        ensure!(
            i64::from(w) * i64::from(h) <= 64_000_000,
            "Selected area is too large"
        );
        let screen = GetDC(null_mut());
        ensure!(!screen.is_null(), "Cannot read screen");
        let dc = CreateCompatibleDC(screen);
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut pixels = null_mut();
        let bmp = CreateDIBSection(screen, &info, DIB_RGB_COLORS, &mut pixels, null_mut(), 0);
        let result = (|| {
            ensure!(
                !dc.is_null() && !bmp.is_null() && !pixels.is_null(),
                "Unable to allocate screen capture"
            );
            let prev = SelectObject(dc, bmp);
            let ok = BitBlt(dc, 0, 0, w, h, screen, x, y, SRCCOPY | CAPTUREBLT);
            GdiFlush();
            SelectObject(dc, prev);
            ensure!(ok != 0, "Unable to capture selected area");
            let bytes =
                std::slice::from_raw_parts(pixels.cast::<u8>(), w as usize * h as usize * 4);
            let rgb = bytes
                .chunks_exact(4)
                .flat_map(|p| [p[2], p[1], p[0]])
                .collect();
            RgbImage::from_raw(w as u32, h as u32, rgb)
                .ok_or_else(|| anyhow::anyhow!("Invalid screenshot"))
        })();
        if !bmp.is_null() {
            DeleteObject(bmp);
        }
        if !dc.is_null() {
            DeleteDC(dc);
        }
        ReleaseDC(null_mut(), screen);
        result
    }
}
