use crate::store::{Format, Source};
use anyhow::{Context, Result, anyhow, bail, ensure};
use std::{
    ffi::c_void,
    mem::{size_of, zeroed},
    path::Path,
    ptr::{copy_nonoverlapping, null_mut},
    slice, thread,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::{DataExchange::*, Memory::*, Threading::*},
    UI::{Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
};

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain([0]).collect()
}
pub fn foreground() -> Source {
    unsafe {
        let hwnd = GetForegroundWindow();
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, &mut pid);
        let mut title = [0; 1024];
        let n = GetWindowTextW(hwnd, title.as_mut_ptr(), 1024).max(0) as usize;
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        let mut exe = String::new();
        if !handle.is_null() {
            let mut path = vec![0; 32768];
            let mut len = path.len() as u32;
            if QueryFullProcessImageNameW(handle, 0, path.as_mut_ptr(), &mut len) != 0 {
                exe = String::from_utf16_lossy(&path[..len as usize]);
            }
            CloseHandle(handle);
        }
        Source {
            hwnd: hwnd as isize,
            pid,
            app: Path::new(&exe)
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            title: String::from_utf16_lossy(&title[..n]),
            exe,
        }
    }
}
fn shortcut(key: u16) -> Result<()> {
    unsafe {
        let mut events: Vec<INPUT> = Vec::new();
        let mut push = |vk, flags| {
            let mut i: INPUT = zeroed();
            i.r#type = INPUT_KEYBOARD;
            i.Anonymous.ki.wVk = vk;
            i.Anonymous.ki.dwFlags = flags;
            events.push(i);
        };
        for k in [
            VK_LWIN,
            VK_RWIN,
            VK_LCONTROL,
            VK_RCONTROL,
            VK_LSHIFT,
            VK_RSHIFT,
            VK_LMENU,
            VK_RMENU,
        ] {
            if GetAsyncKeyState(k as i32) < 0 {
                push(k, KEYEVENTF_KEYUP);
            }
        }
        push(VK_CONTROL, 0);
        push(key, 0);
        push(key, KEYEVENTF_KEYUP);
        push(VK_CONTROL, KEYEVENTF_KEYUP);
        ensure!(
            SendInput(
                events.len() as u32,
                events.as_ptr(),
                size_of::<INPUT>() as i32
            ) == events.len() as u32,
            "Windows rejected keyboard input"
        );
        Ok(())
    }
}
pub fn copy_selection(owner: isize, src: &Source) -> Result<Vec<Format>> {
    ensure!(matches_target(src), "Copy target changed");
    let before = unsafe { GetClipboardSequenceNumber() };
    shortcut(0x43)?;
    let deadline = Instant::now() + Duration::from_millis(1200);
    while unsafe { GetClipboardSequenceNumber() } == before {
        ensure!(
            Instant::now() < deadline,
            "Copy did not update the clipboard"
        );
        thread::sleep(Duration::from_millis(12));
    }
    snapshot(owner)
}
fn matches_target(src: &Source) -> bool {
    unsafe {
        let hwnd = GetForegroundWindow();
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, &mut pid);
        !hwnd.is_null() && hwnd as isize == src.hwnd && pid == src.pid
    }
}
pub fn paste_target(src: &Source) -> Result<()> {
    focus_and_paste(src, || true)
}

/// A delayed restore must not steal focus from a third app or paste a newer clipboard.
pub fn paste_restored(
    owner: isize,
    sequence: u32,
    src: &Source,
    cancelled: &std::sync::atomic::AtomicBool,
) -> Result<()> {
    unsafe {
        ensure!(
            GetForegroundWindow() as isize == owner || matches_target(src),
            "입력 창이 변경되었습니다. Ctrl+V로 붙여넣을 수 있습니다."
        );
        focus_and_paste(src, || {
            !cancelled.load(std::sync::atomic::Ordering::Acquire)
                && GetClipboardOwner() as isize == owner
                && GetClipboardSequenceNumber() == sequence
        })
    }
}
fn focus_and_paste(src: &Source, current: impl Fn() -> bool) -> Result<()> {
    unsafe {
        let hwnd = src.hwnd as HWND;
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, &mut pid);
        ensure!(IsWindow(hwnd) != 0 && src.pid == pid, "Paste target closed");
        ensure!(current(), "붙여넣기 요청이 변경되었습니다.");
        if !matches_target(src) {
            let original = GetForegroundWindow();
            ensure!(
                SetForegroundWindow(hwnd) != 0 || matches_target(src),
                "대상 창을 활성화하지 못했습니다. Ctrl+V로 붙여넣을 수 있습니다."
            );
            let deadline = Instant::now() + Duration::from_millis(400);
            while !matches_target(src) {
                let foreground = GetForegroundWindow();
                ensure!(
                    current()
                        && Instant::now() < deadline
                        && (foreground.is_null() || foreground == original || foreground == hwnd),
                    "대상 창을 확인하지 못했습니다. Ctrl+V로 붙여넣을 수 있습니다."
                );
                thread::sleep(Duration::from_millis(8));
            }
        }
        ensure!(
            current() && matches_target(src),
            "붙여넣기 대상이 변경되었습니다."
        );
        shortcut(0x56)
    }
}

struct Clipboard;
impl Clipboard {
    fn open(owner: isize) -> Result<Self> {
        for _ in 0..12 {
            if unsafe { OpenClipboard(owner as HWND) } != 0 {
                return Ok(Self);
            }
            thread::sleep(Duration::from_millis(10));
        }
        bail!("Clipboard is busy")
    }
}
impl Drop for Clipboard {
    fn drop(&mut self) {
        unsafe {
            CloseClipboard();
        }
    }
}
struct Locked(HGLOBAL, *mut c_void);
impl Locked {
    fn new(h: HGLOBAL) -> Result<Self> {
        let p = unsafe { GlobalLock(h) };
        ensure!(!p.is_null(), "GlobalLock failed");
        Ok(Self(h, p))
    }
}
impl Drop for Locked {
    fn drop(&mut self) {
        unsafe {
            GlobalUnlock(self.0);
        }
    }
}
fn reg(name: &str) -> u32 {
    unsafe { RegisterClipboardFormatW(wide(name).as_ptr()) }
}
fn name(id: u32) -> String {
    if id >= 0xc000 {
        let mut buf = [0; 256];
        let n = unsafe { GetClipboardFormatNameW(id, buf.as_mut_ptr(), 256) };
        return String::from_utf16_lossy(&buf[..n.max(0) as usize]);
    }
    match id {
        1 => "Text",
        3 => "MetafilePicture",
        6 => "TIFF",
        8 => "DeviceIndependentBitmap",
        9 => "Palette",
        13 => "UnicodeText",
        14 => "EnhancedMetafile",
        15 => "FileDrop",
        16 => "Locale",
        17 => "DeviceIndependentBitmapV5",
        _ => return format!("Format{id}"),
    }
    .into()
}
fn bits(handle: HANDLE, enhanced: bool) -> Result<Vec<u8>> {
    unsafe {
        let get = |len, ptr: *mut u8| {
            if enhanced {
                GetEnhMetaFileBits(handle, len, ptr)
            } else {
                GetMetaFileBitsEx(handle, len, ptr.cast())
            }
        };
        let len = get(0, null_mut());
        ensure!(len > 0, "Empty metafile");
        let mut data = vec![0; len as usize];
        ensure!(get(len, data.as_mut_ptr()) == len, "Incomplete metafile");
        Ok(data)
    }
}
fn capture(id: u32, h: HANDLE) -> Result<Vec<u8>> {
    unsafe {
        match id {
            14 => bits(h, true),
            3 => {
                ensure!(
                    GlobalSize(h) >= size_of::<METAFILEPICT>(),
                    "Invalid metafile"
                );
                let lock = Locked::new(h)?;
                let p = &*lock.1.cast::<METAFILEPICT>();
                let data = bits(p.hMF, false)?;
                let mut out = Vec::new();
                for x in [p.mm, p.xExt, p.yExt, data.len() as i32] {
                    out.extend(x.to_le_bytes())
                }
                out.extend(data);
                Ok(out)
            }
            9 => {
                let n = GetPaletteEntries(h, 0, 0, null_mut());
                ensure!(n > 0 && n <= 65535, "Invalid palette");
                let mut entries = vec![PALETTEENTRY::default(); n as usize];
                ensure!(
                    GetPaletteEntries(h, 0, n, entries.as_mut_ptr()) == n,
                    "Incomplete palette"
                );
                let mut bytes = vec![0, 3];
                bytes.extend((n as u16).to_le_bytes());
                for e in entries {
                    bytes.extend([e.peRed, e.peGreen, e.peBlue, e.peFlags]);
                }
                Ok(bytes)
            }
            _ => {
                let n = GlobalSize(h);
                ensure!(n > 0, "Empty format");
                let lock = Locked::new(h)?;
                Ok(slice::from_raw_parts(lock.1.cast::<u8>(), n).to_vec())
            }
        }
    }
}
fn supported(id: u32) -> bool {
    matches!(id, 1 | 3..=17 | 0xc000..=0xffff)
}
fn transferable(name: &str) -> bool {
    // OLE bookkeeping belongs to the live data object, not the saved document.
    !matches!(name, "DataObject" | "Ole Private Data")
}
pub fn snapshot(owner: isize) -> Result<Vec<Format>> {
    unsafe {
        let _clipboard = Clipboard::open(owner)?;
        for marker in [
            "ExcludeClipboardContentFromMonitorProcessing",
            "Clipboard Viewer Ignore",
        ] {
            ensure!(
                IsClipboardFormatAvailable(reg(marker)) == 0,
                "This item requested exclusion from history"
            );
        }
        let policy = reg("CanIncludeInClipboardHistory");
        if IsClipboardFormatAvailable(policy) != 0 {
            let h = GetClipboardData(policy);
            ensure!(
                !h.is_null() && GlobalSize(h) >= 4,
                "Unreadable clipboard history policy"
            );
            ensure!(
                *(Locked::new(h)?.1 as *const u32) != 0,
                "This item requested exclusion from history"
            );
        }
        let mut out = Vec::new();
        let mut id = EnumClipboardFormats(0);
        while id != 0 {
            let name = name(id);
            if supported(id) && !name.is_empty() && transferable(&name) {
                let h = GetClipboardData(id);
                if !h.is_null()
                    && let Ok(data) = capture(id, h)
                {
                    out.push(Format { id, name, data });
                }
            }
            id = EnumClipboardFormats(id);
        }
        ensure!(!out.is_empty(), "No supported clipboard formats");
        Ok(out)
    }
}

// Own each object until Windows accepts it. Standard and GDI formats use different destructors.
struct Owned {
    id: u32,
    handle: HANDLE,
    nested: HANDLE,
}
impl Drop for Owned {
    fn drop(&mut self) {
        unsafe {
            if self.handle.is_null() {
                return;
            }
            match self.id {
                14 => {
                    DeleteEnhMetaFile(self.handle);
                }
                9 => {
                    DeleteObject(self.handle);
                }
                _ => {
                    GlobalFree(self.handle);
                }
            }
            if !self.nested.is_null() {
                DeleteMetaFile(self.nested);
            }
        }
    }
}
impl Owned {
    fn prepare(f: &Format) -> Result<Self> {
        unsafe {
            ensure!(supported(f.id), "Unsupported format {}", f.name);
            let id = if f.id >= 0xc000 {
                ensure!(!f.name.is_empty(), "Missing format name");
                reg(&f.name)
            } else {
                f.id
            };
            ensure!(id != 0 && !f.data.is_empty(), "Invalid clipboard format");
            let bytes = &f.data;
            let mut object = Self {
                id,
                handle: null_mut(),
                nested: null_mut(),
            };
            match id {
                14 => object.handle = SetEnhMetaFileBits(bytes.len().try_into()?, bytes.as_ptr()),
                9 => {
                    ensure!(bytes.len() >= 4 && bytes[..2] == [0, 3], "Invalid palette");
                    let n = u16::from_le_bytes([bytes[2], bytes[3]]) as usize;
                    ensure!(n > 0 && bytes.len() == 4 + n * 4, "Invalid palette size");
                    let aligned: Vec<u32> = bytes
                        .chunks_exact(4)
                        .map(|x| u32::from_le_bytes(x.try_into().unwrap()))
                        .collect();
                    object.handle = CreatePalette(aligned.as_ptr().cast());
                }
                _ => {
                    let size = if id == 3 {
                        size_of::<METAFILEPICT>()
                    } else {
                        bytes.len()
                    };
                    object.handle = GlobalAlloc(GMEM_MOVEABLE, size);
                    let lock = Locked::new(object.handle)?;
                    if id == 3 {
                        ensure!(bytes.len() > 16, "Invalid metafile header");
                        let field =
                            |i: usize| i32::from_le_bytes(bytes[i..i + 4].try_into().unwrap());
                        ensure!(
                            field(12) as usize == bytes.len() - 16,
                            "Invalid metafile length"
                        );
                        object.nested =
                            SetMetaFileBitsEx((bytes.len() - 16).try_into()?, bytes[16..].as_ptr());
                        ensure!(!object.nested.is_null(), "Invalid metafile data");
                        lock.1.cast::<METAFILEPICT>().write(METAFILEPICT {
                            mm: field(0),
                            xExt: field(4),
                            yExt: field(8),
                            hMF: object.nested,
                        });
                    } else {
                        copy_nonoverlapping(bytes.as_ptr(), lock.1.cast(), size);
                    }
                }
            }
            ensure!(!object.handle.is_null(), "Unable to prepare {}", f.name);
            Ok(object)
        }
    }
}
pub fn restore(owner: isize, formats: &[Format]) -> Result<()> {
    restore_if(owner, formats, || true).map(|_| ())
}
fn prepare_file_copy(drop: &Format) -> Result<Vec<Owned>> {
    // DROPFILES is a 20-byte header followed by a double-NUL-terminated list.
    let bytes = &drop.data;
    ensure!(bytes.len() >= 20, "파일 목록 헤더를 읽을 수 없습니다.");
    let offset = u32::from_le_bytes(bytes[..4].try_into()?) as usize;
    ensure!(
        offset >= 20 && offset < bytes.len(),
        "파일 목록 위치가 올바르지 않습니다."
    );
    let names = &bytes[offset..];
    let wide = bytes[16..20] != [0; 4];
    let terminated = if wide {
        offset.is_multiple_of(2)
            && names.len() >= 6
            && names[..2] != [0, 0]
            && names.windows(4).step_by(2).any(|s| s == [0; 4])
    } else {
        names.first() != Some(&0) && names.windows(2).any(|s| s == [0; 2])
    };
    ensure!(terminated, "파일 목록이 완전하지 않습니다.");
    // Shell IDLists, descriptor objects and asynchronous transfer flags belong
    // to the original Explorer operation. Publish a new, ordinary file copy.
    let effect = Format {
        id: reg("Preferred DropEffect"),
        name: "Preferred DropEffect".into(),
        data: 1u32.to_le_bytes().to_vec(),
    };
    Ok(vec![Owned::prepare(drop)?, Owned::prepare(&effect)?])
}
/// Cancellation is safe until EmptyClipboard; after that, finish publishing every format.
pub fn restore_if(
    owner: isize,
    formats: &[Format],
    current: impl Fn() -> bool,
) -> Result<Option<u32>> {
    unsafe {
        ensure!(owner != 0 && !formats.is_empty(), "No clipboard data");
        // HWP 2020 rejects an archived native clip when its companion Info
        // format is replayed. Keep the original bytes in the DB for inspection.
        let has_hwp = formats
            .iter()
            .any(|f| f.name.eq_ignore_ascii_case("Hwp Native"));
        let shell_files = formats.iter().find(|f| f.id == 15).filter(|_| {
            formats
                .iter()
                .any(|f| f.name.eq_ignore_ascii_case("Shell IDList Array"))
        });
        let mut prepared = if let Some(drop) = shell_files {
            prepare_file_copy(drop)?
        } else {
            formats
                .iter()
                .filter(|f| supported(f.id) && transferable(&f.name))
                .filter(|f| !(has_hwp && f.name.eq_ignore_ascii_case("Hwp_Native_Info")))
                .map(Owned::prepare)
                .collect::<Result<Vec<_>>>()?
        };
        ensure!(!prepared.is_empty(), "No transferable clipboard formats");
        if !current() {
            return Ok(None);
        }
        let clipboard = Clipboard::open(owner)?;
        if !current() {
            return Ok(None);
        }
        ensure!(EmptyClipboard() != 0, "Cannot clear clipboard");
        for object in &mut prepared {
            if SetClipboardData(object.id, object.handle).is_null() {
                return Err(anyhow!("Windows rejected format {}", object.id))
                    .context("Clipboard partially restored");
            }
            object.handle = null_mut();
            object.nested = null_mut();
        }
        drop(clipboard);
        Ok(Some(GetClipboardSequenceNumber()))
    }
}
