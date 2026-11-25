use super::paint::{self, Bitmap, Font};
use anyhow::{Result, ensure};
use clip_everything::clipboard::wide;
use std::{
    cell::RefCell,
    mem::size_of,
    ptr::{null, null_mut},
    rc::Rc,
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::LibraryLoader::GetModuleHandleW,
    UI::{Shell::*, WindowsAndMessaging::*},
};

pub struct Content {
    pub title: String,
    pub description: String,
    pub icon: Option<Rc<Bitmap>>,
    pub image: Option<Vec<u8>>,
}

pub fn excerpt(title: &str, preview: &str) -> String {
    let preview = preview.trim();
    let body = if preview
        .lines()
        .next()
        .is_some_and(|first| first.trim() == title.trim())
    {
        preview.split_once('\n').map_or("", |(_, rest)| rest.trim())
    } else {
        preview
    };
    let mut chars = body.chars();
    let mut result: String = chars.by_ref().take(240).collect();
    if chars.next().is_some() {
        result.push('…');
    }
    result
}

struct TextBlock {
    value: Vec<u16>,
    font: Font,
    bounds: RECT,
}
impl TextBlock {
    fn new(value: &str, font: Font) -> Self {
        Self {
            value: wide(value),
            font,
            bounds: RECT::default(),
        }
    }
    fn measure(&self, dc: HDC, width: i32, lines: i32) -> SIZE {
        unsafe {
            let previous = SelectObject(dc, self.font.0);
            let mut metrics = TEXTMETRICW::default();
            GetTextMetricsW(dc, &mut metrics);
            let mut r = RECT {
                right: width,
                ..Default::default()
            };
            DrawTextW(
                dc,
                self.value.as_ptr(),
                -1,
                &mut r,
                DT_CALCRECT | DT_NOPREFIX | DT_WORDBREAK,
            );
            SelectObject(dc, previous);
            SIZE {
                cx: r.right.min(width),
                cy: r.bottom.min(lines * metrics.tmHeight),
            }
        }
    }
    fn draw(&self, dc: HDC, color: u32) {
        unsafe {
            let previous = SelectObject(dc, self.font.0);
            SetTextColor(dc, color);
            let mut r = self.bounds;
            DrawTextW(
                dc,
                self.value.as_ptr(),
                -1,
                &mut r,
                DT_WORDBREAK | DT_NOPREFIX | DT_END_ELLIPSIS | DT_EDITCONTROL,
            );
            SelectObject(dc, previous);
        }
    }
}

struct Card {
    title: TextBlock,
    body: Option<TextBlock>,
    icon: Option<Rc<Bitmap>>,
    image: Option<Bitmap>,
    dpi: i32,
    width: i32,
    height: i32,
    header_height: i32,
    image_top: i32,
}
impl Card {
    fn new(dc: HDC, content: Content, dpi: i32, work: RECT) -> Self {
        let px = |n: i32| (n * dpi + 48) / 96;
        let pad = px(10);
        let badge = px(24);
        let inset = if content.icon.is_some() {
            badge + px(8)
        } else {
            0
        };
        let limit = (px(320).min(work.right - work.left) - 2 * pad).max(inset + 1);
        let mut title = TextBlock::new(&content.title, Font::weighted(px(13), 600));
        let title_size = title.measure(dc, limit - inset, 2);
        let header_height = title_size.cy.max(if content.icon.is_some() {
            badge
        } else {
            px(18)
        });
        let mut body = (!content.description.is_empty())
            .then(|| TextBlock::new(&content.description, Font::new(px(13))));
        let body_size = body
            .as_ref()
            .map_or(SIZE::default(), |text| text.measure(dc, limit, 4));
        let image = content.image.and_then(|bytes| {
            Bitmap::decode(&bytes, (limit - px(4)).max(1) as u32, px(144) as u32)
        });
        let inner = (inset + title_size.cx)
            .max(body_size.cx)
            .max(image.as_ref().map_or(0, |img| img.width + px(4)))
            .min(limit);
        let width = 2 * pad + inner;
        let mut y = pad + header_height;
        title.bounds = RECT {
            left: pad + inset,
            top: pad + (header_height - title_size.cy) / 2,
            right: width - pad,
            bottom: pad + (header_height + title_size.cy) / 2,
        };
        if let Some(text) = &mut body {
            y += px(6);
            text.bounds = RECT {
                left: pad,
                top: y,
                right: width - pad,
                bottom: y + body_size.cy,
            };
            y += body_size.cy;
        }
        let image_top = y + px(8);
        if let Some(img) = &image {
            y = image_top + img.height + px(4);
        }
        Self {
            title,
            body,
            icon: content.icon,
            image,
            dpi,
            width,
            height: y + pad,
            header_height,
            image_top,
        }
    }
    fn draw(&self, dc: HDC) {
        unsafe {
            let px = |n: i32| (n * self.dpi + 48) / 96;
            let pad = px(10);
            let bounds = RECT {
                right: self.width,
                bottom: self.height,
                ..Default::default()
            };
            paint::rounded_rect(dc, bounds, 0xffffff, 0xded4c8, px(1), px(14));
            SetBkMode(dc, TRANSPARENT as i32);
            if let Some(icon) = &self.icon {
                let top = pad + (self.header_height - px(24)) / 2;
                let badge = RECT {
                    left: pad,
                    top,
                    right: pad + px(24),
                    bottom: top + px(24),
                };
                paint::rounded_rect(dc, badge, 0xfbf4ec, 0xfbf4ec, 1, px(8));
                icon.draw(dc, pad + px(4), top + px(4), px(16), px(16));
            }
            self.title.draw(dc, 0x35291d);
            if let Some(body) = &self.body {
                body.draw(dc, 0x705f50);
            }
            if let Some(img) = &self.image {
                let frame = RECT {
                    left: pad,
                    top: self.image_top,
                    right: self.width - pad,
                    bottom: self.image_top + img.height + px(4),
                };
                paint::rounded_rect(dc, frame, 0xfcfaf8, 0xeee6dd, 1, px(6));
                img.draw(
                    dc,
                    (self.width - img.width) / 2,
                    self.image_top + px(2),
                    img.width,
                    img.height,
                );
            }
        }
    }
}

fn origin(anchor: RECT, work: RECT, w: i32, h: i32, gap: i32, p: POINT) -> POINT {
    let x = anchor
        .left
        .clamp(work.left, (work.right - w).max(work.left));
    if anchor.bottom + gap + h <= work.bottom {
        return POINT {
            x,
            y: anchor.bottom + gap,
        };
    }
    if anchor.top - gap - h >= work.top {
        return POINT {
            x,
            y: anchor.top - gap - h,
        };
    }
    let y = anchor.top.clamp(work.top, (work.bottom - h).max(work.top));
    if anchor.right + gap + w <= work.right {
        return POINT {
            x: anchor.right + gap,
            y,
        };
    }
    if anchor.left - gap - w >= work.left {
        return POINT {
            x: anchor.left - gap - w,
            y,
        };
    }
    let y = if p.y + h + gap < work.bottom {
        p.y + gap
    } else {
        (p.y - gap - h).max(work.top)
    };
    POINT { x, y }
}

pub struct Preview {
    pub hwnd: HWND,
    card: Box<RefCell<Option<Card>>>,
}
impl Preview {
    pub fn new(owner: HWND) -> Result<Self> {
        unsafe {
            let hwnd = CreateWindowExW(
                WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_TRANSPARENT,
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
            ensure!(!hwnd.is_null(), "미리보기 창을 만들 수 없습니다.");
            let tip = Self {
                hwnd,
                card: Box::default(),
            };
            ensure!(
                SetWindowSubclass(
                    hwnd,
                    Some(procedure),
                    1,
                    tip.card.as_ref() as *const _ as usize
                ) != 0,
                "미리보기 창을 연결할 수 없습니다."
            );
            Ok(tip)
        }
    }
    pub fn show(&self, cell: HWND, cursor: POINT, dpi: i32, content: Content) {
        unsafe {
            let mut monitor = MONITORINFO {
                cbSize: size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            let mut anchor = RECT::default();
            if GetMonitorInfoW(
                MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST),
                &mut monitor,
            ) == 0
                || GetWindowRect(cell, &mut anchor) == 0
            {
                return;
            }
            let dc = GetDC(self.hwnd);
            if dc.is_null() {
                return;
            }
            let card = Card::new(dc, content, dpi, monitor.rcWork);
            ReleaseDC(self.hwnd, dc);
            let (w, h) = (card.width, card.height);
            let pos = origin(anchor, monitor.rcWork, w, h, (6 * dpi + 48) / 96, cursor);
            SetWindowTextW(self.hwnd, card.title.value.as_ptr());
            *self.card.borrow_mut() = Some(card);
            let radius = (14 * dpi + 48) / 96;
            let region = CreateRoundRectRgn(0, 0, w + 1, h + 1, radius, radius);
            if !region.is_null() && SetWindowRgn(self.hwnd, region, 0) == 0 {
                DeleteObject(region);
            }
            SetWindowPos(
                self.hwnd,
                HWND_TOPMOST,
                pos.x,
                pos.y,
                w,
                h,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
            InvalidateRect(self.hwnd, null(), 0);
            UpdateWindow(self.hwnd);
        }
    }
}
impl Drop for Preview {
    fn drop(&mut self) {
        unsafe {
            RemoveWindowSubclass(self.hwnd, Some(procedure), 1);
            DestroyWindow(self.hwnd);
        }
    }
}
unsafe extern "system" fn procedure(
    h: HWND,
    msg: u32,
    w: WPARAM,
    l: LPARAM,
    _: usize,
    data: usize,
) -> LRESULT {
    unsafe {
        match msg {
            WM_NCHITTEST => HTTRANSPARENT as isize,
            WM_MOUSEACTIVATE => MA_NOACTIVATE as isize,
            WM_ERASEBKGND => 1,
            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                let dc = BeginPaint(h, &mut ps);
                let state = &*(data as *const RefCell<Option<Card>>);
                if let Some(card) = state.borrow().as_ref() {
                    card.draw(dc);
                }
                EndPaint(h, &ps);
                0
            }
            _ => DefSubclassProc(h, msg, w, l),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn descriptions_skip_repeated_titles() {
        assert_eq!(excerpt("build", "build"), "");
        assert_eq!(excerpt("build", "build\r\noutput"), "output");
        assert!(excerpt("x", &"한".repeat(241)).ends_with('…'));
    }
    #[test]
    fn card_flips_above_a_tile_at_monitor_bottom() {
        let work = RECT {
            left: -800,
            top: 0,
            right: 0,
            bottom: 600,
        };
        let tile = RECT {
            left: -72,
            top: 550,
            right: -8,
            bottom: 598,
        };
        let p = origin(tile, work, 200, 85, 6, POINT { x: -35, y: 575 });
        assert_eq!((p.x, p.y), (-200, 459));
    }
}
