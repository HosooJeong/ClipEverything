use clip_everything::board::{self, Cell, Viewport};
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
fn arrange_in_viewport_order_restores_offscreen_and_unplaced_clips() -> anyhow::Result<()> {
    let mut db = database()?;
    let b = board::load(&db, "Editor")?;
    let far = Cell { row: 4, col: 6 };
    board::move_slot(&mut db, "Editor", b.revision, 1, far)?;
    db.execute_batch("INSERT INTO ClipboardItems VALUES(3,'Editor','2027'),(4,'Other','2028');")?;
    // A clip reused in a different source can still have a slot on this board.
    db.execute("UPDATE ClipboardItems SET SourceApp='Other' WHERE Id=1", [])?;
    let b = board::load(&db, "Editor")?;
    assert_eq!(b.occupied_size(), (7, 5));
    assert!(!board::measure(
        &mut db,
        "Editor",
        [0., 0., 50., 50.],
        Viewport {
            cols: 2,
            rows: 1,
            ..Viewport::default()
        }
    )?);
    assert_eq!(board::read(&db, "Editor")?.slots[1].cell, far);
    board::arrange(&mut db, "Editor", 2)?;
    let packed = board::read(&db, "Editor")?;
    assert_eq!((packed.cols, packed.rows), (2, 2));
    assert_eq!(
        packed
            .slots
            .iter()
            .map(|s| (s.id, s.cell))
            .collect::<Vec<_>>(),
        [
            (2, Cell { row: 0, col: 0 }),
            (1, Cell { row: 0, col: 1 }),
            (3, Cell { row: 1, col: 0 })
        ],
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM ClipboardItems", [], |r| r
            .get::<_, i64>(0))?,
        4
    );
    Ok(())
}
#[test]
fn first_capture_resolves_pointer_on_first_display_and_retains_position() -> anyhow::Result<()> {
    let mut db = database()?;
    let tx = db.transaction()?;
    board::place(&tx, "Other", 1, Some((-15, 130)))?;
    tx.commit()?;
    assert!(board::measure(
        &mut db,
        "Other",
        [-120., 40., 100., 30.],
        Viewport::default()
    )?);
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
fn registration_respects_the_visible_page_and_fills_overflow_rows() -> anyhow::Result<()> {
    let mut db = database()?;
    board::measure(
        &mut db,
        "Editor",
        [100., 100., 50., 50.],
        Viewport {
            origin: Cell { row: 2, col: 4 },
            rows: 1,
            cols: 2,
        },
    )?;
    let tx = db.transaction()?;
    for id in 3..=6 {
        tx.execute(
            "INSERT INTO ClipboardItems VALUES(?,'Editor','today')",
            [id],
        )?;
        board::place(&tx, "Editor", id, Some((150, 100)))?;
    }
    tx.commit()?;
    let b = board::read(&db, "Editor")?;
    assert_eq!(
        b.slots[2..]
            .iter()
            .map(|s| (s.id, s.cell))
            .collect::<Vec<_>>(),
        [
            (4, Cell { row: 2, col: 4 }),
            (3, Cell { row: 2, col: 5 }),
            (5, Cell { row: 3, col: 4 }),
            (6, Cell { row: 3, col: 5 })
        ],
    );
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
