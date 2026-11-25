mod dialogs;
mod grid;
use grid::{LEVELS, Pages, Shape};
pub use grid::{MIN_HEIGHT, MIN_WIDTH};
mod paint;
mod popup_menu;
mod preview;
mod slide_surface;
mod windows;

use self::windows::*;
use crate::{Command, Event};
use anyhow::{Result, ensure};
use clip_everything::{
    board::{Board, Cell, Viewport},
    clipboard::wide,
    preferences::TileContent,
    store::{self, Item},
};
pub use dialogs::{Dialogs, translate};
use dialogs::{confirm, text};
use paint::{Bitmap, Face, Font, PreviewKey, Symbol};
pub(crate) use popup_menu::{MenuAnchor, PopupMenu};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    mem::size_of,
    ptr::{null, null_mut},
};
use tao::event_loop::EventLoopProxy;
use windows_sys::Win32::System::SystemServices::{SS_LEFT, SS_NOPREFIX};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    UI::{
        Controls::*, HiDpi::GetDpiForWindow, Input::KeyboardAndMouse::*, Shell::*,
        WindowsAndMessaging::*,
    },
};

pub enum Input {
    Geometry(String, [f64; 4], Viewport),
    Control(i32),
    Search,
    Key(u16),
    TabFocus(bool),
    Wheel(i32),
    CategoryScroll(i32, i32),
    CategoryKey(i32, u16),
    Pick(Binding),
    TileFocused(isize),
    Context(Option<Binding>, i32, i32),
    ContextAction(ContextAction),
    Hover(isize, Option<i64>),
    Dialog(isize, i32),
    Drop {
        app: String,
        rev: i64,
        id: i64,
        to: Cell,
    },
}
pub enum ContextAction {
    Rename(i64),
    Delete(i64),
    Arrange(String),
}
#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(default)]
struct Options {
    sidebar_collapsed: bool,
    level: i32,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            sidebar_collapsed: false,
            level: 5,
        }
    }
}
impl Options {
    fn load(raw: Option<String>) -> Self {
        let Some(value) = raw.and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        else {
            return Self::default();
        };
        let mut options: Self = serde_json::from_value(value.clone()).unwrap_or_default();
        if value.get("level").is_none()
            && let Some(size) = value["size"].as_i64()
        {
            options.level = 5 + ((size.clamp(32, 40) as i32 - 36) / 2);
        }
        options.level = options.level.clamp(1, LEVELS);
        options
    }
}
pub struct NativeUi {
    hwnd: HWND,
    bridge: Box<Bridge>,
    font: Font,
    bar: HashMap<i32, HWND>,
    category_buttons: Vec<HWND>,
    category_top: usize,
    category_capacity: usize,
    reveal_category: bool,
    category_wheel: i32,
    tiles: Vec<HWND>,
    items: Vec<Item>,
    ids: HashMap<i64, usize>,
    boards: Vec<Board>,
    categories: Vec<(String, String, usize, Option<i64>)>,
    active: String,
    records: Vec<Record>,
    query: String,
    selected: usize,
    page: i32,
    pages: Pages,
    initial_fit: bool,
    slide: Option<grid::SidebarSlide>,
    slide_surface: slide_surface::SlideSurface,
    visible: Vec<usize>,
    columns: i32,
    options: Options,
    tile_content: TileContent,
    visible_previews: HashMap<PreviewKey, HWND>,
    previews_queued: bool,
    ko: bool,
    preview_window: preview::Preview,
    context_menu: Option<PopupMenu>,
    last_geometry: Option<(String, [i32; 4], Viewport)>,
    wheel_delta: i32,
}
impl NativeUi {
    pub fn new(
        hwnd: HWND,
        proxy: EventLoopProxy<Event>,
        db: &Connection,
        ko: bool,
    ) -> Result<Self> {
        unsafe {
            let controls = INITCOMMONCONTROLSEX {
                dwSize: size_of::<INITCOMMONCONTROLSEX>() as u32,
                dwICC: ICC_STANDARD_CLASSES,
            };
            ensure!(
                InitCommonControlsEx(&controls) != 0,
                "Windows 컨트롤을 초기화할 수 없습니다."
            );
            let dpi = GetDpiForWindow(hwnd).max(96) as i32;
            let font = Font::new(13 * dpi / 96);
            let bridge = Box::new(Bridge::new(proxy));
            bridge.font.set(font.0);
            bridge.dpi.set(dpi);
            let preview_window = preview::Preview::new(hwnd)?;
            bridge.tip.set(preview_window.hwnd);
            db.execute_batch(
                "CREATE TABLE IF NOT EXISTS NativeView(Id INTEGER PRIMARY KEY,Json TEXT NOT NULL)",
            )?;
            let raw = db
                .query_row("SELECT Json FROM NativeView WHERE Id=1", [], |r| {
                    r.get::<_, String>(0)
                })
                .optional()?;
            let options = Options::load(raw);
            let mut ui = Self {
                hwnd,
                bridge,
                font,
                bar: HashMap::new(),
                category_buttons: vec![],
                category_top: 0,
                category_capacity: 1,
                reveal_category: true,
                category_wheel: 0,
                tiles: vec![],
                items: vec![],
                ids: HashMap::new(),
                boards: vec![],
                categories: vec![],
                active: String::new(),
                records: vec![],
                query: String::new(),
                selected: 0,
                page: 0,
                pages: Pages::new((4, 3), (4, 3)),
                initial_fit: true,
                slide: None,
                slide_surface: Default::default(),
                visible: Vec::new(),
                columns: 4,
                options,
                tile_content: TileContent::default(),
                visible_previews: HashMap::new(),
                previews_queued: false,
                ko,
                preview_window,
                context_menu: None,
                last_geometry: None,
                wheel_delta: 0,
            };
            ensure!(
                SetWindowSubclass(
                    hwnd,
                    Some(parent_proc),
                    3,
                    &*ui.bridge as *const Bridge as usize
                ) != 0,
                "UI 창을 연결할 수 없습니다."
            );
            for id in [
                SEARCH,
                SETTINGS,
                HELP,
                SMALLER,
                LARGER,
                TOGGLE_SIDEBAR,
                EMPTY_MESSAGE,
                CATEGORY_SCROLL,
                PREVIOUS,
                NEXT,
            ] {
                let (class, style) = match id {
                    EMPTY_MESSAGE => ("STATIC", SS_LEFT | SS_NOPREFIX),
                    CATEGORY_SCROLL => ("SCROLLBAR", SBS_VERT as u32),
                    SEARCH => ("EDIT", ES_AUTOHSCROLL as u32 | WS_TABSTOP),
                    _ => ("BUTTON", BS_OWNERDRAW as u32 | WS_TABSTOP),
                };
                let h = child(hwnd, id, class, "", style, &ui.bridge)?;
                ui.bar.insert(id, h);
            }
            ui.set_language(ko);
            Ok(ui)
        }
    }
    fn tr<'a>(&self, k: &'a str, e: &'a str) -> &'a str {
        if self.ko { k } else { e }
    }
    pub fn set_language(&mut self, ko: bool) {
        self.bridge.korean.set(ko);
        self.ko = ko;
        unsafe {
            SetWindowTextW(self.bar[&SEARCH], wide(&self.query).as_ptr());
        }
    }
    pub fn set_tile_content(&mut self, mode: TileContent) {
        self.tile_content = mode;
    }
    fn queue_tile_previews(&mut self) {
        if !self.previews_queued
            && self
                .visible_previews
                .keys()
                .any(|key| !self.bridge.previews.borrow().contains_key(key))
        {
            self.previews_queued = self.bridge.proxy.send_event(Event::TilePreviews).is_ok();
        }
    }
    pub fn load_tile_previews(&mut self, db: &Connection) {
        self.previews_queued = false;
        // Work in small batches, and only for the currently visible page.
        let missing = {
            let cached = self.bridge.previews.borrow();
            self.visible_previews
                .iter()
                .filter(|(key, _)| !cached.contains_key(key))
                .take(8)
                .map(|(&key, &tile)| (key, tile))
                .collect::<Vec<_>>()
        };
        for (key, tile) in missing {
            let image = store::preview_image(db, key.0)
                .ok()
                .flatten()
                .and_then(|bytes| Bitmap::decode(&bytes, 192, 128));
            self.bridge.previews.borrow_mut().insert(key, image);
            unsafe {
                InvalidateRect(tile, null(), 0);
            }
        }
        self.queue_tile_previews();
    }
    pub fn copy_succeeded(&self, id: i64) {
        self.bridge.copy_succeeded(self.hwnd, id);
    }
    pub fn hide(&self) {
        if let Some(menu) = &self.context_menu {
            menu.hide();
        }
        self.slide_surface.hide();
        self.bridge.cancel();
        self.bridge.hide_tip();
    }
    pub fn focus(&self) {
        unsafe {
            let h = self.tile_hwnd(self.selected).unwrap_or(self.bar[&SEARCH]);
            SetFocus(h);
        }
    }
    pub fn activate(&mut self, source: &str) {
        self.hide();
        self.active = self.resolve_source(source);
        self.reveal_category = true;
        self.query.clear();
        dialogs::set(self.bar[&SEARCH], "");
        self.page = 0;
        self.selected = 0;
        self.filter();
    }
    pub fn items(&mut self, items: Vec<Item>, db: &Connection) {
        self.items = items;
        self.ids = self
            .items
            .iter()
            .enumerate()
            .map(|(n, i)| (i.id, n))
            .collect();
        for id in self.items.iter().filter_map(|i| i.program_icon) {
            if self.bridge.icons.borrow().contains_key(&id) {
                continue;
            }
            if let Ok(bytes) =
                db.query_row("SELECT Icon FROM SourcePrograms WHERE Id=?", [id], |r| {
                    r.get::<_, Vec<u8>>(0)
                })
                && let Some(bmp) = Bitmap::decode(&bytes, 32, 32)
            {
                self.bridge
                    .icons
                    .borrow_mut()
                    .insert(id, std::rc::Rc::new(bmp));
            }
        }
        self.rebuild();
    }
    pub fn layouts(&mut self, data: Vec<Board>) -> Result<()> {
        // Tile HWNDs belong to cells. Keep keyboard focus with the item when
        // a committed move or swap rebinds those same HWNDs to new records.
        let source = unsafe { GetFocus() };
        let focused = self
            .bridge
            .bindings
            .borrow()
            .get(&(source as usize))
            .cloned();
        self.boards = data;
        self.rebuild();
        if let Some(binding) = focused
            && binding.app.as_deref() == Some(self.active.as_str())
            && let Some(id) = binding.record.item
            && let Some(index) = self.records.iter().position(|r| r.item == Some(id))
            && index != binding.index
        {
            self.selected = index;
            self.layout()?;
            if unsafe { GetFocus() } == source {
                if let Some(destination) = self.tile_hwnd(index) {
                    unsafe {
                        SetFocus(destination);
                    }
                } else {
                    self.choose(index)?;
                }
            }
            return Ok(());
        }
        self.layout()
    }
    fn rebuild(&mut self) {
        let mut cats = HashMap::new();
        for i in &self.items {
            let c = cats
                .entry(i.app.to_lowercase())
                .or_insert_with(|| (i.app.clone(), i.program_name.clone(), 0, i.program_icon));
            c.2 += 1;
        }
        self.categories = cats.into_values().collect();
        for (app, label, ..) in &mut self.categories {
            if label.is_empty() {
                *label = if app.is_empty() {
                    if self.ko {
                        "출처 미상".into()
                    } else {
                        "Unknown source".into()
                    }
                } else {
                    app.clone()
                };
            }
        }
        self.categories
            .sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));
        let selected = self.resolve_source(&self.active);
        if selected != self.active {
            self.active = selected;
            self.page = 0;
            self.selected = 0;
            self.reveal_category = true;
            self.query.clear();
            dialogs::set(self.bar[&SEARCH], "");
        }
        self.filter();
    }
    fn resolve_source(&self, preferred: &str) -> String {
        let find = |name: &str| {
            self.categories
                .iter()
                .find(|c| c.0.eq_ignore_ascii_case(name))
        };
        find(preferred)
            .or_else(|| find(&self.active))
            .map(|c| c.0.clone())
            .or_else(|| self.items.first().map(|item| item.app.clone()))
            .unwrap_or_default()
    }
    pub fn active_program(&self) -> &str {
        &self.active
    }
    fn board(&self) -> Option<&Board> {
        if self.categories.is_empty() {
            return None;
        }
        self.boards
            .iter()
            .find(|b| b.app.eq_ignore_ascii_case(&self.active))
    }
    pub fn item(&self, id: i64) -> Option<&Item> {
        self.ids.get(&id).map(|n| &self.items[*n])
    }
    fn title<'a>(&'a self, i: &'a Item) -> &'a str {
        if !i.name.is_empty() {
            &i.name
        } else if let Some(s) = i.preview.trim().lines().next() {
            s
        } else if !i.title.is_empty() {
            &i.title
        } else if !i.app.is_empty() {
            &i.app
        } else {
            self.tr("클립", "Clip")
        }
    }
    fn filter(&mut self) {
        let old = self.records.get(self.selected).and_then(|r| r.item);
        let (cols, rows) = self.dimensions();
        self.columns = cols;
        let mut records = if self.query.trim().is_empty()
            && let Some(b) = self.board()
        {
            let cols = i64::from(cols);
            let mut cells = (0..i64::from(rows) * cols)
                .map(|i| Record {
                    item: None,
                    cell: Some(Cell {
                        row: i / cols,
                        col: i % cols,
                    }),
                })
                .collect::<Vec<_>>();
            for slot in &b.slots {
                if let Some(cell) = cells.get_mut((slot.cell.row * cols + slot.cell.col) as usize)
                    && self.ids.contains_key(&slot.id)
                {
                    cell.item = Some(slot.id);
                }
            }
            cells
        } else {
            self.items
                .iter()
                .filter(|i| self.active.eq_ignore_ascii_case(&i.app))
                .map(|i| Record {
                    item: Some(i.id),
                    cell: None,
                })
                .collect()
        };
        let q = self.query.trim().to_lowercase();
        if !q.is_empty() {
            records.retain(|r| {
                r.item.and_then(|id| self.item(id)).is_some_and(|i| {
                    self.title(i).to_lowercase().contains(&q) || i.tags.to_lowercase().contains(&q)
                })
            });
        }
        self.selected = old
            .and_then(|id| records.iter().position(|r| r.item == Some(id)))
            .or_else(|| records.iter().position(|r| r.item.is_some()))
            .unwrap_or(0);
        self.records = records;
    }
    fn face(&self, h: HWND, face: Face, accessible: &str) {
        if self.bridge.faces.borrow().get(&(h as usize)) == Some(&face)
            && dialogs::text(h) == accessible
        {
            return;
        }
        dialogs::set(h, accessible);
        self.bridge.faces.borrow_mut().insert(h as usize, face);
        unsafe {
            InvalidateRect(h, null(), 0);
        }
    }
    fn tile_hwnd(&self, index: usize) -> Option<HWND> {
        self.visible
            .iter()
            .position(|&i| i == index)
            .and_then(|n| self.tiles.get(n))
            .copied()
    }
    fn dimensions(&self) -> (i32, i32) {
        let (cols, rows) = (self.pages.cols, self.pages.rows);
        if self.query.trim().is_empty()
            && let Some(b) = self.board()
        {
            let (occupied_cols, occupied_rows) = b.occupied_size();
            let full_pages =
                |occupied: i64, span: i32| ((occupied.max(1) as i32 + span - 1) / span) * span;
            return (
                full_pages(occupied_cols, cols),
                full_pages(occupied_rows, rows),
            );
        }
        (cols, rows)
    }
    pub fn moved(&mut self) -> Result<()> {
        self.last_geometry = None;
        self.layout()
    }
    pub fn layout(&mut self) -> Result<()> {
        self.layout_frame(false)?;
        if self.is_sliding() {
            self.slide_surface
                .capture(self.hwnd, self.bridge.rail_width.get());
        }
        Ok(())
    }
    fn layout_frame(&mut self, sidebar_frame: bool) -> Result<()> {
        unsafe {
            self.bridge.cancel();
            let dpi = GetDpiForWindow(self.hwnd).max(96) as i32;
            let px = |n: i32| (n * dpi + 48) / 96;
            if dpi != self.bridge.dpi.get() {
                let f = Font::new(px(13));
                self.bridge.font.set(f.0);
                self.bridge.dpi.set(dpi);
                for &h in self
                    .bar
                    .values()
                    .chain(&self.tiles)
                    .chain(&self.category_buttons)
                {
                    SendMessageW(h, WM_SETFONT, f.0 as usize, 0);
                }
                self.font = f;
            }
            let mut shape = Shape::new(self.options.level, dpi, self.options.sidebar_collapsed);
            if self.initial_fit {
                self.initial_fit = false;
                shape.fit_initial_window(self.hwnd);
            }
            let rail = self.slide.as_ref().map_or(
                px(grid::sidebar_width(self.options.sidebar_collapsed)),
                |slide| slide.width,
            );
            shape.left = rail + px(8);
            let mut bounds = RECT::default();
            GetClientRect(self.hwnd, &mut bounds);
            let w = bounds.right;
            let h = bounds.bottom;
            if w < px(40) || h < px(60) {
                return Ok(());
            }
            self.bridge.grid_shape.set(Some(shape));
            let capacity = shape.capacity(w, h);
            let resized = capacity != (self.pages.cols, self.pages.rows);
            let old_origin = self.pages.origin(self.page);
            if resized {
                self.pages = Pages::new(capacity, capacity);
                self.filter();
            }
            let (cols, rows) = self.dimensions();
            let display_rows = if self.query.trim().is_empty() {
                rows
            } else {
                ((self.records.len() as i32 + cols - 1) / cols).max(1)
            };
            self.pages = Pages::new(capacity, (cols, display_rows));
            if resized {
                let col = old_origin.0.min(cols - 1);
                let row = old_origin.1.min(display_rows - 1);
                self.page = self.pages.containing(col, row);
            }
            self.page = self.page.clamp(0, self.pages.count - 1);
            let paged = self.pages.count > 1;
            let margin = px(8);
            let gap = shape.gap;
            let compact = px(grid::COMPACT_SIDEBAR);
            let expanded = px(grid::SIDEBAR);
            let openness = ((rail - compact) as f32 / (expanded - compact) as f32).clamp(0., 1.);
            let collapsed = openness <= 0.22;
            let label_fade = ((openness - 0.22) / 0.58).clamp(0., 1.);
            self.bridge.rail_width.set(rail);
            let row_h = px(28);
            let rail_pad = px(12);
            let rail_button = px(36);
            let category_height = px(40);
            let row_pitch = category_height + px(8);
            let list_y = rail_pad + rail_button + px(12);
            let help_y = h - rail_pad - rail_button;
            let settings_y = help_y - rail_button - px(4);
            self.category_capacity =
                ((settings_y - px(10) - list_y + px(8)) / row_pitch).max(1) as usize;
            let programs = self.categories.len();
            let scroll = programs > self.category_capacity;
            self.category_top = self
                .category_top
                .min(programs.saturating_sub(self.category_capacity));
            if self.reveal_category {
                if let Some(index) = self.categories.iter().position(|c| c.0 == self.active) {
                    self.category_top = self
                        .category_top
                        .min(index)
                        .max((index + 1).saturating_sub(self.category_capacity));
                }
                self.reveal_category = false;
            }
            while self.category_buttons.len() < self.categories.len() {
                self.category_buttons.push(child(
                    self.hwnd,
                    TAB + self.category_buttons.len() as i32,
                    "BUTTON",
                    "",
                    BS_OWNERDRAW as u32 | WS_TABSTOP,
                    &self.bridge,
                )?);
            }
            let mut placements = PlacementBatch::new();
            let list_width = rail - 2 * rail_pad;
            let toggle = self.bar[&TOGGLE_SIDEBAR];
            let action = if self.options.sidebar_collapsed {
                self.tr("사이드바 펼치기", "Expand sidebar")
            } else {
                self.tr("사이드바 접기", "Collapse sidebar")
            };
            self.face(
                toggle,
                Face {
                    symbol: Some(if self.options.sidebar_collapsed {
                        Symbol::OpenSidebar
                    } else {
                        Symbol::CloseSidebar
                    }),
                    text: if collapsed {
                        String::new()
                    } else {
                        self.tr("프로그램", "Programs").into()
                    },
                    centered: collapsed,
                    label_alpha: Some((255. * label_fade).round() as u8),
                    ..Default::default()
                },
                action,
            );
            placements.position(toggle, rail_pad, rail_pad, list_width, rail_button);
            for (n, &button) in self.category_buttons.iter().enumerate() {
                let Some((app, label, count, icon)) = self.categories.get(n) else {
                    ShowWindow(button, SW_HIDE);
                    continue;
                };
                if !(self.category_top..self.category_top + self.category_capacity).contains(&n) {
                    ShowWindow(button, SW_HIDE);
                    continue;
                }
                let row = n - self.category_top;
                self.face(
                    button,
                    Face {
                        text: if collapsed {
                            String::new()
                        } else {
                            label.clone()
                        },
                        count: (!collapsed).then(|| count.to_string()),
                        centered: collapsed,
                        label_alpha: Some((255. * label_fade).round() as u8),
                        icon: *icon,
                        solid: true,
                        active: *app == self.active,
                        fallback: if icon.is_some() {
                            String::new()
                        } else {
                            initial(label)
                        },
                        ..Default::default()
                    },
                    &format!("{label}  {count}"),
                );
                placements.position(
                    button,
                    rail_pad,
                    list_y + row as i32 * row_pitch,
                    list_width - if scroll { px(10) } else { 0 },
                    category_height,
                );
            }
            let scrollbar = self.bar[&CATEGORY_SCROLL];
            if scroll {
                placements.position(
                    scrollbar,
                    rail - rail_pad - px(7),
                    list_y,
                    px(8),
                    self.category_capacity as i32 * row_pitch - px(8),
                );
                let info = SCROLLINFO {
                    cbSize: size_of::<SCROLLINFO>() as u32,
                    fMask: SIF_RANGE | SIF_PAGE | SIF_POS,
                    nMin: 0,
                    nMax: programs as i32 - 1,
                    nPage: self.category_capacity as u32,
                    nPos: self.category_top as i32,
                    ..Default::default()
                };
                SetScrollInfo(scrollbar, SB_CTL, &info, 1);
            } else {
                ShowWindow(scrollbar, SW_HIDE);
            }
            for (id, label, symbol, y) in [
                (
                    SETTINGS,
                    self.tr("설정", "Settings"),
                    Symbol::Settings,
                    settings_y,
                ),
                (HELP, self.tr("도움말", "Help"), Symbol::Help, help_y),
            ] {
                let button = self.bar[&id];
                self.face(
                    button,
                    Face {
                        symbol: Some(symbol),
                        centered: collapsed,
                        label_alpha: Some((255. * label_fade).round() as u8),
                        text: if collapsed {
                            String::new()
                        } else {
                            label.into()
                        },
                        ..Default::default()
                    },
                    label,
                );
                placements.position(button, rail_pad, y, list_width, rail_button);
            }
            let mut right = w - margin;
            if paged {
                for (id, label, symbol, enabled) in [
                    (
                        NEXT,
                        self.tr("다음 페이지", "Next page"),
                        Symbol::Next,
                        self.page + 1 < self.pages.count,
                    ),
                    (
                        PREVIOUS,
                        self.tr("이전 페이지", "Previous page"),
                        Symbol::Previous,
                        self.page > 0,
                    ),
                ] {
                    right -= px(26);
                    self.face(
                        self.bar[&id],
                        Face {
                            symbol: Some(symbol),
                            centered: true,
                            ..Default::default()
                        },
                        &format!("{label} ({}/{})", self.page + 1, self.pages.count),
                    );
                    placements.position(self.bar[&id], right, margin, px(26), row_h);
                    EnableWindow(self.bar[&id], enabled as i32);
                    right -= px(4);
                }
            } else {
                ShowWindow(self.bar[&PREVIOUS], SW_HIDE);
                ShowWindow(self.bar[&NEXT], SW_HIDE);
            }
            for (id, symbol, label, enabled) in [
                (
                    LARGER,
                    Symbol::Add,
                    self.tr("버튼 크게", "Increase button size"),
                    self.options.level < LEVELS,
                ),
                (
                    SMALLER,
                    Symbol::Minus,
                    self.tr("버튼 작게", "Decrease button size"),
                    self.options.level > 1,
                ),
            ] {
                right -= px(28);
                self.face(
                    self.bar[&id],
                    Face {
                        symbol: Some(symbol),
                        centered: true,
                        ..Default::default()
                    },
                    &format!("{label} ({}/{LEVELS})", self.options.level),
                );
                placements.position(self.bar[&id], right, margin, px(28), row_h);
                EnableWindow(self.bar[&id], enabled as i32);
                right -= px(4);
            }
            let hint = if right - shape.left < px(200) {
                self.tr("검색 (Ctrl+F)", "Search (Ctrl+F)")
            } else {
                self.tr(
                    "제목 또는 태그 검색 (Ctrl+F)",
                    "Search titles or tags (Ctrl+F)",
                )
            };
            SendMessageW(
                self.bar[&SEARCH],
                EM_SETCUEBANNER,
                1,
                wide(hint).as_ptr() as isize,
            );
            self.bridge.search_bounds.set(RECT {
                left: shape.left,
                top: margin,
                right,
                bottom: margin + row_h,
            });
            let edit_h = px(20);
            placements.position(
                self.bar[&SEARCH],
                shape.left + px(30),
                margin + (row_h - edit_h) / 2,
                right - shape.left - px(38),
                edit_h,
            );
            let y = shape.top;
            let button_h = shape.height;
            let pitch = button_h + gap;
            let width = shape.width;
            let (first_col, first_row) = self.pages.origin(self.page);
            self.visible.clear();
            let render_empty = self.board().is_some() && self.query.trim().is_empty();
            let empty = self.bar[&EMPTY_MESSAGE];
            if self.categories.is_empty() {
                dialogs::set(empty, self.tr("등록한 항목이 없습니다.", "No saved clips."));
                placements.position(empty, shape.left, y, w - shape.left - shape.edge, row_h);
            } else {
                ShowWindow(empty, SW_HIDE);
            }
            for row in first_row..(first_row + self.pages.rows).min(display_rows) {
                for col in first_col..(first_col + self.pages.cols).min(cols) {
                    let index = (row * cols + col) as usize;
                    if render_empty || index < self.records.len() {
                        self.visible.push(index);
                    }
                }
            }
            let previous_focus = GetFocus();
            let had_tile_focus = self.tiles.contains(&previous_focus);
            if !self.visible.contains(&self.selected) {
                self.selected = self
                    .visible
                    .iter()
                    .copied()
                    .find(|&n| self.records.get(n).is_some_and(|r| r.item.is_some()))
                    .or_else(|| self.visible.first().copied())
                    .unwrap_or(0);
            }
            let num = self.visible.len();
            while self.tiles.len() < num {
                self.tiles.push(child(
                    self.hwnd,
                    TILE + self.tiles.len() as i32,
                    "BUTTON",
                    "",
                    BS_OWNERDRAW as u32 | WS_TABSTOP,
                    &self.bridge,
                )?);
            }
            self.bridge.bindings.borrow_mut().clear();
            self.visible_previews.clear();
            for (n, &ctrl) in self.tiles.iter().enumerate() {
                if n >= num {
                    ShowWindow(ctrl, SW_HIDE);
                    continue;
                }
                let index = self.visible[n];
                let r = self.records.get(index).copied().unwrap_or(Record {
                    item: None,
                    cell: None,
                });
                let i = r.item.and_then(|id| self.item(id));
                let label = i
                    .map(|i| self.title(i))
                    .unwrap_or(self.tr("빈칸", "Empty cell"));
                let face = Face {
                    text: if i.is_some() {
                        label.into()
                    } else {
                        String::new()
                    },
                    icon: i
                        .filter(|_| self.tile_content == TileContent::Icon)
                        .and_then(|i| i.program_icon),
                    preview: i
                        .filter(|i| {
                            self.tile_content == TileContent::Preview
                                && (i.shot != 0 || i.has_thumbnail)
                        })
                        .map(|i| (i.id, i.shot)),
                    active: index == self.selected,
                    stacked: true,
                    blank: i.is_none(),
                    fallback: i
                        .filter(|i| {
                            self.tile_content == TileContent::Icon && i.program_icon.is_none()
                        })
                        .map(|i| initial(&i.program_name))
                        .unwrap_or_default(),
                    ..Default::default()
                };
                let preview = face.preview;
                self.face(ctrl, face, &self.bridge.item_label(r.item, label));
                if let Some(key) = preview {
                    self.visible_previews.insert(key, ctrl);
                }
                self.bridge.bindings.borrow_mut().insert(
                    ctrl as usize,
                    Binding {
                        record: r,
                        index,
                        app: r.cell.map(|_| self.active.clone()),
                        rev: self.board().map_or(0, |b| b.revision),
                    },
                );
                placements.position(
                    ctrl,
                    shape.left + (index as i32 % cols - first_col) * (width + gap),
                    y + (index as i32 / cols - first_row) * pitch,
                    width,
                    button_h,
                );
            }
            self.bridge
                .previews
                .borrow_mut()
                .retain(|key, _| self.visible_previews.contains_key(key));
            self.queue_tile_previews();
            placements.apply(self.hwnd);
            if had_tile_focus && (resized || !self.tiles[..num].contains(&previous_focus)) {
                self.focus();
            }
            let geo = (
                self.active.clone(),
                [shape.left + width / 2, y + button_h / 2, width + gap, pitch],
                Viewport {
                    origin: Cell {
                        row: i64::from(first_row),
                        col: i64::from(first_col),
                    },
                    rows: i64::from(self.pages.rows),
                    cols: i64::from(self.pages.cols),
                },
            );
            if self.board().is_some()
                && self.query.trim().is_empty()
                && !self.is_sliding()
                && self.last_geometry.as_ref() != Some(&geo)
            {
                self.last_geometry = Some(geo.clone());
                let scale = dpi as f64 / 96.;
                self.bridge.event(Input::Geometry(
                    geo.0,
                    geo.1.map(|n| f64::from(n) / scale),
                    geo.2,
                ));
            }
            if sidebar_frame {
                let dirty = RECT {
                    left: 0,
                    top: 0,
                    right: rail + px(2),
                    bottom: h,
                };
                RedrawWindow(
                    self.hwnd,
                    &dirty,
                    null_mut(),
                    RDW_INVALIDATE | RDW_ERASE | RDW_FRAME | RDW_ALLCHILDREN | RDW_UPDATENOW,
                );
            } else {
                InvalidateRect(self.hwnd, null(), 1);
            }
            Ok(())
        }
    }
    fn choose(&mut self, n: usize) -> Result<()> {
        self.selected = n.min(self.records.len().saturating_sub(1));
        self.page = self.pages.containing(
            self.selected as i32 % self.columns,
            self.selected as i32 / self.columns,
        );
        self.layout()?;
        self.focus();
        Ok(())
    }
    pub fn focus_item(&mut self, id: i64) -> Result<()> {
        if let Some(n) = self.records.iter().position(|r| r.item == Some(id)) {
            self.choose(n)?;
        }
        Ok(())
    }
    pub fn action(&mut self, event: Input, db: &Connection) -> Result<Option<Command>> {
        if !matches!(
            event,
            Input::Geometry(..) | Input::Control(TOGGLE_SIDEBAR) | Input::Hover(..)
        ) {
            self.slide_surface.hide();
        }
        match event {
            Input::Dialog(..) => {}
            Input::Geometry(app, geometry, viewport) => {
                return Ok(Some(Command::GridGeometry {
                    app,
                    geometry,
                    viewport,
                }));
            }
            Input::Control(SEARCH) => {}
            Input::Control(PREVIOUS) => self.turn_page(-1)?,
            Input::Control(NEXT) => self.turn_page(1)?,
            Input::Control(SETTINGS) => return Ok(Some(Command::ShowSettings)),
            Input::Control(HELP) => return Ok(Some(Command::ShowHelp)),
            Input::Control(id) if id == SMALLER || id == LARGER => {
                let level =
                    (self.options.level + if id == LARGER { 1 } else { -1 }).clamp(1, LEVELS);
                if level != self.options.level {
                    self.options.level = level;
                    self.save_options(db)?;
                }
            }
            Input::Control(TOGGLE_SIDEBAR) => return Ok(Some(Command::ToggleSidebar)),
            Input::Control(id) if (TAB..TILE).contains(&id) => {
                self.select_category((id - TAB) as usize)?
            }
            Input::CategoryKey(id, key) => {
                if matches!(key, VK_RIGHT | VK_RETURN) {
                    self.focus();
                } else {
                    let index = id - TAB + i32::from(key == VK_DOWN) - i32::from(key == VK_UP);
                    self.select_category(
                        index.clamp(0, self.categories.len().saturating_sub(1) as i32) as usize,
                    )?;
                }
            }
            Input::CategoryScroll(code, pos) => {
                let top = self.category_top as i32;
                let cap = self.category_capacity as i32;
                let next = match code {
                    SB_LINEUP => top - 1,
                    SB_LINEDOWN => top + 1,
                    SB_PAGEUP => top - cap,
                    SB_PAGEDOWN => top + cap,
                    SB_THUMBTRACK | SB_THUMBPOSITION => pos,
                    SB_TOP => 0,
                    SB_BOTTOM => i32::MAX,
                    _ => top,
                };
                self.scroll_categories(next)?;
            }
            Input::Control(_) => {}
            Input::Search => {
                let q = text(self.bar[&SEARCH]);
                if q != self.query {
                    self.query = q;
                    self.page = 0;
                    self.filter();
                    self.layout()?;
                }
            }
            Input::Pick(r) => {
                self.selected = r.index;
                if let Some(id) = r.record.item {
                    return Ok(Some(Command::Restore { id }));
                }
                self.layout()?;
            }
            Input::TileFocused(hwnd) => {
                let bindings = self.bridge.bindings.borrow();
                if unsafe { GetFocus() } as isize == hwnd
                    && let Some(focused) = bindings.get(&(hwnd as usize))
                    && self.selected != focused.index
                {
                    self.selected = focused.index;
                    for (&key, face) in self.bridge.faces.borrow_mut().iter_mut() {
                        if let Some(binding) = bindings.get(&key) {
                            face.active = binding.index == focused.index;
                            unsafe {
                                InvalidateRect(key as HWND, null(), 0);
                            }
                        }
                    }
                }
            }
            Input::Drop { app, rev, id, to } => {
                return Ok(Some(Command::MoveSlot { app, rev, id, to }));
            }
            Input::Context(r, x, y) => self.context(r, x, y)?,
            Input::ContextAction(action) => return Ok(self.context_command(action)),
            Input::Hover(h, id) => self.preview(h as HWND, id, db)?,
            Input::Wheel(d) => {
                let mut p = POINT::default();
                unsafe {
                    GetCursorPos(&mut p);
                    ScreenToClient(self.hwnd, &mut p);
                }
                if p.x < self.bridge.rail_width.get() {
                    self.category_wheel += d;
                    let steps = self.category_wheel / 120;
                    self.category_wheel %= 120;
                    self.scroll_categories(self.category_top as i32 - steps * 3)?;
                    return Ok(None);
                }
                self.wheel_delta += d;
                let steps = self.wheel_delta / 120;
                self.wheel_delta %= 120;
                if steps != 0 {
                    self.turn_page(-steps)?;
                }
            }
            Input::TabFocus(back) => unsafe {
                let next = GetNextDlgTabItem(self.hwnd, GetFocus(), back as i32);
                if !next.is_null() {
                    SetFocus(next);
                }
            },
            Input::Key(key) => match key {
                VK_PRIOR => self.turn_page(-1)?,
                VK_NEXT => self.turn_page(1)?,
                VK_ESCAPE => return Ok(Some(Command::Hide)),
                0x46 | 0x4b => unsafe {
                    SetFocus(self.bar[&SEARCH]);
                    SendMessageW(self.bar[&SEARCH], EM_SETSEL, 0, -1);
                },
                VK_RETURN => {
                    if let Some(id) = self.records.get(self.selected).and_then(|r| r.item) {
                        return Ok(Some(Command::Restore { id }));
                    }
                }
                _ => {
                    let delta = match key {
                        VK_UP => -self.columns,
                        VK_DOWN => self.columns,
                        VK_LEFT => -1,
                        VK_RIGHT => 1,
                        VK_HOME => -(self.selected as i32),
                        VK_END => self.records.len() as i32,
                        _ => 0,
                    };
                    self.choose((self.selected as i32 + delta).max(0) as usize)?;
                }
            },
        }
        Ok(None)
    }
    fn scroll_categories(&mut self, next: i32) -> Result<()> {
        let max = self.categories.len().saturating_sub(self.category_capacity);
        let top = (next.max(0) as usize).min(max);
        if top != self.category_top {
            self.category_top = top;
            self.layout()?;
        }
        Ok(())
    }
    fn select_category(&mut self, index: usize) -> Result<()> {
        if let Some(c) = self.categories.get(index) {
            self.activate(&c.0.clone());
            self.layout()?;
            unsafe {
                SetFocus(self.category_buttons[index]);
            }
        }
        Ok(())
    }
    fn turn_page(&mut self, direction: i32) -> Result<()> {
        let next = (self.page + direction).clamp(0, self.pages.count - 1);
        if next != self.page {
            self.page = next;
            let (col, row) = self.pages.origin(next);
            self.selected = (row * self.columns + col) as usize;
            self.layout()?;
            self.focus();
        }
        Ok(())
    }
    fn save_options(&mut self, db: &Connection) -> Result<()> {
        db.execute(
            "INSERT OR REPLACE INTO NativeView VALUES(1,?)",
            params![serde_json::to_string(&self.options)?],
        )?;
        self.page = 0;
        self.layout()
    }
    pub fn minimum_width(&self) -> i32 {
        grid::minimum_width(self.is_sliding() || self.options.sidebar_collapsed)
    }
    pub fn toggle_sidebar(&mut self, db: &Connection) -> Result<()> {
        let next = Options {
            sidebar_collapsed: !self.options.sidebar_collapsed,
            ..self.options
        };
        db.execute(
            "INSERT OR REPLACE INTO NativeView VALUES(1,?)",
            params![serde_json::to_string(&next)?],
        )?;
        let slide = grid::SidebarSlide::start(
            self.hwnd,
            self.bridge.rail_width.get(),
            next.sidebar_collapsed,
            self.bridge.dpi.get(),
        );
        if slide.animated() {
            self.slide_surface
                .capture(self.hwnd, self.bridge.rail_width.get());
        }
        self.slide = Some(slide);
        self.options = next;
        Ok(())
    }
    pub fn is_sliding(&self) -> bool {
        self.slide.is_some()
    }
    pub fn advance_sidebar(&mut self) -> Result<bool> {
        let Some(slide) = self.slide.as_mut() else {
            return Ok(false);
        };
        self.bridge.anchor_right.set(true);
        let done = slide.advance(
            self.hwnd,
            unsafe { GetDpiForWindow(self.hwnd) }.max(96) as i32,
            self.options.sidebar_collapsed,
        );
        self.bridge.anchor_right.set(false);
        if done {
            self.slide = None;
            self.last_geometry = None;
        }
        self.layout_frame(true)?;
        if done {
            unsafe {
                RedrawWindow(
                    self.hwnd,
                    null(),
                    null_mut(),
                    RDW_INVALIDATE | RDW_ERASE | RDW_FRAME | RDW_ALLCHILDREN | RDW_UPDATENOW,
                );
            }
            self.slide_surface.finish();
        } else {
            self.slide_surface
                .align(self.hwnd, self.bridge.rail_width.get());
        }
        Ok(done)
    }
    fn context(&mut self, binding: Option<Binding>, x: i32, y: i32) -> Result<()> {
        let Some(board) = self.board() else {
            return Ok(());
        };
        let id = binding
            .and_then(|b| b.record.item)
            .filter(|&id| self.item(id).is_some());
        let mut entries = Vec::new();
        if let Some(id) = id {
            entries.push((
                self.tr("이름 수정", "Rename"),
                Event::Ui(Input::ContextAction(ContextAction::Rename(id))),
            ));
            entries.push((
                self.tr("클립 삭제", "Delete clip"),
                Event::Ui(Input::ContextAction(ContextAction::Delete(id))),
            ));
        }
        let divider = (!entries.is_empty()).then_some(entries.len());
        entries.push((
            self.tr("자동 정렬", "Auto arrange"),
            Event::Ui(Input::ContextAction(ContextAction::Arrange(
                board.app.clone(),
            ))),
        ));
        let title = self.tr("ClipEverything 클립 메뉴", "ClipEverything clip menu");
        self.bridge.hide_tip();
        let menu = match &mut self.context_menu {
            Some(menu) => menu,
            None => self
                .context_menu
                .insert(PopupMenu::new(self.hwnd, self.bridge.proxy.clone())?),
        };
        menu.show(title, MenuAnchor::Point(POINT { x, y }), entries, divider)
    }
    fn context_command(&self, action: ContextAction) -> Option<Command> {
        match action {
            ContextAction::Rename(id) if self.item(id).is_some() => Some(Command::OpenEdit { id }),
            ContextAction::Delete(id) if self.item(id).is_some() => confirm(
                self.hwnd,
                self.tr("클립을 삭제하시겠습니까?", "Delete this clip?"),
            )
            .then_some(Command::Delete { id }),
            ContextAction::Arrange(app) if self.active.eq_ignore_ascii_case(&app) => {
                Some(Command::ArrangeBoard {
                    app,
                    cols: i64::from(self.pages.cols),
                })
            }
            _ => None,
        }
    }
    fn preview(&self, cell: HWND, id: Option<i64>, db: &Connection) -> Result<()> {
        unsafe {
            let mut p = POINT::default();
            GetCursorPos(&mut p);
            if self.context_menu.as_ref().is_some_and(PopupMenu::visible)
                || self.bridge.hover.get() != cell as usize
                || WindowFromPoint(p) != cell
                || !GetCapture().is_null()
                || IsWindowVisible(self.hwnd) == 0
            {
                return Ok(());
            }
            let content = if let Some(id) = id {
                if self
                    .bridge
                    .bindings
                    .borrow()
                    .get(&(cell as usize))
                    .and_then(|b| b.record.item)
                    != Some(id)
                {
                    return Ok(());
                }
                let Some(i) = self.item(id) else {
                    return Ok(());
                };
                let title = self.title(i).trim();
                let image = if i.shot != 0 || i.has_thumbnail {
                    store::preview_image(db, id)?
                } else {
                    None
                };
                preview::Content {
                    title: title.into(),
                    description: preview::excerpt(title, &i.preview),
                    icon: i
                        .program_icon
                        .and_then(|key| self.bridge.icons.borrow().get(&key).cloned()),
                    image,
                }
            } else {
                preview::Content {
                    title: dialogs::text(cell),
                    description: String::new(),
                    icon: None,
                    image: None,
                }
            };
            self.preview_window
                .show(cell, p, self.bridge.dpi.get(), content);
            Ok(())
        }
    }
}
fn initial(s: &str) -> String {
    s.chars().next().unwrap_or('◇').to_uppercase().collect()
}
impl Drop for NativeUi {
    fn drop(&mut self) {
        unsafe {
            KillTimer(self.hwnd, grid::SLIDE_TIMER);
            self.bridge.clear_copy(self.hwnd);
            RemoveWindowSubclass(self.hwnd, Some(parent_proc), 3);
            self.bridge.cancel();
            for &h in self
                .bar
                .values()
                .chain(&self.tiles)
                .chain(&self.category_buttons)
            {
                RemoveWindowSubclass(h, Some(windows::child_proc), 2);
                DestroyWindow(h);
            }
            self.bridge.tip.set(null_mut());
        }
    }
}
#[cfg(test)]
mod tests {
    use super::Options;
    #[test]
    fn previous_view_settings_keep_size_and_start_expanded() {
        let old = Options::load(Some(
            r#"{"icons":true,"cols":3,"icon_cols":6,"rows":4,"size":40}"#.into(),
        ));
        assert_eq!(old.level, 7);
        assert!(!old.sidebar_collapsed);
        let saved = serde_json::to_string(&old).unwrap();
        assert!(!saved.contains("icons"));
        assert_eq!(Options::load(Some(saved)).level, 7);
        assert_eq!(Options::load(None).level, 5);
    }
}
