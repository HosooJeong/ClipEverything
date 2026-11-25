use clip_everything::store::{self, Format, Source};
use std::{fs, path::PathBuf};
fn dir(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("clip-rust-test-{}-{name}", std::process::id()))
}
fn text(value: &str) -> Format {
    Format {
        id: 13,
        name: "UnicodeText".into(),
        data: value
            .encode_utf16()
            .chain([0])
            .flat_map(u16::to_le_bytes)
            .collect(),
    }
}
#[test]
fn legacy_backup_includes_uncheckpointed_records_and_preserves_annotations() -> anyhow::Result<()> {
    let root = dir("legacy");
    let old = store::open(&root.join("old"), None)?;
    old.execute_batch("PRAGMA wal_autocheckpoint=0;ALTER TABLE ClipboardItems DROP COLUMN PreviewText;
        INSERT INTO ClipboardItems(Name,IsFavorite,Tags,CreatedAt,LastCopiedAt) VALUES('기획',1,'업무','2026','2026');
        INSERT INTO ClipboardFormats(ItemId,FormatId,FormatName,Data) VALUES(1,13,'UnicodeText',X'41000000');")?;
    let imported = store::open(&root.join("new"), Some(&root.join("old/clips.db")))?;
    let row = store::items(&imported)?.remove(0);
    assert_eq!((row.name.as_str(), row.tags.as_str()), ("기획", "업무"));
    assert!(imported.query_row(
        "SELECT IsFavorite FROM ClipboardItems WHERE Id=?",
        [row.id],
        |r| r.get::<_, bool>(0)
    )?);
    assert_eq!(store::formats(&imported, row.id)?, vec![text("A")]);
    imported.execute("DELETE FROM ClipboardItems", [])?;
    assert_eq!(
        old.query_row("SELECT count(*) FROM ClipboardItems", [], |r| r
            .get::<_, i32>(0))?,
        1
    );
    assert!(store::formats(&imported, row.id)?.is_empty());
    drop((imported, old));
    fs::remove_dir_all(root)?;
    Ok(())
}
#[test]
fn rich_formats_keep_payload_and_ignore_registered_number_changes() -> anyhow::Result<()> {
    let root = dir("rich");
    let mut db = store::open(&root, None)?;
    let src = Source::default();
    let plain = text("합계");
    let one = store::save(&mut db, std::slice::from_ref(&plain), &src)?;
    let mut html = Format {
        id: 0xc221,
        name: "HTML Format".into(),
        data: b"<b>sum</b>".to_vec(),
    };
    let rich = store::save(&mut db, &[plain.clone(), html.clone()], &src)?;
    assert_ne!(one, rich);
    html.id = 0xc333;
    assert_eq!(rich, store::save(&mut db, &[html, plain], &src)?);
    assert_eq!(store::items(&db)?.len(), 2);
    drop(db);
    fs::remove_dir_all(root)?;
    Ok(())
}
#[test]
fn rollback_and_retention_protect_history() -> anyhow::Result<()> {
    let root = dir("retention");
    let mut db = store::open(&root, None)?;
    let src = Source::default();
    db.execute_batch("CREATE TRIGGER fail BEFORE INSERT ON ClipboardFormats BEGIN SELECT RAISE(ABORT,'test'); END;")?;
    assert!(store::save(&mut db, &[text("rollback")], &src).is_err());
    assert!(store::items(&db)?.is_empty());
    db.execute_batch("DROP TRIGGER fail;WITH RECURSIVE n(i) AS (VALUES(1) UNION ALL SELECT i+1 FROM n WHERE i<1002) INSERT INTO ClipboardItems(Id,CreatedAt,LastCopiedAt,IsFavorite) SELECT i,'2000','2000',i=1 FROM n;")?;
    store::save(&mut db, &[text("새 클립")], &src)?;
    let rows = store::items(&db)?;
    assert_eq!(rows.len(), 1001);
    assert!(rows.iter().any(|r| r.id == 1));
    drop(db);
    fs::remove_dir_all(root)?;
    Ok(())
}
