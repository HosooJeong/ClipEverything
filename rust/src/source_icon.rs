use image::{DynamicImage, ImageFormat, RgbaImage};
use std::{
    collections::HashSet, io::Cursor, mem::size_of, os::windows::ffi::OsStrExt, path::Path,
    ptr::null_mut,
};
use windows_sys::Win32::{
    Graphics::Gdi::*,
    UI::{Shell::*, WindowsAndMessaging::*},
};

use anyhow::Result;
use clip_everything::clipboard::wide;
use rusqlite::{Connection, OptionalExtension, params};
use windows_sys::Win32::{Globalization::GetUserDefaultUILanguage, Storage::FileSystem::*};

pub fn import_existing(db: &Connection) -> Result<()> {
    let mut q=db.prepare("SELECT c.SourceApp,c.ExecutablePath FROM ClipboardItems c LEFT JOIN SourcePrograms p ON p.App=c.SourceApp COLLATE NOCASE WHERE p.Id IS NULL OR (p.Exe='' AND c.ExecutablePath<>'') ORDER BY (c.ExecutablePath<>'') DESC,c.LastCopiedAt DESC,c.Id DESC")?;
    let rows = q
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    let tx = db.unchecked_transaction()?;
    let mut seen = HashSet::new();
    for (app, exe) in rows {
        if seen.insert(app.to_lowercase()) {
            remember(&tx, &app, &exe)?;
        }
    }
    tx.commit()?;
    Ok(())
}
pub fn remember(db: &Connection, app: &str, exe: &str) -> Result<()> {
    if app.is_empty() {
        return Ok(());
    }
    let known = db
        .query_row("SELECT Exe FROM SourcePrograms WHERE App=?", [app], |r| {
            r.get::<_, String>(0)
        })
        .optional()?;
    if known
        .as_deref()
        .is_some_and(|old| !old.is_empty() || exe.is_empty())
    {
        return Ok(());
    }
    let path = Path::new(exe);
    let local = matches!(path.components().next(),Some(std::path::Component::Prefix(p)) if matches!(p.kind(),std::path::Prefix::Disk(_)|std::path::Prefix::VerbatimDisk(_)))
        && path.is_absolute()
        && path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("exe"))
        && path.is_file();
    let (label, icon) = if local {
        let name = path
            .as_os_str()
            .encode_wide()
            .chain([0])
            .collect::<Vec<_>>();
        (display_name(&name), extract_icon(&name))
    } else {
        (None, None)
    };
    db.execute("INSERT INTO SourcePrograms(App,Exe,Label,Icon) VALUES(?,?,?,?) ON CONFLICT(App) DO UPDATE SET Exe=excluded.Exe,Label=excluded.Label,Icon=excluded.Icon",params![app,if local{exe}else{""},label.as_deref().unwrap_or(app),icon])?;
    Ok(())
}
fn display_name(path: &[u16]) -> Option<String> {
    unsafe {
        let size = GetFileVersionInfoSizeW(path.as_ptr(), null_mut());
        if size == 0 {
            return None;
        }
        let mut block = vec![0u32; (size as usize).div_ceil(4)];
        if GetFileVersionInfoW(path.as_ptr(), 0, size, block.as_mut_ptr().cast()) == 0 {
            return None;
        }
        let query = |key: &str| {
            let mut p = null_mut();
            let mut len = 0;
            (VerQueryValueW(block.as_ptr().cast(), wide(key).as_ptr(), &mut p, &mut len) != 0
                && !p.is_null()
                && len > 0)
                .then_some((p, len as usize))
        };
        let (p, bytes) = query(r"\VarFileInfo\Translation")?;
        let mut translations = std::slice::from_raw_parts(p.cast::<u16>(), bytes / 2)
            .chunks_exact(2)
            .map(|v| (v[0], v[1]))
            .collect::<Vec<_>>();
        let lang = GetUserDefaultUILanguage();
        translations.sort_by_key(|v| v.0 != lang);
        for field in ["FileDescription", "ProductName"] {
            for &(lang, cp) in &translations {
                if let Some((p, n)) = query(&format!(r"\StringFileInfo\{lang:04x}{cp:04x}\{field}"))
                {
                    let value =
                        String::from_utf16_lossy(std::slice::from_raw_parts(p.cast::<u16>(), n));
                    let text = value.trim_matches('\0').trim();
                    if !text.is_empty() {
                        return Some(text.into());
                    }
                }
            }
        }
        None
    }
}
fn extract_icon(name: &[u16]) -> Option<Vec<u8>> {
    let mut info = SHFILEINFOW::default();
    unsafe {
        if SHGetFileInfoW(
            name.as_ptr(),
            0,
            &mut info,
            size_of::<SHFILEINFOW>() as u32,
            SHGFI_ICON | SHGFI_LARGEICON,
        ) == 0
            || info.hIcon.is_null()
        {
            return None;
        }
        let pixels = rasterize(info.hIcon);
        DestroyIcon(info.hIcon);
        let mut output = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(pixels?)
            .write_to(&mut output, ImageFormat::Png)
            .ok()?;
        Some(output.into_inner())
    }
}
fn rasterize(icon: HICON) -> Option<RgbaImage> {
    const SIDE: i32 = 32;
    unsafe {
        let dc = CreateCompatibleDC(null_mut());
        if dc.is_null() {
            return None;
        }
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: SIDE,
                biHeight: -SIDE,
                biPlanes: 1,
                biBitCount: 32,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut ptr = null_mut();
        let bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut ptr, null_mut(), 0);
        if bitmap.is_null() {
            DeleteDC(dc);
            return None;
        }
        let old = SelectObject(dc, bitmap);
        let draw = |background| {
            let count = (SIDE * SIDE * 4) as usize;
            std::ptr::write_bytes(ptr.cast::<u8>(), background, count);
            let ok = DrawIconEx(dc, 0, 0, icon, SIDE, SIDE, 0, null_mut(), DI_NORMAL);
            GdiFlush();
            (ok != 0).then(|| std::slice::from_raw_parts(ptr.cast::<u8>(), count).to_vec())
        };
        let black = draw(0);
        let white = draw(255);
        SelectObject(dc, old);
        DeleteObject(bitmap);
        DeleteDC(dc);
        let black = black?;
        let white = white?;
        let mut rgba = Vec::with_capacity(black.len());
        for (b, w) in black.chunks_exact(4).zip(white.chunks_exact(4)) {
            let alpha = 255 - w[0].saturating_sub(b[0]);
            let channel = |v: u8| {
                if alpha == 0 {
                    0
                } else {
                    (u16::from(v) * 255 / u16::from(alpha)).min(255) as u8
                }
            };
            rgba.extend([channel(b[2]), channel(b[1]), channel(b[0]), alpha]);
        }
        RgbaImage::from_raw(SIDE as u32, SIDE as u32, rgba)
    }
}
