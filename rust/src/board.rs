use anyhow::{Result, ensure};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct Cell {
    pub row: i64,
    pub col: i64,
}
#[derive(Serialize, Debug)]
pub struct Slot {
    pub id: i64,
    #[serde(flatten)]
    pub cell: Cell,
    pub shot: i64,
}
#[derive(Serialize)]
pub struct Board {
    pub app: String,
    pub rows: i64,
    pub cols: i64,
    pub revision: i64,
    pub slots: Vec<Slot>,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Viewport {
    pub origin: Cell,
    pub rows: i64,
    pub cols: i64,
}
impl Default for Viewport {
    fn default() -> Self {
        Self {
            origin: Cell { row: 0, col: 0 },
            rows: 3,
            cols: 4,
        }
    }
}

impl Board {
    pub fn occupied_size(&self) -> (i64, i64) {
        self.slots.iter().fold((0, 0), |(cols, rows), slot| {
            (cols.max(slot.cell.col + 1), rows.max(slot.cell.row + 1))
        })
    }
}

pub fn init(db: &Connection) -> Result<()> {
    let tx = db.unchecked_transaction()?;
    let installed = tx.table_exists(None, "ButtonBoards")?;
    tx.execute_batch("CREATE TABLE IF NOT EXISTS ButtonBoards(
        App TEXT PRIMARY KEY, Rows INTEGER NOT NULL DEFAULT 3, Cols INTEGER NOT NULL DEFAULT 4,
        Revision INTEGER NOT NULL DEFAULT 0, OriginX REAL,OriginY REAL,StepX REAL,StepY REAL);
        CREATE TABLE IF NOT EXISTS ButtonSlots(App TEXT NOT NULL REFERENCES ButtonBoards(App) ON DELETE CASCADE,
        ItemId INTEGER NOT NULL REFERENCES ClipboardItems(Id) ON DELETE CASCADE,RowNo INTEGER NOT NULL,ColNo INTEGER NOT NULL,
        PRIMARY KEY(App,ItemId),UNIQUE(App,RowNo,ColNo));
        CREATE INDEX IF NOT EXISTS button_slot_item ON ButtonSlots(ItemId);
        CREATE TABLE IF NOT EXISTS PendingButtonPositions(App TEXT NOT NULL,ItemId INTEGER NOT NULL,X INTEGER,Y INTEGER,
        PRIMARY KEY(App,ItemId),FOREIGN KEY(App,ItemId) REFERENCES ButtonSlots(App,ItemId) ON DELETE CASCADE);
        CREATE TABLE IF NOT EXISTS ButtonPreviews(ItemId INTEGER PRIMARY KEY REFERENCES ClipboardItems(Id) ON DELETE CASCADE,
        Png BLOB NOT NULL,Revision INTEGER NOT NULL DEFAULT 1);
        CREATE TABLE IF NOT EXISTS BoardViewport(
        App TEXT PRIMARY KEY REFERENCES ButtonBoards(App) ON DELETE CASCADE,
        FirstRow INTEGER NOT NULL, FirstCol INTEGER NOT NULL, Rows INTEGER NOT NULL, Cols INTEGER NOT NULL);
        CREATE TABLE IF NOT EXISTS BoardWindow(Id INTEGER PRIMARY KEY,X INTEGER,Y INTEGER);")?;
    if !installed {
        tx.execute_batch("INSERT INTO ButtonBoards(App,Rows,Cols) SELECT SourceApp,max(3,(count(*)+3)/4),4 FROM ClipboardItems GROUP BY SourceApp;
          INSERT INTO ButtonSlots SELECT SourceApp,Id,pos/4,pos%4 FROM
          (SELECT SourceApp,Id,row_number() OVER(PARTITION BY SourceApp ORDER BY LastCopiedAt DESC,Id DESC)-1 AS pos FROM ClipboardItems);")?;
    }
    tx.commit()?;
    Ok(())
}
pub fn load(db: &Connection, app: &str) -> Result<Board> {
    db.execute(
        "INSERT OR IGNORE INTO ButtonBoards(App,Rows,Cols) VALUES(?,3,4)",
        [app],
    )?;
    read(db, app)
}

pub fn read(db: &Connection, app: &str) -> Result<Board> {
    let (rows, cols, revision) = db.query_row(
        "SELECT Rows,Cols,Revision FROM ButtonBoards WHERE App=?",
        [app],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    let mut q=db.prepare("SELECT s.ItemId,s.RowNo,s.ColNo,coalesce(p.Revision,0) FROM ButtonSlots s LEFT JOIN ButtonPreviews p ON p.ItemId=s.ItemId WHERE s.App=? ORDER BY s.RowNo,s.ColNo")?;
    let slots = q
        .query_map([app], |r| {
            Ok(Slot {
                id: r.get(0)?,
                cell: Cell {
                    row: r.get(1)?,
                    col: r.get(2)?,
                },
                shot: r.get(3)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(Board {
        app: app.into(),
        rows,
        cols,
        revision,
        slots,
    })
}
fn bump(db: &Connection, app: &str) -> Result<()> {
    db.execute(
        "UPDATE ButtonBoards SET Revision=Revision+1 WHERE App=?",
        [app],
    )?;
    Ok(())
}
pub fn place(tx: &Transaction, app: &str, id: i64, pointer: Option<(i32, i32)>) -> Result<()> {
    let board = load(tx, app)?;
    if board.slots.iter().any(|s| s.id == id) {
        return Ok(());
    }
    let geometry=tx.query_row("SELECT OriginX,OriginY,StepX,StepY FROM ButtonBoards WHERE App=? AND StepX>0 AND StepY>0",[app],|r|Ok((r.get::<_,f64>(0)?,r.get::<_,f64>(1)?,r.get::<_,f64>(2)?,r.get::<_,f64>(3)?))).optional()?;
    let occupied: HashSet<_> = board
        .slots
        .iter()
        .map(|s| (s.cell.row, s.cell.col))
        .collect();
    let view = tx
        .query_row(
            "SELECT FirstRow,FirstCol,Rows,Cols FROM BoardViewport WHERE App=?",
            [app],
            |r| {
                Ok(Viewport {
                    origin: Cell {
                        row: r.get(0)?,
                        col: r.get(1)?,
                    },
                    rows: r.get(2)?,
                    cols: r.get(3)?,
                })
            },
        )
        .optional()?
        .unwrap_or_default();
    let end_row = view.origin.row + view.rows;
    let end_col = view.origin.col + view.cols;
    let mut best = None;
    for row in view.origin.row..end_row {
        for col in view.origin.col..end_col {
            if occupied.contains(&(row, col)) {
                continue;
            }
            let score = match (pointer, geometry) {
                (Some((x, y)), Some((ox, oy, dx, dy))) => {
                    (ox + (col - view.origin.col) as f64 * dx - f64::from(x)).powi(2)
                        + (oy + (row - view.origin.row) as f64 * dy - f64::from(y)).powi(2)
                }
                _ => 0.0,
            };
            if best.is_none_or(|(old, _, _)| score < old) {
                best = Some((score, row, col));
            }
        }
    }
    let (_, row, col) = best.unwrap_or_else(|| {
        let (row, col) = (end_row..)
            .flat_map(|row| (view.origin.col..end_col).map(move |col| (row, col)))
            .find(|cell| !occupied.contains(cell))
            .expect("finite board has an available cell below the viewport");
        (0., row, col)
    });
    extend(tx, app, Cell { row, col })?;
    tx.execute(
        "INSERT INTO ButtonSlots(App,ItemId,RowNo,ColNo) VALUES(?,?,?,?)",
        params![app, id, row, col],
    )?;
    if geometry.is_none()
        && let Some((x, y)) = pointer
    {
        tx.execute(
            "INSERT INTO PendingButtonPositions VALUES(?,?,?,?)",
            params![app, id, x, y],
        )?;
    }
    bump(tx, app)
}
pub fn measure(db: &mut Connection, app: &str, geometry: [f64; 4], view: Viewport) -> Result<bool> {
    let tx = db.transaction()?;
    tx.execute(
        "INSERT OR REPLACE INTO BoardViewport VALUES(?,?,?,?,?)",
        params![app, view.origin.row, view.origin.col, view.rows, view.cols],
    )?;
    let [x, y, dx, dy] = geometry;
    tx.execute(
        "UPDATE ButtonBoards SET OriginX=?,OriginY=?,StepX=?,StepY=? WHERE App=?",
        params![x, y, dx, dy, app],
    )?;
    let pending = tx
        .prepare("SELECT ItemId,X,Y FROM PendingButtonPositions WHERE App=? ORDER BY rowid")?
        .query_map([app], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i32>(1)?,
                r.get::<_, i32>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    if !pending.is_empty() {
        tx.execute("DELETE FROM ButtonSlots WHERE App=?1 AND ItemId IN (SELECT ItemId FROM PendingButtonPositions WHERE App=?1)",[app])?;
        for &(id, px, py) in &pending {
            place(&tx, app, id, Some((px, py)))?;
        }
    }
    tx.commit()?;
    Ok(!pending.is_empty())
}
fn check_revision(b: &Board, rev: i64) -> Result<()> {
    ensure!(b.revision == rev, "Board changed. Try again.");
    Ok(())
}
fn extend(db: &Connection, app: &str, cell: Cell) -> Result<()> {
    db.execute(
        "UPDATE ButtonBoards SET Rows=max(Rows,?),Cols=max(Cols,?) WHERE App=?",
        params![cell.row + 1, cell.col + 1, app],
    )?;
    Ok(())
}
pub fn move_slot(db: &mut Connection, app: &str, rev: i64, id: i64, to: Cell) -> Result<()> {
    let tx = db.transaction()?;
    let b = load(&tx, app)?;
    check_revision(&b, rev)?;
    ensure!(to.row >= 0 && to.col >= 0, "Invalid destination");
    let from = b
        .slots
        .iter()
        .find(|s| s.id == id)
        .ok_or_else(|| anyhow::anyhow!("Button is no longer on this board"))?
        .cell;
    tx.execute(
        "DELETE FROM ButtonSlots WHERE App=? AND ItemId=?",
        params![app, id],
    )?;
    tx.execute(
        "UPDATE ButtonSlots SET RowNo=?,ColNo=? WHERE App=? AND RowNo=? AND ColNo=?",
        params![from.row, from.col, app, to.row, to.col],
    )?;
    tx.execute(
        "INSERT INTO ButtonSlots VALUES(?,?,?,?)",
        params![app, id, to.row, to.col],
    )?;
    extend(&tx, app, to)?;
    bump(&tx, app)?;
    tx.commit()?;
    Ok(())
}
pub fn arrange(db: &mut Connection, app: &str, cols: i64) -> Result<()> {
    ensure!(cols > 0, "Invalid column count");
    let tx = db.transaction()?;
    let ids = tx
        .prepare(
            "SELECT i.Id FROM ClipboardItems i
             LEFT JOIN ButtonSlots s ON s.App=?1 AND s.ItemId=i.Id
             WHERE s.ItemId IS NOT NULL OR i.SourceApp=?1 COLLATE NOCASE
             ORDER BY s.ItemId IS NULL,s.RowNo,s.ColNo,i.LastCopiedAt DESC,i.Id DESC",
        )?
        .query_map([app], |r| r.get::<_, i64>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    tx.execute("DELETE FROM ButtonSlots WHERE App=?", [app])?;
    {
        let mut insert = tx.prepare("INSERT INTO ButtonSlots VALUES(?,?,?,?)")?;
        for (n, id) in ids.iter().enumerate() {
            let n = n as i64;
            insert.execute(params![app, id, n / cols, n % cols])?;
        }
    }
    tx.execute(
        "UPDATE ButtonBoards SET Rows=?,Cols=?,Revision=Revision+1 WHERE App=?",
        params![((ids.len() as i64 + cols - 1) / cols).max(1), cols, app],
    )?;
    tx.commit()?;
    Ok(())
}
