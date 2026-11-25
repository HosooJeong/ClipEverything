use clip_everything::clipboard::wide;
use std::{cell::RefCell, collections::HashMap, mem::size_of, ptr::null_mut};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    UI::WindowsAndMessaging::{ULW_ALPHA, UpdateLayeredWindow},
};

#[derive(Clone, Copy)]
pub struct Contrast(pub bool);
impl Contrast {
    pub fn read() -> Self {
        use windows_sys::Win32::UI::{
            Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW},
            WindowsAndMessaging::{SPI_GETHIGHCONTRAST, SystemParametersInfoW},
        };
        let mut info = HIGHCONTRASTW {
            cbSize: size_of::<HIGHCONTRASTW>() as u32,
            ..Default::default()
        };
        Self(
            unsafe {
                SystemParametersInfoW(
                    SPI_GETHIGHCONTRAST,
                    info.cbSize,
                    (&mut info as *mut HIGHCONTRASTW).cast(),
                    0,
                ) != 0
            } && info.dwFlags & HCF_HIGHCONTRASTON != 0,
        )
    }
    pub fn color(self, normal: u32, system: SYS_COLOR_INDEX) -> u32 {
        if self.0 {
            unsafe { GetSysColor(system) }
        } else {
            normal
        }
    }
}
pub const BG: u32 = 0xFAF7F5;
const BLUE: u32 = 0xC65A17;
const LINE: u32 = 0xE3DBD5;
pub const RAIL_BG: u32 = 0xf7f4f0;
const ICON_INK: u32 = 0x84593d;
const INK: u32 = 0x302620;
const HOVER: u32 = 0xFFF2EA;

pub(super) fn rounded_rect(dc: HDC, r: RECT, bg: u32, border: u32, pen_width: i32, radius: i32) {
    unsafe {
        let brush = CreateSolidBrush(bg);
        let pen = CreatePen(PS_SOLID, pen_width, border);
        let pb = SelectObject(dc, brush);
        let pp = SelectObject(dc, pen);
        RoundRect(dc, r.left, r.top, r.right, r.bottom, radius, radius);
        SelectObject(dc, pp);
        SelectObject(dc, pb);
        DeleteObject(pen);
        DeleteObject(brush);
    }
}

#[derive(Clone, Copy, Eq, PartialEq, Hash)]
struct TileSurface {
    width: u32,
    height: u32,
    radius: i32,
    stroke: i32,
    fill: u32,
    outside: Option<u32>,
    border: u32,
}

impl TileSurface {
    fn rasterize(self) -> image::RgbaImage {
        let half_w = self.width as f32 * 0.5;
        let half_h = self.height as f32 * 0.5;
        let radius = (self.radius as f32).min(half_w).min(half_h);
        let (outside, border, fill) = (
            self.outside.unwrap_or_default().to_le_bytes(),
            self.border.to_le_bytes(),
            self.fill.to_le_bytes(),
        );
        image::RgbaImage::from_fn(self.width, self.height, |x, y| {
            // Signed distance from the pixel centre to the outer rounded rectangle.
            // Offsetting the same contour inward keeps the stroke concentric at each corner.
            let dx = (x as f32 + 0.5 - half_w).abs() - (half_w - radius);
            let dy = (y as f32 + 0.5 - half_h).abs() - (half_h - radius);
            let distance = dx.max(0.0).hypot(dy.max(0.0)) + dx.max(dy).min(0.0) - radius;
            let outer = (0.5 - distance).clamp(0.0, 1.0);
            let inner = (0.5 - distance - self.stroke as f32).clamp(0.0, 1.0);
            let alpha = if self.outside.is_none() { outer } else { 1.0 };
            if alpha == 0.0 {
                return image::Rgba([0, 0, 0, 0]);
            }
            let channel = |i: usize| {
                ((f32::from(outside[i]) * (1.0 - outer)
                    + f32::from(border[i]) * (outer - inner)
                    + f32::from(fill[i]) * inner)
                    / alpha)
                    .round() as u8
            };
            image::Rgba([
                channel(0),
                channel(1),
                channel(2),
                (alpha * 255.0).round() as u8,
            ])
        })
    }

    pub(super) fn paint(self, dc: HDC, rect: RECT) {
        thread_local! {
            static TILES: RefCell<HashMap<TileSurface, Bitmap>> = RefCell::new(HashMap::new());
        }
        TILES.with_borrow_mut(|tiles| {
            if !tiles.contains_key(&self) {
                // All tiles on a board share a size. Release the previous zoom/DPI's GDI handles.
                tiles.retain(|s, _| s.width == self.width && s.height == self.height);
                if let Some(bitmap) = Bitmap::from_rgba(&self.rasterize()) {
                    tiles.insert(self, bitmap);
                }
            }
            if let Some(bitmap) = tiles.get(&self) {
                bitmap.draw(dc, rect.left, rect.top, bitmap.width, bitmap.height);
            } else {
                rounded_rect(
                    dc,
                    rect,
                    self.fill,
                    self.border,
                    self.stroke,
                    self.radius * 2,
                );
            }
        });
    }
}
pub(super) fn rounded_panel(
    width: i32,
    height: i32,
    radius: i32,
    stroke: i32,
    fill: u32,
    border: u32,
) -> Option<Bitmap> {
    Bitmap::from_rgba(
        &TileSurface {
            width: width as u32,
            height: height as u32,
            radius,
            stroke,
            fill,
            border,
            outside: None,
        }
        .rasterize(),
    )
}

pub fn search_field(dc: HDC, r: RECT, dpi: i32, focus: bool, contrast: Contrast) {
    if r.right <= r.left || r.bottom <= r.top {
        return;
    }
    let px = |n: i32| (n * dpi + 48) / 96;
    rounded_rect(
        dc,
        r,
        contrast.color(0xffffff, COLOR_WINDOW),
        contrast.color(if focus { BLUE } else { LINE }, COLOR_WINDOWTEXT),
        px(1),
        px(12),
    );
    let size = px(17);
    Symbol::Search.paint(
        dc,
        r.left + px(8),
        (r.top + r.bottom - size) / 2,
        size,
        contrast.color(if focus { BLUE } else { ICON_INK }, COLOR_WINDOWTEXT),
    );
}

pub struct Font(pub HFONT);
impl Font {
    pub fn new(px: i32) -> Self {
        Self::weighted(px, 400)
    }
    pub(super) fn weighted(px: i32, weight: i32) -> Self {
        Self(unsafe {
            CreateFontW(
                -px,
                0,
                0,
                0,
                weight,
                0,
                0,
                0,
                DEFAULT_CHARSET as u32,
                0,
                0,
                CLEARTYPE_QUALITY as u32,
                0,
                wide("Malgun Gothic").as_ptr(),
            )
        })
    }
}
impl Drop for Font {
    fn drop(&mut self) {
        unsafe {
            DeleteObject(self.0);
        }
    }
}

pub struct Bitmap {
    dc: HDC,
    bitmap: HBITMAP,
    previous: HGDIOBJ,
    pub width: i32,
    pub height: i32,
}
const ALPHA_BLEND: BLENDFUNCTION = BLENDFUNCTION {
    BlendOp: AC_SRC_OVER as u8,
    BlendFlags: 0,
    SourceConstantAlpha: 255,
    AlphaFormat: AC_SRC_ALPHA as u8,
};

impl Bitmap {
    pub fn decode(bytes: &[u8], width: u32, height: u32) -> Option<Self> {
        let img = image::load_from_memory(bytes).ok()?;
        let img = if img.width() > width || img.height() > height {
            img.thumbnail(width, height)
        } else {
            img
        }
        .to_rgba8();
        Self::from_rgba(&img)
    }
    fn from_rgba(img: &image::RgbaImage) -> Option<Self> {
        let (w, h) = (img.width() as i32, img.height() as i32);
        unsafe {
            let dc = CreateCompatibleDC(null_mut());
            if dc.is_null() {
                return None;
            }
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: w,
                    biHeight: -h,
                    biPlanes: 1,
                    biBitCount: 32,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut ptr = null_mut();
            let bmp = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut ptr, null_mut(), 0);
            if bmp.is_null() {
                DeleteDC(dc);
                return None;
            }
            let dst = std::slice::from_raw_parts_mut(ptr.cast::<u8>(), img.len());
            for (src, dst) in img.chunks_exact(4).zip(dst.chunks_exact_mut(4)) {
                let a = src[3] as u16;
                let mult = |v: u8| ((v as u16 * a + 127) / 255) as u8;
                dst.copy_from_slice(&[mult(src[2]), mult(src[1]), mult(src[0]), src[3]]);
            }
            Some(Self {
                dc,
                bitmap: bmp,
                previous: SelectObject(dc, bmp),
                width: w,
                height: h,
            })
        }
    }
    pub fn draw(&self, dc: HDC, x: i32, y: i32, w: i32, h: i32) {
        unsafe {
            AlphaBlend(
                dc,
                x,
                y,
                w,
                h,
                self.dc,
                0,
                0,
                self.width,
                self.height,
                ALPHA_BLEND,
            );
        }
    }
    pub(super) fn update_layer(&self, hwnd: HWND, position: POINT) -> bool {
        unsafe {
            UpdateLayeredWindow(
                hwnd,
                null_mut(),
                &position,
                &SIZE {
                    cx: self.width,
                    cy: self.height,
                },
                self.dc,
                &POINT::default(),
                0,
                &ALPHA_BLEND,
                ULW_ALPHA,
            ) != 0
        }
    }
}
impl Drop for Bitmap {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.dc, self.previous);
            DeleteObject(self.bitmap);
            DeleteDC(self.dc);
        }
    }
}

#[derive(Clone, Copy, Eq, PartialEq, Hash)]
pub enum Symbol {
    Copied,
    Capture,
    Add,
    Minus,
    OpenSidebar,
    CloseSidebar,
    Settings,
    Help,
    Search,
    Previous,
    Next,
}

type IconCache = HashMap<(Symbol, i32, u32), Bitmap>;
thread_local! { static CONTROL_ICONS: RefCell<IconCache>=RefCell::new(HashMap::new()); }

impl Symbol {
    fn mask(self) -> &'static [u8; 96 * 96] {
        match self {
            Self::Search => include_bytes!(concat!(env!("OUT_DIR"), "/search.alpha")),
            Self::Settings => include_bytes!(concat!(env!("OUT_DIR"), "/settings.alpha")),
            Self::Help => include_bytes!(concat!(env!("OUT_DIR"), "/help.alpha")),
            Self::Previous => include_bytes!(concat!(env!("OUT_DIR"), "/previous.alpha")),
            Self::Copied => include_bytes!(concat!(env!("OUT_DIR"), "/copied.alpha")),
            Self::Capture => include_bytes!(concat!(env!("OUT_DIR"), "/capture.alpha")),
            Self::Next => include_bytes!(concat!(env!("OUT_DIR"), "/next.alpha")),
            Self::Add => include_bytes!(concat!(env!("OUT_DIR"), "/increase.alpha")),
            Self::Minus => include_bytes!(concat!(env!("OUT_DIR"), "/decrease.alpha")),
            Self::OpenSidebar => include_bytes!(concat!(env!("OUT_DIR"), "/sidebar-open.alpha")),
            Self::CloseSidebar => include_bytes!(concat!(env!("OUT_DIR"), "/sidebar-close.alpha")),
        }
    }
    pub(super) fn paint(self, dc: HDC, x: i32, y: i32, size: i32, color: u32) {
        if size <= 0 {
            return;
        }
        CONTROL_ICONS.with_borrow_mut(|cache| {
            let key = (self, size, color);
            if let std::collections::hash_map::Entry::Vacant(slot) = cache.entry(key) {
                let mask =
                    image::ImageBuffer::<image::Luma<u8>, _>::from_raw(96, 96, &self.mask()[..])
                        .unwrap();
                let alpha = image::imageops::resize(
                    &mask,
                    size as u32,
                    size as u32,
                    image::imageops::FilterType::Lanczos3,
                );
                let [r, g, b, _] = color.to_le_bytes();
                let pixels = image::RgbaImage::from_fn(alpha.width(), alpha.height(), |x, y| {
                    image::Rgba([r, g, b, alpha.get_pixel(x, y)[0]])
                });
                let Some(bitmap) = Bitmap::from_rgba(&pixels) else {
                    return;
                };
                slot.insert(bitmap);
            }
            cache[&key].draw(dc, x, y, size, size);
        });
    }
}
#[derive(Clone, Default, PartialEq)]
pub struct Face {
    pub text: String,
    pub label_alpha: Option<u8>,
    pub count: Option<String>,
    pub icon: Option<i64>,
    pub preview: Option<PreviewKey>,
    pub active: bool,
    pub solid: bool,
    pub centered: bool,
    pub stacked: bool,
    pub symbol: Option<Symbol>,
    pub fallback: String,
    pub blank: bool,
}
pub type PreviewKey = (i64, i64);
pub struct ButtonState {
    pub drop_here: bool,
    pub copied: bool,
    pub contrast: Contrast,
    pub disabled: bool,
    pub hover: bool,
    pub pressed: bool,
    pub focus: bool,
}
pub fn draw(
    dc: HDC,
    r: RECT,
    face: &Face,
    image: Option<&Bitmap>,
    font: HFONT,
    dpi: i32,
    state: ButtonState,
) {
    // Paint in control-local coordinates for both live HWNDs and offscreen
    // WM_PRINT. In particular, AlphaBlend must sample the icon at the same
    // origin in both passes or its scaled pixels can jump during the handoff.
    unsafe {
        let (w, h) = (r.right - r.left, r.bottom - r.top);
        if w <= 0 || h <= 0 {
            return;
        }
        let mem = CreateCompatibleDC(dc);
        let bitmap = CreateCompatibleBitmap(dc, w, h);
        if !mem.is_null() && !bitmap.is_null() {
            let old = SelectObject(mem, bitmap);
            draw_face(
                mem,
                RECT {
                    left: 0,
                    top: 0,
                    right: w,
                    bottom: h,
                },
                face,
                image,
                font,
                dpi,
                state,
            );
            BitBlt(dc, r.left, r.top, w, h, mem, 0, 0, SRCCOPY);
            SelectObject(mem, old);
        } else {
            draw_face(dc, r, face, image, font, dpi, state);
        }
        if !bitmap.is_null() {
            DeleteObject(bitmap);
        }
        if !mem.is_null() {
            DeleteDC(mem);
        }
    }
}

fn draw_face(
    dc: HDC,
    r: RECT,
    face: &Face,
    image: Option<&Bitmap>,
    font: HFONT,
    dpi: i32,
    state: ButtonState,
) {
    let ButtonState {
        disabled,
        hover,
        pressed,
        focus,
        drop_here,
        copied,
        contrast,
    } = state;
    let px = |n: i32| (n * dpi + 48) / 96;
    let solid = face.solid && face.active;
    let panel = matches!(
        face.symbol,
        Some(Symbol::OpenSidebar | Symbol::CloseSidebar)
    );
    let rail = face.solid || panel || matches!(face.symbol, Some(Symbol::Settings | Symbol::Help));
    let bg = if pressed {
        0xf7e1c9
    } else if drop_here {
        0xffebd8
    } else if solid {
        0xfce8d7
    } else if hover || focus && rail {
        HOVER
    } else if rail {
        RAIL_BG
    } else if face.blank {
        BG
    } else if face.active {
        0xfff5eb
    } else {
        0xffffff
    };
    let border = if rail {
        if focus { 0xe6c7a9 } else { bg }
    } else if focus {
        BLUE
    } else if face.blank {
        0xf0ebe6
    } else {
        LINE
    };
    let highlighted = solid || pressed || drop_here || face.stacked && face.active;
    let bg = contrast.color(
        bg,
        if highlighted {
            COLOR_HIGHLIGHT
        } else {
            COLOR_WINDOW
        },
    );
    let fg_role = if highlighted {
        COLOR_HIGHLIGHTTEXT
    } else {
        COLOR_WINDOWTEXT
    };
    let border = contrast.color(
        border,
        if disabled {
            COLOR_GRAYTEXT
        } else if highlighted && focus {
            COLOR_HIGHLIGHTTEXT
        } else {
            COLOR_WINDOWTEXT
        },
    );
    unsafe {
        if let Some(sign @ (Symbol::Add | Symbol::Minus)) = face.symbol {
            let brush = CreateSolidBrush(contrast.color(BG, COLOR_WINDOW));
            FillRect(dc, &r, brush);
            DeleteObject(brush);
            let size = px(if pressed { 20 } else { 22 });
            let ink = if disabled {
                0xa4988b
            } else if hover || focus || pressed {
                BLUE
            } else {
                ICON_INK
            };
            sign.paint(
                dc,
                (r.left + r.right - size) / 2,
                (r.top + r.bottom - size) / 2,
                size,
                contrast.color(
                    ink,
                    if disabled {
                        COLOR_GRAYTEXT
                    } else {
                        COLOR_WINDOWTEXT
                    },
                ),
            );
            return;
        }
        let base = CreateSolidBrush(contrast.color(if rail { RAIL_BG } else { BG }, COLOR_WINDOW));
        FillRect(dc, &r, base);
        DeleteObject(base);
        let stroke = px(if focus && !rail { 2 } else { 1 });
        if face.stacked {
            let (w, h) = (r.right - r.left, r.bottom - r.top);
            if w <= 0 || h <= 0 {
                return;
            }
            TileSurface {
                width: w as u32,
                height: h as u32,
                radius: px(8),
                stroke,
                fill: bg,
                outside: Some(contrast.color(BG, COLOR_WINDOW)),
                border,
            }
            .paint(dc, r);
        } else {
            rounded_rect(dc, r, bg, border, stroke, px(12));
        }
        if drop_here {
            let pad = px(4);
            let brush = LOGBRUSH {
                lbStyle: BS_SOLID,
                lbColor: contrast.color(BLUE, fg_role),
                lbHatch: 0,
            };
            let pen = ExtCreatePen(
                (PS_GEOMETRIC | PS_DOT | PS_ENDCAP_ROUND) as u32,
                px(1).max(1) as u32,
                &brush,
                0,
                std::ptr::null(),
            );
            let oldpen = SelectObject(dc, pen);
            let oldbrush = SelectObject(dc, GetStockObject(NULL_BRUSH));
            SetBkMode(dc, TRANSPARENT as i32);
            RoundRect(
                dc,
                r.left + pad,
                r.top + pad,
                r.right - pad,
                r.bottom - pad,
                px(10),
                px(10),
            );
            SelectObject(dc, oldbrush);
            SelectObject(dc, oldpen);
            DeleteObject(pen);
        }
        if solid {
            let marker = RECT {
                left: r.left + px(1),
                top: (r.top + r.bottom - px(16)) / 2,
                right: r.left + px(4),
                bottom: (r.top + r.bottom - px(16)) / 2 + px(16),
            };
            let marker_color = contrast.color(BLUE, fg_role);
            rounded_rect(dc, marker, marker_color, marker_color, 1, px(3));
        }
        SetBkMode(dc, TRANSPARENT as i32);
        let f = SelectObject(dc, font);
        let ink = if disabled {
            0xa4988b
        } else if solid {
            0x8e490f
        } else {
            INK
        };
        let ink = contrast.color(ink, if disabled { COLOR_GRAYTEXT } else { fg_role });
        let text_color = if let Some(alpha) = face.label_alpha.filter(|_| !contrast.0) {
            let fg = ink.to_le_bytes();
            let base = bg.to_le_bytes();
            let a = u32::from(alpha);
            let channel = |i: usize| {
                ((u32::from(fg[i]) * a + u32::from(base[i]) * (255 - a) + 127) / 255) as u8
            };
            u32::from_le_bytes([channel(0), channel(1), channel(2), 0])
        } else {
            ink
        };
        SetTextColor(dc, text_color);
        let mut text = r;
        let icon_size = px(if panel {
            22
        } else if rail {
            20
        } else {
            18
        })
        .min(r.right - r.left)
        .min(r.bottom - r.top);
        let x = if rail && face.centered {
            r.left + ((r.right - r.left).min(px(super::grid::COMPACT_SIDEBAR - 24)) - icon_size) / 2
        } else if face.stacked || face.centered {
            (r.left + r.right - icon_size) / 2
        } else {
            r.left + px(if rail { 12 } else { 7 })
        };
        let y = if face.stacked {
            r.top + px(7)
        } else {
            (r.top + r.bottom - icon_size) / 2
        };
        let mut artwork_bottom =
            (copied || face.symbol.is_some() || image.is_some() || !face.fallback.is_empty())
                .then_some(y + icon_size);
        if copied {
            Symbol::Copied.paint(dc, x, y, icon_size, contrast.color(0x507423, fg_role));
        } else if let Some(symbol) = face.symbol {
            symbol.paint(
                dc,
                x,
                y,
                icon_size,
                if contrast.0 {
                    ink
                } else if disabled {
                    0xa4988b
                } else if hover || focus || pressed {
                    BLUE
                } else {
                    ICON_INK
                },
            );
        } else if let Some(img) = image {
            if face.preview.is_some() {
                let mut metrics = TEXTMETRICW::default();
                GetTextMetricsW(dc, &mut metrics);
                let available_w = (r.right - r.left - px(10)).max(1);
                let available_h = (r.bottom - y - metrics.tmHeight - px(5)).max(1);
                let scale = (f64::from(available_w) / f64::from(img.width))
                    .min(f64::from(available_h) / f64::from(img.height))
                    .min(1.0);
                let w = ((f64::from(img.width) * scale) as i32).max(1);
                let h = ((f64::from(img.height) * scale) as i32).max(1);
                img.draw(dc, (r.left + r.right - w) / 2, y, w, h);
                artwork_bottom = Some(y + h);
            } else {
                img.draw(dc, x, y, icon_size, icon_size);
            }
        } else if !face.fallback.is_empty() {
            let mut box_ = RECT {
                left: x,
                top: y,
                right: x + icon_size,
                bottom: y + icon_size,
            };
            let brush = CreateSolidBrush(contrast.color(
                0xF5E8D9,
                if highlighted {
                    COLOR_HIGHLIGHT
                } else {
                    COLOR_WINDOW
                },
            ));
            FillRect(dc, &box_, brush);
            DeleteObject(brush);
            DrawTextW(
                dc,
                wide(&face.fallback).as_ptr(),
                -1,
                &mut box_,
                DT_SINGLELINE | DT_CENTER | DT_VCENTER | DT_NOPREFIX,
            );
        }
        if face.stacked {
            text.left += px(4);
            text.right -= px(4);
            text.bottom -= px(2);
            let label = wide(&face.text);
            let flags = DT_CENTER | DT_WORDBREAK | DT_EDITCONTROL | DT_END_ELLIPSIS | DT_NOPREFIX;
            if let Some(bottom) = artwork_bottom {
                text.top = bottom + px(3);
            } else {
                text.top += px(2);
                let mut bounds = text;
                DrawTextW(dc, label.as_ptr(), -1, &mut bounds, flags | DT_CALCRECT);
                text.top += (text.bottom - bounds.bottom).max(0) / 2;
            }
            DrawTextW(dc, label.as_ptr(), -1, &mut text, flags);
        } else {
            text.left += px(
                if face.icon.is_some() || face.symbol.is_some() || !face.fallback.is_empty() {
                    if rail { 39 } else { 31 }
                } else {
                    5
                },
            );
            text.right -= px(if rail { 10 } else { 5 });
            if let Some(count) = &face.count {
                let count = wide(count);
                let mut bounds = RECT::default();
                DrawTextW(
                    dc,
                    count.as_ptr(),
                    -1,
                    &mut bounds,
                    DT_CALCRECT | DT_SINGLELINE,
                );
                let mut area = text;
                area.left = (area.right - bounds.right).max(area.left);
                DrawTextW(
                    dc,
                    count.as_ptr(),
                    -1,
                    &mut area,
                    DT_RIGHT | DT_VCENTER | DT_SINGLELINE,
                );
                text.right = area.left - px(5);
            }
            DrawTextW(
                dc,
                wide(&face.text).as_ptr(),
                -1,
                &mut text,
                DT_SINGLELINE
                    | DT_VCENTER
                    | DT_END_ELLIPSIS
                    | DT_NOPREFIX
                    | if face.centered { DT_CENTER } else { 0 },
            );
        }
        SelectObject(dc, f);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn popup_corners_have_fractional_alpha_at_every_scale() {
        for dpi in [96, 120, 144, 192] {
            let px = |n| (n * dpi + 48) / 96;
            let img = TileSurface {
                width: px(174) as u32,
                height: px(116) as u32,
                radius: px(11),
                stroke: px(1),
                fill: 0xffffff,
                outside: None,
                border: 0xe9e1d9,
            }
            .rasterize();
            assert_eq!(*img.get_pixel(0, 0), image::Rgba([0, 0, 0, 0]));
            assert!(img.pixels().any(|p| p[3] > 0 && p[3] < 255));
            let inset = px(5) as u32;
            assert_eq!(
                *img.get_pixel(inset, inset),
                image::Rgba([255, 255, 255, 255])
            );
        }
    }

    #[test]
    fn success_glyph_replaces_the_icon_and_contrast_focus_uses_system_colors() {
        let canvas = Bitmap::from_rgba(&image::RgbaImage::new(80, 68)).unwrap();
        let bounds = RECT {
            left: 0,
            top: 0,
            right: 80,
            bottom: 68,
        };
        let font = Font::new(13);
        let face = Face {
            stacked: true,
            active: true,
            text: "Example".into(),
            fallback: "E".into(),
            ..Default::default()
        };
        let render = |copied, contrast| {
            draw(
                canvas.dc,
                bounds,
                &face,
                None,
                font.0,
                96,
                ButtonState {
                    focus: true,
                    copied,
                    contrast: Contrast(contrast),
                    hover: false,
                    pressed: false,
                    disabled: false,
                    drop_here: false,
                },
            );
        };
        let success_pixels = || {
            (7..25)
                .flat_map(|y| (31..49).map(move |x| (x, y)))
                .filter(|&(x, y)| unsafe { GetPixel(canvas.dc, x, y) } == 0x507423)
                .count()
        };
        render(false, false);
        assert_eq!(success_pixels(), 0);
        render(true, false);
        assert!(success_pixels() > 3);
        render(false, true);
        unsafe {
            assert_eq!(GetPixel(canvas.dc, 40, 1), GetSysColor(COLOR_HIGHLIGHTTEXT));
            assert_eq!(GetPixel(canvas.dc, 40, 4), GetSysColor(COLOR_HIGHLIGHT));
        }
    }
}
