use std::{mem::size_of, ptr::null_mut};
use windows_sys::Win32::{Foundation::*, Graphics::Gdi::*, UI::WindowsAndMessaging::*};

// Logical client dimensions. Even with paging, the toolbar keeps a usable search field.
pub const SIDEBAR: i32 = 214;
pub const COMPACT_SIDEBAR: i32 = 76;
pub const fn sidebar_width(collapsed: bool) -> i32 {
    if collapsed { COMPACT_SIDEBAR } else { SIDEBAR }
}
pub const fn minimum_width(collapsed: bool) -> i32 {
    sidebar_width(collapsed) + 8 + 120 + 2 * 28 + 2 * 26 + 4 * 4 + 8
}
pub const MIN_WIDTH: i32 = minimum_width(false);
pub const MIN_HEIGHT: i32 = 12 + 4 * 44 + 36 + 12;
pub const LEVELS: i32 = 10;
#[derive(Clone, Copy, PartialEq)]
pub struct Shape {
    pub cols: i32,
    pub rows: i32,
    pub width: i32,
    pub height: i32,
    pub gap: i32,
    pub left: i32,
    pub top: i32,
    pub edge: i32,
    pub min_width: i32,
    pub min_height: i32,
}
impl Shape {
    pub fn new(cols: i32, rows: i32, level: i32, dpi: i32, collapsed: bool) -> Self {
        let px = |n: i32| (n * dpi + 48) / 96;
        let width = 58 + (level.clamp(1, LEVELS) - 1) * 5;
        Self {
            cols,
            rows,
            width: px(width),
            height: px(width - 10),
            gap: px(5),
            left: px(sidebar_width(collapsed) + 8),
            top: px(46),
            edge: px(40),
            min_width: px(minimum_width(collapsed)),
            min_height: px(MIN_HEIGHT),
        }
    }
    fn content(self) -> (i32, i32) {
        (
            self.left + self.cols * (self.width + self.gap) - self.gap + self.edge,
            self.top + self.rows * (self.height + self.gap) - self.gap + self.edge,
        )
    }
}

// The frame is a viewport. Switching boards adopts its existing size;
// only changes to the current board's geometry request a resize.
#[derive(Default)]
pub struct WindowFit {
    last: Option<(String, Shape)>,
}
impl WindowFit {
    fn desired_size(&mut self, app: &str, shape: Shape, client: (i32, i32)) -> Option<(i32, i32)> {
        let content = shape.content();
        if let Some((name, previous)) = &mut self.last {
            let old = *previous;
            *previous = shape;
            if name != app {
                name.clear();
                name.push_str(app);
                return None;
            }
            if old.width == shape.width && old.height == shape.height {
                if old.cols == shape.cols && old.rows == shape.rows {
                    return None;
                }
                return Some((
                    client.0 + (shape.cols - old.cols) * (shape.width + shape.gap),
                    client.1 + (shape.rows - old.rows) * (shape.height + shape.gap),
                ));
            }
        } else {
            self.last = Some((app.into(), shape));
        }
        Some(content)
    }
    pub fn fit(&mut self, h: HWND, shape: Shape, app: &str) {
        unsafe {
            let mut client = RECT::default();
            GetClientRect(h, &mut client);
            let Some((w, height)) = self.desired_size(app, shape, (client.right, client.bottom))
            else {
                return;
            };
            if IsIconic(h) != 0 || IsZoomed(h) != 0 {
                return;
            }
            let mut frame = RECT::default();
            if GetWindowRect(h, &mut frame) == 0 {
                return;
            }
            frame.right += w.max(shape.min_width) - client.right;
            frame.bottom += height.max(shape.min_height) - client.bottom;
            position_frame(h, frame, SWP_NOCOPYBITS);
        }
    }
}

fn position_frame(h: HWND, frame: RECT, flags: SET_WINDOW_POS_FLAGS) {
    unsafe {
        let mut monitor = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if GetMonitorInfoW(MonitorFromWindow(h, MONITOR_DEFAULTTONEAREST), &mut monitor) == 0 {
            return;
        }
        let work = monitor.rcWork;
        let w = (frame.right - frame.left).clamp(1, work.right - work.left);
        let height = (frame.bottom - frame.top).clamp(1, work.bottom - work.top);
        let x = frame.left.clamp(work.left, work.right - w);
        let y = frame.top.clamp(work.top, work.bottom - height);
        SetWindowPos(
            h,
            null_mut(),
            x,
            y,
            w,
            height,
            SWP_NOACTIVATE | SWP_NOZORDER | SWP_NOOWNERZORDER | flags,
        );
    }
}

pub const SLIDE_TIMER: usize = 0xCE09;
pub struct SidebarSlide {
    start: i32,
    end: i32,
    pub width: i32,
    last_tick: std::time::Instant,
    elapsed: std::time::Duration,
    duration: std::time::Duration,
    dpi: i32,
}
impl SidebarSlide {
    pub fn start(h: HWND, width: i32, collapsed: bool, dpi: i32) -> Self {
        unsafe {
            let end = (sidebar_width(collapsed) * dpi + 48) / 96;
            let mut animate = 0i32;
            let enabled = SystemParametersInfoW(
                SPI_GETCLIENTAREAANIMATION,
                0,
                (&mut animate as *mut i32).cast(),
                0,
            ) != 0
                && animate != 0
                && IsWindowVisible(h) != 0
                && width != end;
            let duration = if enabled && SetTimer(h, SLIDE_TIMER, 15, None) != 0 {
                // A reversal covers only the remaining distance.
                let distance = (end - width).abs() as f64
                    / ((SIDEBAR - COMPACT_SIDEBAR) as f64 * dpi as f64 / 96.);
                std::time::Duration::from_secs_f64(0.20 * distance.clamp(0.3, 1.))
            } else {
                std::time::Duration::ZERO
            };
            Self {
                start: width,
                end,
                width,
                last_tick: std::time::Instant::now(),
                elapsed: std::time::Duration::ZERO,
                duration,
                dpi,
            }
        }
    }
    pub fn animated(&self) -> bool {
        !self.duration.is_zero()
    }
    pub fn advance(&mut self, h: HWND, dpi: i32, collapsed: bool) -> bool {
        unsafe {
            let now = std::time::Instant::now();
            // A delayed paint must not skip most of the transition in one frame.
            self.elapsed += (now - self.last_tick).min(std::time::Duration::from_millis(32));
            self.last_tick = now;
            let done = self.elapsed >= self.duration
                || dpi != self.dpi
                || IsWindowVisible(h) == 0
                || IsIconic(h) != 0;
            let next = if done {
                (sidebar_width(collapsed) * dpi + 48) / 96
            } else {
                let t = (self.elapsed.as_secs_f64() / self.duration.as_secs_f64()).min(1.);
                let eased = t * t * (3. - 2. * t);
                self.start + ((self.end - self.start) as f64 * eased).round() as i32
            };
            let previous = (i64::from(self.width) * i64::from(dpi) + i64::from(self.dpi) / 2)
                / i64::from(self.dpi);
            let delta = next - previous as i32;
            self.width = next;
            if delta != 0 && IsZoomed(h) == 0 && IsIconic(h) == 0 {
                let mut frame = RECT::default();
                if GetWindowRect(h, &mut frame) != 0 {
                    frame.left -= delta;
                    position_frame(h, frame, SWP_NOREDRAW);
                }
            }
            if done {
                KillTimer(h, SLIDE_TIMER);
            }
            done
        }
    }
}
// A page is a rectangular part of the board, so a wide board keeps its stored row/column coordinates.
#[derive(Default, Clone, Copy)]
pub struct Pages {
    pub cols: i32,
    pub rows: i32,
    pub across: i32,
    pub count: i32,
}
impl Pages {
    pub fn new(shape: Shape, w: i32, h: i32, rows: i32) -> Self {
        let cols = ((w - shape.left - shape.edge + shape.gap) / (shape.width + shape.gap))
            .clamp(1, shape.cols);
        let visible =
            ((h - shape.top - shape.edge + shape.gap) / (shape.height + shape.gap)).max(1);
        let across = (shape.cols + cols - 1) / cols;
        Self {
            cols,
            rows: visible,
            across,
            count: across * ((rows.max(1) + visible - 1) / visible),
        }
    }
    pub fn origin(self, page: i32) -> (i32, i32) {
        (
            page % self.across * self.cols,
            page / self.across * self.rows,
        )
    }
    pub fn containing(self, col: i32, row: i32) -> i32 {
        row / self.rows * self.across + col / self.cols
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn minimum_fits_search_controls_and_a_complete_tile_at_every_size() {
        for dpi in [96, 120, 144, 192] {
            let px = |n: i32| (n * dpi + 48) / 96;
            for level in 1..=LEVELS {
                for collapsed in [false, true] {
                    let shape = Shape::new(4, 3, level, dpi, collapsed);
                    let controls = px(8) + 2 * (px(26) + px(4)) + 2 * (px(28) + px(4));
                    assert!(shape.min_width - controls - shape.left >= px(112));
                    assert!(shape.top + shape.height + shape.edge <= shape.min_height);
                }
            }
        }
    }
    #[test]
    fn switching_programs_keeps_manual_viewport_until_an_explicit_edit() {
        let mut fit = WindowFit::default();
        let small = Shape::new(2, 3, 5, 96, false);
        let large = Shape::new(12, 16, 5, 96, false);
        let manual = (620, 380);
        assert_eq!(fit.desired_size("A", small, manual), Some(small.content()));
        assert_eq!(fit.desired_size("B", large, manual), None);
        assert_eq!(fit.desired_size("B", large, manual), None);
        assert_eq!(
            fit.desired_size("B", Shape { rows: 17, ..large }, manual),
            Some((620, 380 + large.height + large.gap))
        );
        assert_eq!(fit.desired_size("A", small, manual), None);
    }
    #[test]
    fn board_growth_keeps_tiles_fixed() {
        for dpi in [96, 120, 144] {
            for level in 1..=LEVELS {
                let s = Shape::new(4, 3, level, dpi, false);
                let col = Shape { cols: 5, ..s };
                let row = Shape { rows: 4, ..s };
                assert_eq!(col.content().0 - s.content().0, s.width + s.gap);
                assert_eq!(row.content().1 - s.content().1, s.height + s.gap);
                let (w, h) = s.content();
                assert_eq!(Pages::new(s, w, h, 3).count, 1);
                let p = Pages::new(col, w, h, 4);
                assert_eq!(p.count, 4);
                assert_eq!(p.origin(p.containing(4, 3)), (4, 3));
            }
        }
    }
}
