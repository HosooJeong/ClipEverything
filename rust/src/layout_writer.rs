//! Keep SQLite write waits out of the Win32 message loop and preserve the order
//! of drops, auto arrange requests and viewport measurements.
use crate::Event;
use anyhow::{Context, Result};
use clip_everything::board::{self, Cell, Viewport};
use rusqlite::{Connection, OpenFlags, params};
use std::{
    path::Path,
    sync::mpsc::{self, Sender},
    thread,
    time::Duration,
};
use tao::event_loop::EventLoopProxy;

pub enum LayoutWrite {
    Move {
        app: String,
        rev: i64,
        id: i64,
        to: Cell,
    },
    Arrange {
        app: String,
        cols: i64,
    },
    Measure {
        app: String,
        geometry: [f64; 4],
        viewport: Viewport,
    },
    Position {
        x: i32,
        y: i32,
    },
}

impl LayoutWrite {
    fn replaces(&self, previous: &Self) -> bool {
        match (self, previous) {
            (Self::Position { .. }, Self::Position { .. }) => true,
            (Self::Measure { app, .. }, Self::Measure { app: old, .. }) => app == old,
            _ => false,
        }
    }
    fn apply(self, db: &mut Connection) -> Result<Option<Event>> {
        match self {
            Self::Move { app, rev, id, to } => {
                board::move_slot(db, &app, rev, id, to)?;
                Ok(Some(Event::LayoutChanged(Ok(None))))
            }
            Self::Arrange { app, cols } => {
                board::arrange(db, &app, cols)?;
                Ok(Some(Event::LayoutChanged(Ok(Some(app)))))
            }
            Self::Measure {
                app,
                geometry,
                viewport,
            } => Ok(board::measure(db, &app, geometry, viewport)?
                .then_some(Event::LayoutChanged(Ok(None)))),
            Self::Position { x, y } => {
                db.execute(
                    "INSERT OR REPLACE INTO BoardWindow VALUES(1,?,?)",
                    params![x, y],
                )?;
                Ok(None)
            }
        }
    }
}
pub struct LayoutWriter(Sender<Option<LayoutWrite>>);
impl LayoutWriter {
    pub fn start(dir: &Path, proxy: EventLoopProxy<Event>) -> Result<Self> {
        let mut db =
            Connection::open_with_flags(dir.join("clips.db"), OpenFlags::SQLITE_OPEN_READ_WRITE)?;
        db.busy_timeout(Duration::from_secs(3))?;
        db.execute_batch("PRAGMA foreign_keys=ON;")?;
        let (send, recv) = mpsc::channel::<Option<LayoutWrite>>();
        thread::Builder::new()
            .name("button-layout".into())
            .spawn(move || {
                let mut batch: Vec<LayoutWrite> = Vec::new();
                while let Ok(first) = recv.recv() {
                    let mut finishing = false;
                    for next in std::iter::once(first).chain(recv.try_iter()) {
                        let Some(next) = next else {
                            finishing = true;
                            break;
                        };
                        if let Some(previous) = batch.last_mut()
                            && next.replaces(previous)
                        {
                            *previous = next;
                        } else {
                            batch.push(next);
                        }
                    }
                    for task in batch.drain(..) {
                        let event = match task.apply(&mut db) {
                            Ok(None) => continue,
                            Ok(Some(event)) => event,
                            Err(e) => Event::LayoutChanged(Err(e.to_string())),
                        };
                        if proxy.send_event(event).is_err() {
                            return;
                        }
                    }
                    if finishing {
                        let _ = proxy.send_event(Event::LayoutDrained);
                        return;
                    }
                }
            })?;
        Ok(Self(send))
    }
    pub fn submit(&self, task: LayoutWrite) -> Result<()> {
        self.0
            .send(Some(task))
            .ok()
            .context("버튼 배치 저장 작업에 연결할 수 없습니다.")
    }
    pub fn finish(&self) -> bool {
        self.0.send(None).is_ok()
    }
}
