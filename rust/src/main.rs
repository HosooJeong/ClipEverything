#![windows_subsystem = "windows"]
use clip_everything::{board, clipboard, preferences, store};
mod native_ui;
use native_ui::{Dialogs, Input, NativeUi, WindowSize};
mod layout_writer;
mod shell;
use layout_writer::{LayoutWrite, LayoutWriter};
mod snip;
mod source_icon;

use anyhow::{Context, Result, bail, ensure};
use rusqlite::{Connection, OptionalExtension};
use serde_json::Value;
use shell::{Settings, Shell};
use std::{
    env, fs,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};
use store::Source;
use tao::{
    dpi::{LogicalSize, PhysicalPosition},
    event::{Event as WindowEventLoop, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy},
    platform::windows::{EventLoopBuilderExtWindows, WindowBuilderExtWindows, WindowExtWindows},
    window::{Window, WindowBuilder},
};
use windows_sys::Win32::{
    Foundation::*, System::Threading::CreateMutexW, UI::WindowsAndMessaging::*,
};

enum Event {
    Restored(Result<bool, String>),
    LayoutDrained,
    Arrange,
    TilePreviews,
    SidebarFrame,
    WindowMoved,
    LayoutChanged(Result<Option<String>, String>),
    Capture,
    Palette,
    Open(bool),
    Menu,
    TrayReset,
    Quit,
    Ui(Input),
    Captured(Result<(Registration, Option<i64>), String>),
    Saved(Result<i64, String>, Source, bool),
    Snipped(Result<Option<Vec<u8>>, String>),
}

#[derive(Clone, Copy)]
enum PreviewChange {
    Keep,
    Replace,
    Clear,
}
impl PreviewChange {
    fn edit(self, draft: Option<&[u8]>) -> Result<store::PreviewEdit<'_>> {
        Ok(match self {
            Self::Keep => store::PreviewEdit::Keep,
            Self::Replace => {
                store::PreviewEdit::Replace(draft.context("미리보기를 다시 캡처해 주십시오.")?)
            }
            Self::Clear => store::PreviewEdit::Clear,
        })
    }
}

enum Command {
    ToggleSidebar,
    ShowSettings,
    ShowHelp,
    CancelDialog,
    OpenEdit {
        id: i64,
    },
    Edit {
        id: i64,
        name: String,
        tags: String,
        shot: PreviewChange,
    },
    Register {
        name: String,
        tags: String,
        shot: PreviewChange,
    },
    MoveSlot {
        app: String,
        rev: i64,
        id: i64,
        to: board::Cell,
    },
    ArrangeBoard {
        app: String,
        cols: i64,
    },
    GridGeometry {
        app: String,
        geometry: [f64; 4],
        viewport: board::Viewport,
    },
    Snip,
    Delete {
        id: i64,
    },
    Clear,
    Restore {
        id: i64,
    },
    Settings {
        value: Settings,
    },
    Hide,
    Quit,
}

struct PendingRestore {
    item_id: i64,
    cancelled: Arc<AtomicBool>,
    automatic: bool,
}

struct Registration {
    formats: Vec<store::Format>,
    source: Source,
    pointer: Option<(i32, i32)>,
}
impl Registration {
    fn capture(owner: isize, source: Source, pointer: Option<(i32, i32)>) -> Result<Self> {
        let formats = clipboard::copy_selection(owner, &source)?;
        Ok(Self {
            formats,
            source,
            pointer,
        })
    }
}

struct App {
    restoring: Option<PendingRestore>,
    dialogs: Dialogs,
    ui: NativeUi,
    layout_writer: LayoutWriter,
    window: Window,
    shell: Shell,
    db: Connection,
    dir: PathBuf,
    settings: Settings,
    target: Option<Source>,
    busy: bool,
    board_opened: bool,
    last_window: Option<(PhysicalPosition<i32>, WindowSize)>,
    initial_source: String,
    snipping: bool,
    registration: Option<Arc<Registration>>,
    draft: Option<Arc<[u8]>>,
    proxy: EventLoopProxy<Event>,
}
fn ko(s: &Settings) -> bool {
    s.language == "ko"
        || (s.language == "system"
            && unsafe { windows_sys::Win32::Globalization::GetUserDefaultUILanguage() } & 0x3ff
                == 0x12)
}
impl App {
    fn cancel_automatic_paste(&self) {
        if let Some(job) = &self.restoring
            && job.automatic
        {
            job.cancelled.store(true, Ordering::Release);
        }
    }
    fn hide_launcher(&mut self) -> Result<()> {
        self.cancel_automatic_paste();
        self.ui.hide();
        self.dialogs.visibility(false);
        self.window.set_visible(false);
        Ok(())
    }
    fn show_launcher(&mut self) -> Result<()> {
        if self.window.is_minimized() {
            self.window.set_minimized(false);
        }
        self.window.set_visible(true);
        if self.dialogs.active() {
            self.dialogs.visibility(true);
        } else {
            unsafe {
                SetForegroundWindow(self.window.hwnd() as HWND);
            }
            self.ui.focus();
        }
        Ok(())
    }
    fn start_restore(&mut self, id: i64, paste: bool) -> Result<()> {
        ensure!(
            !self.busy && !self.snipping && self.restoring.is_none(),
            "클립보드 작업이 진행 중입니다."
        );
        let path = self.dir.join("clips.db");
        let owner = self.window.hwnd();
        let target = if paste { self.target.clone() } else { None };
        let automatic = target.is_some();
        let proxy = self.proxy.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let token = Arc::clone(&cancel);
        thread::Builder::new()
            .name("clipboard-restore".into())
            .spawn(move || {
                let result = (|| -> Result<bool> {
                    let db = Connection::open_with_flags(
                        path,
                        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
                    )?;
                    let formats = store::formats(&db, id)?;
                    if token.load(Ordering::Acquire) {
                        return Ok(false);
                    }
                    let Some(sequence) =
                        clipboard::restore_if(owner, &formats, || !token.load(Ordering::Acquire))?
                    else {
                        return Ok(false);
                    };
                    if let Some(src) = target
                        && !token.load(Ordering::Acquire)
                    {
                        clipboard::paste_restored(owner, sequence, &src, &token)?;
                        return Ok(true);
                    }
                    Ok(false)
                })();
                let _ = proxy.send_event(Event::Restored(result.map_err(|e| format!("{e:#}"))));
            })?;
        self.restoring = Some(PendingRestore {
            item_id: id,
            cancelled: cancel,
            automatic,
        });
        Ok(())
    }
    fn open_current(&mut self, settings: bool) -> Result<()> {
        if self.dialogs.active() {
            return self.show_launcher();
        }
        self.target = None;
        let app = if self.board_opened {
            self.ui.active_program().to_owned()
        } else {
            self.initial_source.clone()
        };
        self.board(&app)?;
        if settings {
            self.command(Command::ShowSettings)?;
        }
        Ok(())
    }
    fn position_board(&self, origin: Option<PhysicalPosition<i32>>, size: Option<WindowSize>) {
        use windows_sys::Win32::Graphics::Gdi::*;
        unsafe {
            let mut cursor = POINT::default();
            GetCursorPos(&mut cursor);
            let saved = origin
                .map(|p| MonitorFromPoint(POINT { x: p.x, y: p.y }, MONITOR_DEFAULTTONULL))
                .unwrap_or(std::ptr::null_mut());
            let monitor = if saved.is_null() {
                MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST)
            } else {
                saved
            };
            let mut info = MONITORINFO {
                cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            if GetMonitorInfoW(monitor, &mut info) != 0 {
                let origin = origin
                    .filter(|_| !saved.is_null())
                    .map(|p| POINT { x: p.x, y: p.y });
                let hwnd = self.window.hwnd() as HWND;
                if MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) != monitor {
                    let work = info.rcWork;
                    SetWindowPos(
                        hwnd,
                        std::ptr::null_mut(),
                        work.left + 1,
                        work.top + 1,
                        0,
                        0,
                        SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
                    );
                }
                self.ui.fit_window(info.rcWork, origin, size);
            }
        }
    }
    fn window_geometry(&self) -> Option<(PhysicalPosition<i32>, WindowSize)> {
        Some((self.window.outer_position().ok()?, self.ui.window_size()?))
    }
    fn remember_window(&mut self) -> Result<()> {
        if self.board_opened
            && self.window.is_visible()
            && !self.window.is_maximized()
            && !self.window.is_minimized()
            && let Some((p, size)) = self.window_geometry()
            && self.last_window != Some((p, size))
        {
            let changed_size = self.last_window.is_none_or(|(_, old)| old != size);
            self.layout_writer.submit(LayoutWrite::Position {
                x: p.x,
                y: p.y,
                size: changed_size.then_some(size),
            })?;
            self.last_window = Some((p, size));
        }
        Ok(())
    }
    fn board_data(&mut self) -> Result<()> {
        let snapshot = self.db.unchecked_transaction()?;
        let names = snapshot
            .prepare("SELECT App FROM ButtonBoards ORDER BY App")?
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        let boards = names
            .iter()
            .map(|name| board::read(&snapshot, name))
            .collect::<Result<Vec<_>>>()?;
        snapshot.commit()?;
        self.ui.layouts(boards)
    }
    fn board(&mut self, app: &str) -> Result<()> {
        self.cancel_automatic_paste();
        if !self.board_opened {
            let pos = self
                .db
                .query_row("SELECT X,Y FROM BoardWindow WHERE Id=1", [], |r| {
                    Ok(PhysicalPosition::new(r.get(0)?, r.get(1)?))
                })
                .optional()?;
            let size = self
                .db
                .query_row(
                    "SELECT Width,Height,Dpi FROM BoardWindowSize WHERE Id=1",
                    [],
                    |r| {
                        Ok(WindowSize {
                            width: r.get(0)?,
                            height: r.get(1)?,
                            dpi: r.get(2)?,
                        })
                    },
                )
                .optional()?;
            self.position_board(pos, size);
            self.last_window = self.window_geometry();
        }
        self.board_opened = true;
        self.ui.activate(app);
        self.board_data()?;
        self.show_launcher()
    }
    fn refresh(&mut self) -> Result<()> {
        self.ui.items(store::items(&self.db)?, &self.db);
        self.ui.layout()
    }
    fn message(&mut self, msg: &str) {
        self.shell.notify(msg);
    }
    fn open_editor(&mut self, id: Option<i64>, registration: bool) -> Result<()> {
        let bytes: Option<Vec<u8>> = if let Some(id) = id {
            self.db
                .query_row("SELECT Png FROM ButtonPreviews WHERE ItemId=?", [id], |r| {
                    r.get(0)
                })
                .optional()?
        } else {
            None
        };
        self.draft = None;
        let item = id.and_then(|id| self.ui.item(id));
        if registration || item.is_some() {
            self.dialogs
                .editor(item, bytes.as_deref(), ko(&self.settings), registration)?;
        }
        Ok(())
    }
    fn finish_editor(&mut self) {
        self.dialogs.close();
        self.registration = None;
        self.draft = None;
        self.ui.focus();
    }
    fn start_registration_save(
        &mut self,
        name: String,
        tags: String,
        shot: PreviewChange,
    ) -> Result<()> {
        let capture = Arc::clone(
            self.registration
                .as_ref()
                .context("등록할 클립이 없습니다.")?,
        );
        let preview = self.draft.clone();
        let path = self.dir.join("clips.db");
        let proxy = self.proxy.clone();
        thread::Builder::new()
            .name("clip-registration".into())
            .spawn(move || {
                let result = (|| {
                    let mut db = open_capture_db(&path)?;
                    store::register(
                        &mut db,
                        &capture.formats,
                        &capture.source,
                        capture.pointer,
                        store::ClipEdit {
                            name: &name,
                            tags: &tags,
                            preview: shot.edit(preview.as_deref())?,
                        },
                    )
                })();
                let _ = proxy.send_event(Event::Saved(
                    result.map_err(|e| format!("{e:#}")),
                    capture.source.clone(),
                    true,
                ));
            })?;
        self.busy = true;
        self.dialogs.set_saving(true);
        Ok(())
    }
    fn command(&mut self, cmd: Command) -> Result<()> {
        match cmd {
            Command::ToggleSidebar => {
                self.ui.toggle_sidebar(&self.db)?;
                self.window.set_min_inner_size(Some(LogicalSize::new(
                    self.ui.minimum_width() as f64,
                    native_ui::MIN_HEIGHT as f64,
                )));
                self.advance_sidebar()?;
            }
            Command::ShowSettings => self.dialogs.settings(&self.settings, ko(&self.settings))?,
            Command::ShowHelp => self.dialogs.help(&self.settings, ko(&self.settings))?,
            Command::CancelDialog => self.finish_editor(),
            Command::OpenEdit { id } => self.open_editor(Some(id), false)?,
            Command::MoveSlot { app, rev, id, to } => {
                self.layout_writer
                    .submit(LayoutWrite::Move { app, rev, id, to })?;
            }
            Command::ArrangeBoard { app, cols } => {
                self.layout_writer
                    .submit(LayoutWrite::Arrange { app, cols })?;
            }
            Command::GridGeometry {
                app,
                geometry: [x, y, dx, dy],
                viewport,
            } => {
                if !self.snipping && dx > 0. && dy > 0. {
                    let pos = self.window.inner_position()?;
                    let s = self.window.scale_factor();
                    self.layout_writer.submit(LayoutWrite::Measure {
                        app,
                        geometry: [
                            f64::from(pos.x) + x * s,
                            f64::from(pos.y) + y * s,
                            dx * s,
                            dy * s,
                        ],
                        viewport,
                    })?;
                }
            }
            Command::Snip => {
                if self.snipping {
                    return Ok(());
                }
                self.hide_launcher()?;
                self.snipping = true;
                let proxy = self.proxy.clone();
                let korean = ko(&self.settings);
                thread::spawn(move || {
                    let result = snip::select(korean).map_err(|e| e.to_string());
                    let _ = proxy.send_event(Event::Snipped(result));
                });
            }
            Command::Edit {
                id,
                name,
                tags,
                shot,
            } => {
                store::edit(
                    &mut self.db,
                    id,
                    store::ClipEdit {
                        name: &name,
                        tags: &tags,
                        preview: shot.edit(self.draft.as_deref())?,
                    },
                )?;
                self.finish_editor();
                self.refresh()?;
                self.board_data()?;
            }
            Command::Register { name, tags, shot } => {
                self.start_registration_save(name, tags, shot)?
            }
            Command::Delete { id } => {
                self.db
                    .execute("DELETE FROM ClipboardItems WHERE Id=?", [id])?;
                self.refresh()?;
                self.board_data()?;
            }
            Command::Clear => {
                self.db.execute("DELETE FROM ClipboardItems", [])?;
                self.refresh()?;
                self.board_data()?;
            }
            Command::Restore { id } => self.start_restore(id, self.target.is_some())?,
            Command::Settings { value } => {
                let old = self.settings.clone();
                let result = (|| -> Result<()> {
                    self.shell.bind(&value)?;
                    if old.startup != value.startup {
                        shell::startup(value.startup, Some(&self.dir))?;
                    }
                    self.db.execute(
                        "INSERT OR REPLACE INTO Preferences(Id,Json) VALUES(1,?)",
                        [serde_json::to_string(&value)?],
                    )?;
                    Ok(())
                })();
                if let Err(e) = result {
                    let _ = self.shell.bind(&old);
                    if old.startup != value.startup {
                        let _ = shell::startup(old.startup, Some(&self.dir));
                    }
                    return Err(e);
                }
                self.settings = value;
                self.dialogs.close();
                self.ui.set_language(ko(&self.settings));
                self.ui.set_tile_content(self.settings.tile_content);
                self.refresh()?;
                self.ui.focus();
            }
            Command::Hide => self.hide_launcher()?,
            Command::Quit => {
                let _ = self.proxy.send_event(Event::Quit);
            }
        }
        Ok(())
    }
    fn advance_sidebar(&mut self) -> Result<()> {
        if self.ui.advance_sidebar()? {
            self.window.set_min_inner_size(Some(LogicalSize::new(
                self.ui.minimum_width() as f64,
                native_ui::MIN_HEIGHT as f64,
            )));
            self.remember_window()?;
        }
        Ok(())
    }
    fn event(&mut self, e: Event) -> Result<()> {
        if self.dialogs.active() && matches!(e, Event::Capture | Event::Palette) {
            return Ok(());
        }
        if self.snipping && matches!(e, Event::Palette | Event::Open(_) | Event::Capture) {
            return Ok(());
        }
        match e {
            Event::SidebarFrame => self.advance_sidebar()?,
            Event::Arrange => {
                if !self.ui.is_sliding() {
                    self.ui.layout()?;
                    self.remember_window()?;
                }
            }
            Event::TilePreviews => self.ui.load_tile_previews(&self.db),
            Event::WindowMoved => {
                if self.board_opened && !self.snipping && !self.ui.is_sliding() {
                    self.remember_window()?;
                    self.ui.moved()?;
                }
            }
            Event::Restored(result) => {
                if let Some(job) = self
                    .restoring
                    .take()
                    .filter(|t| !t.cancelled.load(Ordering::Acquire))
                {
                    if result.map_err(anyhow::Error::msg)? {
                        self.hide_launcher()?;
                    } else {
                        self.ui.copy_succeeded(job.item_id);
                    }
                }
            }
            Event::Snipped(result) => {
                self.snipping = false;
                self.show_launcher()?;
                if let Some(bytes) = result.map_err(anyhow::Error::msg)? {
                    self.dialogs.captured(&bytes);
                    self.draft = Some(bytes.into());
                }
            }
            Event::Ui(Input::Dialog(h, id)) => {
                if !(self.snipping || self.busy && self.registration.is_some())
                    && self.dialogs.matches(h)
                    && let Some(cmd) =
                        self.dialogs
                            .command(id, &self.settings, ko(&self.settings))?
                {
                    self.command(cmd)?;
                }
            }
            Event::Ui(e) => {
                if !self.dialogs.active()
                    && let Some(cmd) = self.ui.action(e, &self.db)?
                {
                    self.command(cmd)?;
                }
            }
            Event::LayoutChanged(result) => {
                self.board_data()?;
                if let Some(app) = result.map_err(anyhow::Error::msg)?
                    && self.ui.active_program() == app
                {
                    self.ui.activate(&app);
                    self.ui.layout()?;
                    if self.window.is_visible() && !self.dialogs.active() {
                        self.ui.focus();
                    }
                }
            }
            Event::Open(settings) => self.open_current(settings)?,
            Event::Menu => {
                self.cancel_automatic_paste();
                self.shell.menu(ko(&self.settings))?;
            }
            Event::TrayReset => self.shell.add_icon()?,
            Event::Palette => {
                let src = clipboard::foreground();
                if src.pid != std::process::id() {
                    self.target = Some(src);
                }
                let app = self
                    .target
                    .as_ref()
                    .map(|src| src.app.clone())
                    .unwrap_or_default();
                self.board(&app)?;
            }
            Event::Capture => {
                ensure!(self.restoring.is_none(), "클립보드 복원이 진행 중입니다.");
                if self.busy {
                    return Ok(());
                }
                let src = clipboard::foreground();
                ensure!(
                    src.pid != std::process::id(),
                    "Select content in another application first"
                );
                self.busy = true;
                let mut cursor = POINT::default();
                let pointer =
                    (unsafe { GetCursorPos(&mut cursor) } != 0).then_some((cursor.x, cursor.y));
                let hwnd = self.window.hwnd();
                let path = self.dir.join("clips.db");
                let proxy = self.proxy.clone();
                let ask = self.settings.open_after;
                thread::spawn(move || {
                    let capture = Registration::capture(hwnd, src.clone(), pointer);
                    let event = if ask {
                        let result = capture.and_then(|draft| {
                            let db = Connection::open_with_flags(
                                path,
                                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
                            )?;
                            let existing = store::matching_clip(&db, &draft.formats)?;
                            Ok((draft, existing))
                        });
                        Event::Captured(result.map_err(|e| format!("{e:#}")))
                    } else {
                        let result = capture.and_then(|draft| {
                            let mut db = open_capture_db(&path)?;
                            store::save_near(&mut db, &draft.formats, &draft.source, draft.pointer)
                        });
                        Event::Saved(result.map_err(|e| format!("{e:#}")), src, false)
                    };
                    let _ = proxy.send_event(event);
                });
            }
            Event::Captured(result) => {
                self.busy = false;
                let (draft, existing) = result.map_err(anyhow::Error::msg)?;
                self.target = Some(draft.source.clone());
                self.board(&draft.source.app)?;
                self.open_editor(existing, true)?;
                self.registration = Some(Arc::new(draft));
            }
            Event::Saved(result, src, registered) => {
                self.busy = false;
                if registered {
                    self.dialogs.set_saving(false);
                }
                let id = result.map_err(anyhow::Error::msg)?;
                if registered {
                    self.finish_editor();
                }
                source_icon::remember(&self.db, &src.app, &src.exe)?;
                self.refresh()?;
                if registered {
                    let app = src.app.clone();
                    self.target = Some(src);
                    self.board(&app)?;
                    self.ui.focus_item(id)?;
                } else {
                    self.board_data()?;
                }
                if self.settings.notify {
                    self.message(if ko(&self.settings) {
                        "클립을 저장했습니다."
                    } else {
                        "Clip saved."
                    });
                }
            }
            Event::Quit | Event::LayoutDrained => {}
        }
        Ok(())
    }
}

fn open_capture_db(path: &Path) -> Result<Connection> {
    let db = Connection::open(path)?;
    db.execute_batch("PRAGMA foreign_keys=ON;")?;
    Ok(db)
}

fn load_settings(db: &Connection, legacy: Option<&Path>) -> Result<Settings> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS Preferences(Id INTEGER PRIMARY KEY, Json TEXT NOT NULL)",
    )?;
    if let Some(settings) = preferences::existing(db)? {
        return Ok(settings);
    }
    let mut s = Settings::default();
    if let Some(file) = legacy
        .and_then(Path::parent)
        .map(|p| p.join("settings.json"))
        && file.is_file()
    {
        let j: Value = serde_json::from_slice(&fs::read(file)?)?;
        s.open_after = j["openOverlayAfterCopy"].as_bool().unwrap_or(s.open_after);
        s.notify = j["showToastNotifications"].as_bool().unwrap_or(s.notify);
        s.language = j["language"].as_str().unwrap_or("system").into();
    }
    db.execute(
        "INSERT INTO Preferences VALUES(1,?)",
        [serde_json::to_string(&s)?],
    )?;
    Ok(s)
}
struct Instance(HANDLE);
impl Drop for Instance {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}

fn run() -> Result<()> {
    let mut args = env::args_os().skip(1);
    let appdata = PathBuf::from(env::var_os("APPDATA").context("APPDATA missing")?);
    let exe = env::current_exe()?;
    let parent = exe.parent().unwrap();
    let portable = parent.join("portable.flag").is_file();
    let mut dir = if portable {
        parent.join("portable-data-rust")
    } else {
        appdata.join("ClipEverythingRust")
    };
    let mut legacy = (!portable).then(|| appdata.join("ClipEverything/clips.db"));
    let mut background = false;
    let mut explicit_import = false;
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--profile") => {
                dir = args.next().context("Missing profile directory")?.into();
                if !explicit_import {
                    legacy = None;
                }
            }
            Some("--import") => {
                explicit_import = true;
                let path = PathBuf::from(args.next().context("Missing legacy DB path")?);
                ensure!(path.is_file(), "Legacy database does not exist");
                legacy = Some(path);
            }
            Some("--background") => background = true,
            Some("--unregister-startup") => return shell::remove_own_startup(),
            _ => bail!("Unknown argument {}", arg.to_string_lossy()),
        }
    }
    // Profiles have independent windows, data and single-instance locks.
    fs::create_dir_all(&dir)?;
    let dir = dir.canonicalize()?;
    use sha2::{Digest, Sha256};
    let profile = format!(
        "{:x}",
        Sha256::digest(dir.to_string_lossy().to_lowercase().as_bytes())
    );
    let title = format!("ClipEverything · {}", &profile[..8]);
    let handle = unsafe {
        CreateMutexW(
            std::ptr::null(),
            0,
            clipboard::wide(&format!("Local\\ClipEverythingRust-{profile}")).as_ptr(),
        )
    };
    ensure!(!handle.is_null(), "Cannot create instance lock");
    let _instance = Instance(handle);
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        unsafe {
            let name = clipboard::wide(&title);
            let mut hwnd = FindWindowW(name.as_ptr(), std::ptr::null());
            if hwnd.is_null() {
                hwnd = FindWindowW(std::ptr::null(), name.as_ptr());
            }
            if !hwnd.is_null() {
                PostMessageW(hwnd, shell::ACTIVATE, 0, 0);
            }
        }
        return Ok(());
    }
    if explicit_import {
        ensure!(
            !dir.join("clips.db").exists(),
            "Choose an empty profile to import a database"
        );
    }
    let db = store::open(&dir, legacy.as_deref())?;
    let mut settings = load_settings(&db, legacy.as_deref())?;
    if !portable
        && preferences::same_dir(&dir, &appdata.join("ClipEverythingRust"))
        && let Some(installed) =
            preferences::reg_string(preferences::INSTALL_KEY, "InstallLocation")?
        && preferences::same_dir(parent, Path::new(&installed))
        && let Some(options) = preferences::recipe()?
    {
        preferences::apply_once(&db, &mut settings, &options)?;
    }
    settings.startup = shell::owns_startup(Some(&dir))?;
    let initial_source = clipboard::foreground().app;
    let events = EventLoopBuilder::<Event>::with_user_event()
        .with_msg_hook(native_ui::translate)
        .build();
    let proxy = events.create_proxy();
    let layout_writer = LayoutWriter::start(&dir, proxy.clone())?;
    let window = WindowBuilder::new()
        .with_title("ClipEverything")
        .with_window_classname(title)
        .with_visible(false)
        .with_decorations(true)
        .with_resizable(true)
        .with_min_inner_size(LogicalSize::new(
            native_ui::MIN_WIDTH as f64,
            native_ui::MIN_HEIGHT as f64,
        ))
        .with_inner_size(LogicalSize::new(553., 276.))
        .build(&events)?;
    shell::set_window_icons(&window)?;
    unsafe {
        let host = window.hwnd() as HWND;
        let style = GetWindowLongPtrW(host, GWL_STYLE);
        SetWindowLongPtrW(host, GWL_STYLE, style | WS_CLIPCHILDREN as isize);
    }
    let shell = Shell::new(window.hwnd() as HWND, proxy.clone())?;
    let notice = shell
        .bind(&settings)
        .err()
        .map(|e| e.to_string())
        .unwrap_or_default();
    source_icon::import_existing(&db)?;
    let hwnd = window.hwnd() as HWND;
    let mut app = App {
        ui: NativeUi::new(hwnd, proxy.clone(), &db, ko(&settings))?,
        dialogs: Dialogs::new(hwnd, proxy.clone()),
        restoring: None,
        layout_writer,
        initial_source,
        snipping: false,
        registration: None,
        draft: None,
        board_opened: false,
        last_window: None,
        window,
        shell,
        db,
        dir,
        settings,
        target: None,
        busy: false,
        proxy,
    };
    app.window.set_min_inner_size(Some(LogicalSize::new(
        app.ui.minimum_width() as f64,
        native_ui::MIN_HEIGHT as f64,
    )));
    app.ui.set_tile_content(app.settings.tile_content);
    app.refresh()?;
    if !background {
        app.open_current(false)?;
    }
    if !notice.is_empty() {
        app.message(&notice);
    }
    let mut quitting_at = None;
    let mut layout_drained = false;
    events.run(move |event, _, flow| {
        *flow = ControlFlow::Wait;
        match event {
            WindowEventLoop::UserEvent(Event::LayoutDrained) => layout_drained = true,
            WindowEventLoop::UserEvent(Event::Restored(_)) if quitting_at.is_some() => {
                app.restoring = None
            }
            WindowEventLoop::UserEvent(Event::Saved(..) | Event::Captured(_))
                if quitting_at.is_some() =>
            {
                app.busy = false;
            }
            _ if quitting_at.is_some() => {}
            WindowEventLoop::UserEvent(Event::Quit) => {
                if let Some(job) = &app.restoring {
                    job.cancelled.store(true, Ordering::Release);
                }
                if !app.ui.is_sliding()
                    && let Err(error) = app.remember_window()
                {
                    app.message(&error.to_string());
                }
                quitting_at = Some(std::time::Instant::now() + std::time::Duration::from_secs(5));
                layout_drained = !app.layout_writer.finish();
            }
            WindowEventLoop::UserEvent(e) => {
                if let Err(err) = app.event(e) {
                    if app.dialogs.active() {
                        app.dialogs.error(&format!("{err:#}"));
                    } else {
                        app.message(&format!("{err:#}"));
                    }
                }
            }
            WindowEventLoop::WindowEvent {
                event: WindowEvent::ScaleFactorChanged { .. },
                ..
            } => {
                if let Err(err) = shell::set_window_icons(&app.window) {
                    app.message(&err.to_string());
                }
                let _ = app.proxy.send_event(Event::Arrange);
            }
            WindowEventLoop::WindowEvent {
                event: WindowEvent::Resized(_),
                ..
            } => {
                let _ = app.proxy.send_event(Event::Arrange);
            }
            WindowEventLoop::WindowEvent {
                event: WindowEvent::Moved(_),
                ..
            } => {
                let _ = app.proxy.send_event(Event::WindowMoved);
            }
            WindowEventLoop::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                if app.settings.close_to_tray {
                    if let Err(e) = app.hide_launcher() {
                        app.message(&e.to_string());
                    }
                } else {
                    let _ = app.proxy.send_event(Event::Quit);
                }
            }
            _ => {}
        }
        if let Some(deadline) = quitting_at
            && layout_drained
        {
            // Let an active transfer finish without making Quit depend indefinitely on a foreign clipboard owner.
            *flow = if (app.restoring.is_none() && !app.busy)
                || std::time::Instant::now() >= deadline
            {
                ControlFlow::Exit
            } else {
                ControlFlow::WaitUntil(deadline)
            };
        }
    })
}
fn main() {
    if let Err(error) = run() {
        unsafe {
            MessageBoxW(
                std::ptr::null_mut(),
                clipboard::wide(&format!("{error:#}")).as_ptr(),
                clipboard::wide("ClipEverything").as_ptr(),
                MB_ICONERROR,
            );
        }
        std::process::exit(1);
    }
}
