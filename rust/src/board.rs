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
#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Axis {
    Row,
    Column,
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
        CREATE TABLE IF NOT EXISTS BoardViewColumns(App TEXT PRIMARY KEY REFERENCES ButtonBoards(App) ON DELETE CASCADE, Cols INTEGER NOT NULL);
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
    let display_cols = tx
        .query_row(
            "SELECT Cols FROM BoardViewColumns WHERE App=?",
            [app],
            |r| r.get::<_, i64>(0),
        )
        .optional()?
        .unwrap_or(board.cols)
        .max(1);
    let full = occupied.len() as i64 == board.rows * board.cols;
    if full {
        tx.execute("UPDATE ButtonBoards SET Rows=Rows+1 WHERE App=?", [app])?;
    }
    let mut best = None;
    for row in (if full { board.rows } else { 0 })..board.rows + i64::from(full) {
        for col in 0..board.cols {
            if occupied.contains(&(row, col)) {
                continue;
            }
            let score = match (pointer, geometry) {
                (Some((x, y)), Some((ox, oy, dx, dy))) => {
                    let index = row * board.cols + col;
                    (ox + (index % display_cols) as f64 * dx - f64::from(x)).powi(2)
                        + (oy + (index / display_cols) as f64 * dy - f64::from(y)).powi(2)
                }
                _ => 0.0,
            };
            if best.is_none_or(|(old, _, _)| score < old) {
                best = Some((score, row, col));
            }
        }
    }
    let (_, row, col) = best.expect("one free cell after expansion");
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
pub fn measure(db: &mut Connection, app: &str, geometry: [f64; 4]) -> Result<bool> {
    measure_view(db, app, geometry, None)
}
pub fn measure_view(
    db: &mut Connection,
    app: &str,
    geometry: [f64; 4],
    columns: Option<i64>,
) -> Result<bool> {
    let tx = db.transaction()?;
    if let Some(cols) = columns.filter(|c| *c > 0) {
        tx.execute(
            "INSERT OR REPLACE INTO BoardViewColumns VALUES(?,?)",
            params![app, cols],
        )?;
    }
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
pub fn move_slot(db: &mut Connection, app: &str, rev: i64, id: i64, to: Cell) -> Result<()> {
    let tx = db.transaction()?;
    let b = load(&tx, app)?;
    check_revision(&b, rev)?;
    ensure!(
        (0..b.rows).contains(&to.row) && (0..b.cols).contains(&to.col),
        "Invalid destination"
    );
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
    bump(&tx, app)?;
    tx.commit()?;
    Ok(())
}
pub fn resize(
    db: &mut Connection,
    app: &str,
    rev: i64,
    axis: Axis,
    index: i64,
    add: bool,
    confirmed: bool,
) -> Result<()> {
    let tx = db.transaction()?;
    let b = load(&tx, app)?;
    check_revision(&b, rev)?;
    let (size, col, field) = match axis {
        Axis::Row => (b.rows, "RowNo", "Rows"),
        Axis::Column => (b.cols, "ColNo", "Cols"),
    };
    ensure!(
        index >= 0 && (index < size || (add && index == size)),
        "Invalid index"
    );
    if !add {
        ensure!(size > 1, "At least one row and column are required");
        let occupied=b.slots.iter().any(|s|match axis{Axis::Row=>s.cell.row,Axis::Column=>s.cell.col}==index);
        ensure!(!occupied || confirmed, "Occupied line needs confirmation");
        tx.execute(
            &format!("DELETE FROM ButtonSlots WHERE App=? AND {col}=?"),
            params![app, index],
        )?;
    }
    // Stage shifted cells in the negative range to avoid transient UNIQUE collisions.
    let shift = if add { 1 } else { -1 };
    tx.execute(
        &format!("UPDATE ButtonSlots SET {col}=-{col}-1 WHERE App=? AND {col}>=?"),
        params![app, index],
    )?;
    tx.execute(
        &format!("UPDATE ButtonSlots SET {col}=-{col}-1+? WHERE App=? AND {col}<0"),
        params![shift, app],
    )?;
    tx.execute(
        &format!("UPDATE ButtonBoards SET {field}={field}+?,Revision=Revision+1 WHERE App=?"),
        params![shift, app],
    )?;
    tx.commit()?;
    Ok(())
}
