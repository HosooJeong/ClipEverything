use clip_everything::board::{self, Axis, Cell};
use rusqlite::{Connection, params};
fn database() -> anyhow::Result<Connection> {
    let db = Connection::open_in_memory()?;
    db.execute_batch("PRAGMA foreign_keys=ON; CREATE TABLE ClipboardItems(Id INTEGER PRIMARY KEY,SourceApp TEXT,LastCopiedAt TEXT); INSERT INTO ClipboardItems VALUES(1,'Editor','2026'),(2,'Editor','2025');")?;
    board::init(&db)?;
    Ok(db)
}
#[test]
fn display_snapshot_never_writes_to_registered_boards() -> anyhow::Result<()> {
    let db = database()?;
    db.execute_batch("PRAGMA query_only=ON")?;
    let snapshot = db.unchecked_transaction()?;
    let board = board::read(&snapshot, "Editor")?;
    assert_eq!((board.rows, board.cols, board.slots.len()), (3, 4, 2));
    snapshot.commit()?;
    Ok(())
}
#[test]
fn resize_warns_and_swaps_without_losing_clips() -> anyhow::Result<()> {
    let mut db = database()?;
    let b = board::load(&db, "Editor")?;
    assert_eq!((b.rows, b.cols, b.slots.len()), (3, 4, 2));
    board::move_slot(&mut db, "Editor", b.revision, 1, Cell { row: 0, col: 1 })?;
    let b = board::load(&db, "Editor")?;
    assert_eq!(b.slots[0].id, 2);
    assert!(board::resize(&mut db, "Editor", b.revision, Axis::Row, 0, false, false).is_err());
    board::resize(&mut db, "Editor", b.revision, Axis::Column, 0, true, false)?;
    let b = board::load(&db, "Editor")?;
    assert_eq!(b.slots[1].cell.col, 2);
    assert!(board::resize(&mut db, "Editor", b.revision, Axis::Column, 2, false, false).is_err());
    board::resize(&mut db, "Editor", b.revision, Axis::Column, 2, false, true)?;
    assert!(board::resize(&mut db, "Editor", b.revision, Axis::Row, 0, false, true).is_err());
    board::init(&db)?;
    assert_eq!(board::load(&db, "Editor")?.slots.len(), 1);
    assert_eq!(
        db.query_row("SELECT count(*) FROM ClipboardItems", [], |r| r
            .get::<_, i64>(0))?,
        2
    );
    Ok(())
}
#[test]
fn first_capture_resolves_pointer_on_first_display_and_retains_position() -> anyhow::Result<()> {
    let mut db = database()?;
    let tx = db.transaction()?;
    board::place(&tx, "Other", 1, Some((-15, 130)))?;
    tx.commit()?;
    assert!(board::measure(&mut db, "Other", [-120., 40., 100., 30.])?);
    let slot = board::load(&db, "Other")?.slots.remove(0);
    assert_eq!(slot.cell, Cell { row: 2, col: 1 });
    let tx = db.transaction()?;
    board::place(&tx, "Other", 1, Some((-120, 40)))?;
    board::place(&tx, "Other", 2, Some((-15, 130)))?;
    tx.commit()?;
    let slots = board::load(&db, "Other")?.slots;
    assert_eq!(slots[0].cell, Cell { row: 1, col: 1 });
    assert_eq!(slots[1].cell, slot.cell);
    assert_eq!(board::load(&db, "Editor")?.slots.len(), 2);
    Ok(())
}
#[test]
fn full_board_grows_without_overwriting_a_button() -> anyhow::Result<()> {
    let mut db = database()?;
    let tx = db.transaction()?;
    for i in 3..=13 {
        tx.execute(
            "INSERT INTO ClipboardItems VALUES(?,'Editor','today')",
            params![i],
        )?;
        board::place(&tx, "Editor", i, None)?;
    }
    tx.commit()?;
    let board = board::load(&db, "Editor")?;
    assert_eq!((board.slots.len(), board.rows), (13, 4));
    Ok(())
}
