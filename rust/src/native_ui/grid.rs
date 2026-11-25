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
pub const GRID_TOP: i32 = 46;
#[derive(Clone, Copy)]
pub struct Shape {
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
    pub fn new(level: i32, dpi: i32, collapsed: bool) -> Self {
        let px = |n: i32| (n * dpi + 48) / 96;
        let width = 58 + (level.clamp(1, LEVELS) - 1) * 5;
        Self {
            width: px(width),
            height: px(width - 10),
            gap: px(5),
            left: px(sidebar_width(collapsed)) + px(8),
            top: px(GRID_TOP),
            edge: px(8),
            min_width: px(minimum_width(collapsed)),
            min_height: px(MIN_HEIGHT),
        }
    }
    fn content(self, cols: i32, rows: i32) -> (i32, i32) {
        (
            self.left + cols * (self.width + self.gap) - self.gap + self.edge,
            self.top + rows * (self.height + self.gap) - self.gap + self.edge,
        )
    }
    pub fn capacity(self, w: i32, h: i32) -> (i32, i32) {
        (
            ((w - self.left - self.edge + self.gap) / (self.width + self.gap)).max(1),
            ((h - self.top - self.edge + self.gap) / (self.height + self.gap)).max(1),
        )
    }
    pub fn snap_frame(self, frame: &mut RECT, side: u32, nonclient: (i32, i32)) {
        let (first_w, first_h) = self.content(1, 1);
        let (nc_w, nc_h) = nonclient;
        let width = snap_length(
            frame.right - frame.left - nc_w,
            first_w,
            self.width + self.gap,
            self.min_width,
        ) + nc_w;
        let height = snap_length(
            frame.bottom - frame.top - nc_h,
            first_h,
            self.height + self.gap,
            self.min_height,
        ) + nc_h;
        match side {
            WMSZ_LEFT | WMSZ_TOPLEFT | WMSZ_BOTTOMLEFT => frame.left = frame.right - width,
            WMSZ_RIGHT | WMSZ_TOPRIGHT | WMSZ_BOTTOMRIGHT => frame.right = frame.left + width,
            _ => {}
        }
        match side {
            WMSZ_TOP | WMSZ_TOPLEFT | WMSZ_TOPRIGHT => frame.top = frame.bottom - height,
            WMSZ_BOTTOM | WMSZ_BOTTOMLEFT | WMSZ_BOTTOMRIGHT => frame.bottom = frame.top + height,
            _ => {}
        }
    }
    pub fn fit_initial_window(self, h: HWND) {
        unsafe {
            let mut client = RECT::default();
            GetClientRect(h, &mut client);
            if IsIconic(h) != 0 || IsZoomed(h) != 0 {
                return;
            }
            let (w, height) = self.content(4, 3);
            let mut frame = RECT::default();
            if GetWindowRect(h, &mut frame) == 0 {
                return;
            }
            frame.right += w.max(self.min_width) - client.right;
            frame.bottom += height.max(self.min_height) - client.bottom;
            position_frame(h, frame, SWP_NOCOPYBITS);
        }
    }
}

fn snap_length(proposed: i32, first: i32, pitch: i32, minimum: i32) -> i32 {
    let minimum_steps = ((minimum - first).max(0) + pitch - 1) / pitch;
    let nearest_steps = (proposed - first + pitch / 2) / pitch;
    first + nearest_steps.max(minimum_steps) * pitch
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
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Pages {
    pub cols: i32,
    pub rows: i32,
    pub across: i32,
    pub count: i32,
}
impl Pages {
    pub fn new((cols, rows): (i32, i32), (board_cols, board_rows): (i32, i32)) -> Self {
        let across = (board_cols + cols - 1) / cols;
        Self {
            cols,
            rows,
            across,
            count: across * ((board_rows.max(1) + rows - 1) / rows),
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
                    let shape = Shape::new(level, dpi, collapsed);
                    let controls = px(8) + 2 * (px(26) + px(4)) + 2 * (px(28) + px(4));
                    assert!(shape.min_width - controls - shape.left >= px(112));
                    assert!(shape.top + shape.height + shape.edge <= shape.min_height);
                }
            }
        }
    }
    #[test]
    fn resizing_reveals_complete_cells_without_changing_their_coordinates() {
        for dpi in [96, 120, 144] {
            for level in 1..=LEVELS {
                let s = Shape::new(level, dpi, false);
                let (w, h) = s.content(4, 3);
                assert_eq!(s.capacity(w, h), (4, 3));
                assert_eq!(s.capacity(w - 1, h - 1), (3, 2));
                let pages = Pages::new(s.capacity(w, h), (5, 4));
                assert_eq!(pages.count, 4);
                assert_eq!(pages.origin(pages.containing(4, 3)), (4, 3));
                let expanded = s.content(5, 4);
                assert_eq!(
                    Pages::new(s.capacity(expanded.0, expanded.1), (5, 4)).count,
                    1
                );
            }
        }
    }
    #[test]
    fn snapping_uses_the_rendered_pitch_and_respects_the_minimum() {
        for dpi in [96, 120, 144, 192] {
            for level in 1..=LEVELS {
                for collapsed in [false, true] {
                    let shape = Shape::new(level, dpi, collapsed);
                    let nc = (dpi / 6, dpi / 2);
                    let (w, h) = shape.content(5, 4);
                    let mut frame = RECT {
                        left: 10,
                        top: 20,
                        right: 10 + w + nc.0 + (shape.width + shape.gap) / 3,
                        bottom: 20 + h + nc.1 - (shape.height + shape.gap) / 3,
                    };
                    shape.snap_frame(&mut frame, WMSZ_BOTTOMRIGHT, nc);
                    assert_eq!((frame.right, frame.bottom), (10 + w + nc.0, 20 + h + nc.1));
                    frame.right = 11;
                    frame.bottom = 21;
                    shape.snap_frame(&mut frame, WMSZ_BOTTOMRIGHT, nc);
                    let width = frame.right - frame.left - nc.0;
                    let height = frame.bottom - frame.top - nc.1;
                    let (cols, rows) = shape.capacity(width, height);
                    assert_eq!((width, height), shape.content(cols, rows));
                    assert!(
                        width >= shape.min_width
                            && width - shape.width - shape.gap < shape.min_width
                    );
                    assert!(
                        height >= shape.min_height
                            && height - shape.height - shape.gap < shape.min_height
                    );
                }
            }
        }
    }
    #[test]
    fn snapping_preserves_the_opposite_edges_and_the_other_axis() {
        let shape = Shape::new(5, 96, false);
        let (w, h) = shape.content(5, 4);
        let nc = (16, 39);
        let original = RECT {
            left: 100,
            top: 200,
            right: 100 + w + nc.0 + 11,
            bottom: 200 + h + nc.1 + 17,
        };
        for (edge, dx, dy) in [
            (WMSZ_LEFT, 1, 0),
            (WMSZ_RIGHT, -1, 0),
            (WMSZ_TOP, 0, 1),
            (WMSZ_BOTTOM, 0, -1),
            (WMSZ_TOPLEFT, 1, 1),
            (WMSZ_TOPRIGHT, -1, 1),
            (WMSZ_BOTTOMLEFT, 1, -1),
            (WMSZ_BOTTOMRIGHT, -1, -1),
        ] {
            let mut rect = original;
            shape.snap_frame(&mut rect, edge, nc);
            assert_eq!(rect.left, original.left + dx.max(0) * 11);
            assert_eq!(rect.right, original.right + dx.min(0) * 11);
            assert_eq!(rect.top, original.top + dy.max(0) * 17);
            assert_eq!(rect.bottom, original.bottom + dy.min(0) * 17);
        }
    }
}
