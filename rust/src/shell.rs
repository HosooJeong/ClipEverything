use crate::{
    Event,
    clipboard::wide,
    native_ui::{MenuAnchor, PopupMenu},
};
use anyhow::{Result, ensure};
pub use clip_everything::preferences::Settings;
use clip_everything::preferences::register_pair;
use std::{
    mem::size_of,
    ptr::{null, null_mut},
};
use tao::{
    dpi::PhysicalSize,
    event_loop::EventLoopProxy,
    platform::windows::{IconExtWindows, WindowExtWindows},
    window::{Icon, Window},
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::PtInRect,
    System::{LibraryLoader::GetModuleHandleW, Registry::*},
    UI::{
        Controls::{LIM_SMALL, LoadIconMetric},
        HiDpi::{GetDpiForWindow, GetSystemMetricsForDpi},
        Input::KeyboardAndMouse::*,
        Shell::*,
        WindowsAndMessaging::*,
    },
};

const TRAY: u32 = WM_APP + 17;
pub const ACTIVATE: u32 = WM_APP + 18;
const APP_ICON_ID: u16 = 101;

pub fn set_window_icons(window: &Window) -> Result<()> {
    let dpi = unsafe { GetDpiForWindow(window.hwnd() as HWND) }.max(96);
    let load = |x, y| {
        let size = unsafe {
            PhysicalSize::new(
                GetSystemMetricsForDpi(x, dpi) as u32,
                GetSystemMetricsForDpi(y, dpi) as u32,
            )
        };
        Icon::from_resource(APP_ICON_ID, Some(size))
    };
    let small = load(SM_CXSMICON, SM_CYSMICON)?;
    let large = load(SM_CXICON, SM_CYICON)?;
    window.set_window_icon(Some(small));
    window.set_taskbar_icon(Some(large));
    Ok(())
}

pub struct Shell {
    hwnd: HWND,
    icon: HICON,
    proxy: Box<EventLoopProxy<Event>>,
    menu: Option<PopupMenu>,
}
impl Shell {
    pub fn new(hwnd: HWND, proxy: EventLoopProxy<Event>) -> Result<Self> {
        unsafe {
            let mut icon = null_mut();
            let loaded = LoadIconMetric(
                GetModuleHandleW(null()),
                APP_ICON_ID as usize as *const u16,
                LIM_SMALL,
                &mut icon,
            );
            ensure!(loaded >= 0 && !icon.is_null(), "Unable to load tray icon");
            let shell = Self {
                hwnd,
                icon,
                proxy: Box::new(proxy),
                menu: None,
            };
            ensure!(
                SetWindowSubclass(
                    hwnd,
                    Some(proc),
                    1,
                    (&*shell.proxy as *const EventLoopProxy<Event>) as usize
                ) != 0,
                "Window subclass failed"
            );
            shell.add_icon()?;
            Ok(shell)
        }
    }
    fn data(&self) -> NOTIFYICONDATAW {
        NOTIFYICONDATAW {
            cbSize: size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: self.hwnd,
            uID: 1,
            ..Default::default()
        }
    }
    pub fn add_icon(&self) -> Result<()> {
        unsafe {
            let mut d = self.data();
            d.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
            d.uCallbackMessage = TRAY;
            d.hIcon = self.icon;
            copy_text(&mut d.szTip, "ClipEverything");
            ensure!(
                Shell_NotifyIconW(NIM_MODIFY, &d) != 0 || Shell_NotifyIconW(NIM_ADD, &d) != 0,
                "Cannot create tray icon"
            );
            Ok(())
        }
    }
    pub fn notify(&self, msg: &str) {
        unsafe {
            let mut d = self.data();
            d.uFlags = NIF_INFO;
            copy_text(&mut d.szInfoTitle, "ClipEverything");
            copy_text(&mut d.szInfo, msg);
            d.dwInfoFlags = NIIF_INFO;
            Shell_NotifyIconW(NIM_MODIFY, &d);
        }
    }
    pub fn menu(&mut self, ko: bool) -> Result<()> {
        let mut cursor = POINT::default();
        let mut anchor = RECT::default();
        unsafe {
            if GetCursorPos(&mut cursor) == 0 {
                return Ok(());
            }
            let id = NOTIFYICONIDENTIFIER {
                cbSize: size_of::<NOTIFYICONIDENTIFIER>() as u32,
                hWnd: self.hwnd,
                uID: 1,
                ..Default::default()
            };
            if Shell_NotifyIconGetRect(&id, &mut anchor) < 0 || PtInRect(&anchor, cursor) == 0 {
                anchor = RECT {
                    left: cursor.x,
                    top: cursor.y,
                    right: cursor.x,
                    bottom: cursor.y,
                };
            }
        }
        let menu = match &mut self.menu {
            Some(menu) => menu,
            None => self
                .menu
                .insert(PopupMenu::new(null_mut(), self.proxy.as_ref().clone())?),
        };
        let title = if ko {
            "ClipEverything 트레이 메뉴"
        } else {
            "ClipEverything tray menu"
        };
        let items = vec![
            (
                if ko {
                    "버튼판 열기"
                } else {
                    "Open button board"
                },
                Event::Open(false),
            ),
            (if ko { "설정" } else { "Settings" }, Event::Open(true)),
            (if ko { "종료" } else { "Quit" }, Event::Quit),
        ];
        menu.show(
            title,
            MenuAnchor::Tray {
                rect: anchor,
                cursor,
            },
            items,
            Some(2),
        )
    }
    pub fn unbind(&self) {
        unsafe {
            UnregisterHotKey(self.hwnd, 1);
            UnregisterHotKey(self.hwnd, 2);
        }
    }
    pub fn bind(&self, s: &Settings) -> Result<()> {
        self.unbind();
        register_pair(self.hwnd as isize, s)
    }
}
fn copy_text(dst: &mut [u16], s: &str) {
    let n = dst.len() - 1;
    for (d, c) in dst.iter_mut().zip(s.encode_utf16().take(n)) {
        *d = c;
    }
}
impl Drop for Shell {
    fn drop(&mut self) {
        unsafe {
            self.unbind();
            RemoveWindowSubclass(self.hwnd, Some(proc), 1);
            Shell_NotifyIconW(NIM_DELETE, &self.data());
            DestroyIcon(self.icon);
        }
    }
}
unsafe extern "system" fn proc(
    hwnd: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
    _id: usize,
    data: usize,
) -> LRESULT {
    unsafe {
        let event = match msg {
            WM_HOTKEY if wp == 1 => Some(Event::Capture),
            WM_HOTKEY if wp == 2 => Some(Event::Palette),
            ACTIVATE => Some(Event::Open(false)),
            TRAY if lp as u32 == WM_LBUTTONUP => Some(Event::Open(false)),
            TRAY if lp as u32 == WM_RBUTTONUP => Some(Event::Menu),
            _ => None,
        };
        if let Some(event) = event {
            let _ = (*(data as *const EventLoopProxy<Event>)).send_event(event);
            return 0;
        }
        if msg >= 0xc000 && msg == RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()) {
            let _ = (*(data as *const EventLoopProxy<Event>)).send_event(Event::TrayReset);
        }
        DefSubclassProc(hwnd, msg, wp, lp)
    }
}
fn startup_command(profile: Option<&std::path::Path>) -> Result<String> {
    let mut command = format!("\"{}\" --background", std::env::current_exe()?.display());
    if let Some(path) = profile {
        command.push_str(&format!(" --profile \"{}\"", path.display()));
    }
    Ok(command)
}
pub fn startup(enabled: bool, profile: Option<&std::path::Path>) -> Result<()> {
    unsafe {
        let mut key = null_mut();
        let path = wide("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
        ensure!(
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                path.as_ptr(),
                0,
                null(),
                0,
                KEY_SET_VALUE,
                null(),
                &mut key,
                null_mut()
            ) == 0,
            "Cannot open startup settings"
        );
        let name = wide("ClipEverythingRust");
        let code = if enabled {
            let text = wide(&startup_command(profile)?);
            RegSetValueExW(
                key,
                name.as_ptr(),
                0,
                REG_SZ,
                text.as_ptr().cast(),
                (text.len() * 2) as u32,
            )
        } else {
            RegDeleteValueW(key, name.as_ptr())
        };
        RegCloseKey(key);
        ensure!(
            code == 0 || (!enabled && code == ERROR_FILE_NOT_FOUND),
            "Cannot update startup setting ({code})"
        );
        Ok(())
    }
}

pub fn owns_startup(profile: Option<&std::path::Path>) -> Result<bool> {
    use clip_everything::preferences::{RUN_KEY, RUN_NAME, reg_string, same_dir};
    let Some(actual) = reg_string(RUN_KEY, RUN_NAME)? else {
        return Ok(false);
    };
    let expected = startup_command(profile)?;
    if actual.eq_ignore_ascii_case(&expected) {
        return Ok(true);
    }
    let base = startup_command(None)?;
    if let Some(dir) = profile {
        let default_dir = std::env::var_os("APPDATA")
            .map(std::path::PathBuf::from)
            .map(|p| p.join("ClipEverythingRust"));
        return Ok(
            actual.eq_ignore_ascii_case(&base) && default_dir.is_some_and(|p| same_dir(&p, dir))
        );
    }
    Ok(actual
        .to_lowercase()
        .starts_with(&(base.to_lowercase() + " --profile ")))
}
pub fn remove_own_startup() -> Result<()> {
    if owns_startup(None)? {
        startup(false, None)?;
    }
    Ok(())
}
