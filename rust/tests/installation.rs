use anyhow::Result;
use clip_everything::preferences::{self, InstallChoices, Settings, Shortcut};
use rusqlite::Connection;
#[test]
fn installer_accepts_the_same_shortcuts_as_the_app() -> Result<()> {
    let shortcut = Shortcut::parse("Shift + Alt + k")?;
    assert_eq!(shortcut.label(), "Alt+Shift+K");
    for bad in [
        "K",
        "Ctrl++K",
        "Win+Ctrl+;",
        "Ctrl+Ctrl+H",
        "Alt+F12",
        "Meta+K",
    ] {
        assert!(Shortcut::parse(bad).is_err(), "{bad}");
    }
    Ok(())
}
#[test]
fn setup_is_applied_once_and_an_upgrade_preserves_other_preferences() -> Result<()> {
    let db = Connection::open_in_memory()?;
    db.execute_batch("CREATE TABLE Preferences(Id INTEGER PRIMARY KEY, Json TEXT NOT NULL)")?;
    let mut old: Settings = serde_json::from_str(r#"{"notify":false,"language":"en"}"#)?;
    assert!(old.close_to_tray);
    let mut options = InstallChoices {
        stamp: "first".into(),
        capture: Shortcut::parse("Shift+Alt+Q")?,
        board: Shortcut::parse("Shift+Alt+W")?,
        close_to_tray: false,
        open_after: false,
    };
    preferences::apply_once(&db, &mut old, &options)?;
    assert!(!old.close_to_tray && !old.open_after && !old.notify);
    assert_eq!(old.language, "en");
    old.close_to_tray = true;
    db.execute(
        "UPDATE Preferences SET Json=?",
        [serde_json::to_string(&old)?],
    )?;
    preferences::apply_once(&db, &mut old, &options)?;
    assert!(preferences::existing(&db)?.unwrap().close_to_tray);
    options.stamp = "upgrade".into();
    preferences::apply_once(&db, &mut old, &options)?;
    assert!(!old.close_to_tray);
    assert_eq!(preferences::applied(&db)?.as_deref(), Some("upgrade"));
    options.stamp = "bad".into();
    options.board = options.capture;
    assert!(preferences::apply_once(&db, &mut old, &options).is_err());
    assert_eq!(preferences::applied(&db)?.as_deref(), Some("upgrade"));
    Ok(())
}
