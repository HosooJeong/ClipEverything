use anyhow::{Result, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use image::{DynamicImage, ImageDecoder, ImageFormat, codecs::bmp::BmpDecoder};
use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{fs, io::Cursor, path::Path};

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Format {
    pub id: u32,
    pub name: String,
    pub data: Vec<u8>,
}

#[derive(Default, Clone)]
pub struct Source {
    pub hwnd: isize,
    pub pid: u32,
    pub app: String,
    pub title: String,
    pub exe: String,
}

#[derive(Serialize)]
pub struct Item {
    pub id: i64,
    pub name: String,
    pub app: String,
    pub title: String,
    pub kind: i32,
    pub tags: String,
    pub favorite: bool,
    pub copied: String,
    pub preview: String,
    pub shot: i64,
    pub program_icon: Option<i64>,
    pub program_name: String,
    pub has_thumbnail: bool,
}
#[derive(Serialize)]
pub struct Detail {
    pub image: Option<String>,
    pub formats: Vec<String>,
}

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS ClipboardItems (
Id INTEGER PRIMARY KEY AUTOINCREMENT, Name TEXT, SourceApp TEXT NOT NULL DEFAULT '',
SourceWindow TEXT NOT NULL DEFAULT '', ExecutablePath TEXT NOT NULL DEFAULT '',
ContentHash TEXT NOT NULL DEFAULT '', ContentType INTEGER NOT NULL DEFAULT 0,
Tags TEXT NOT NULL DEFAULT '', IsFavorite INTEGER NOT NULL DEFAULT 0, CreatedAt TEXT NOT NULL,
LastCopiedAt TEXT NOT NULL, Thumbnail BLOB, PreviewText TEXT NOT NULL DEFAULT '');
CREATE TABLE IF NOT EXISTS ClipboardFormats (
Id INTEGER PRIMARY KEY AUTOINCREMENT, ItemId INTEGER NOT NULL REFERENCES ClipboardItems(Id) ON DELETE CASCADE,
FormatId INTEGER NOT NULL, FormatName TEXT NOT NULL, Data BLOB NOT NULL);
CREATE TABLE IF NOT EXISTS SourcePrograms(Id INTEGER PRIMARY KEY,App TEXT NOT NULL COLLATE NOCASE UNIQUE,Exe TEXT NOT NULL,Label TEXT NOT NULL,Icon BLOB);
CREATE INDEX IF NOT EXISTS idx_items_hash ON ClipboardItems(ContentHash);
CREATE INDEX IF NOT EXISTS idx_formats_item ON ClipboardFormats(ItemId);";

pub fn open(dir: &Path, legacy: Option<&Path>) -> Result<Connection> {
    fs::create_dir_all(dir)?;
    let path = dir.join("clips.db");
    if !path.exists()
        && let Some(source) = legacy.filter(|p| p.is_file())
    {
        // SQLite's backup API includes committed WAL pages in a consistent snapshot.
        let old = Connection::open_with_flags(source, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let temp = dir.join("import.pending");
        if temp.exists() {
            fs::remove_file(&temp)?;
        }
        old.backup(rusqlite::MAIN_DB, &temp, None)?;
        let imported = Connection::open(&temp)?;
        imported.prepare("SELECT ContentHash FROM ClipboardItems LIMIT 0")?;
        imported.prepare("SELECT Data FROM ClipboardFormats LIMIT 0")?;
        drop(imported);
        fs::rename(temp, &path)?;
    }
    let db = Connection::open(&path)?;
    db.busy_timeout(std::time::Duration::from_secs(3))?;
    db.execute_batch("PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL;")?;
    db.execute_batch(SCHEMA)?;
    if !db.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_table_info('ClipboardItems') WHERE name='PreviewText')",
        [],
        |r| r.get::<_, bool>(0),
    )? {
        db.execute_batch(
            "ALTER TABLE ClipboardItems ADD COLUMN PreviewText TEXT NOT NULL DEFAULT '';",
        )?;
    }
    crate::board::init(&db)?;
    Ok(db)
}

pub fn items(db: &Connection) -> Result<Vec<Item>> {
    let mut q=db.prepare("SELECT Id,coalesce(Name,''),SourceApp,SourceWindow,ContentType,coalesce(Tags,''),coalesce(IsFavorite,0),LastCopiedAt,coalesce(PreviewText,''),coalesce((SELECT Revision FROM ButtonPreviews WHERE ItemId=ClipboardItems.Id),0),(SELECT Id FROM SourcePrograms WHERE App=SourceApp AND Icon IS NOT NULL),coalesce(length(Thumbnail),0)>0,coalesce((SELECT Label FROM SourcePrograms WHERE App=SourceApp),SourceApp) FROM ClipboardItems ORDER BY LastCopiedAt DESC, Id DESC")?;
    Ok(q.query_map([], |r| {
        Ok(Item {
            id: r.get(0)?,
            name: r.get(1)?,
            app: r.get(2)?,
            title: r.get(3)?,
            kind: r.get(4)?,
            tags: r.get(5)?,
            favorite: r.get(6)?,
            copied: r.get(7)?,
            preview: r.get(8)?,
            shot: r.get(9)?,
            program_icon: r.get(10)?,
            program_name: r.get(12)?,
            has_thumbnail: r.get(11)?,
        })
    })?
    .collect::<Result<_, _>>()?)
}
pub fn formats(db: &Connection, id: i64) -> Result<Vec<Format>> {
    let mut q = db.prepare(
        "SELECT FormatId,FormatName,Data FROM ClipboardFormats WHERE ItemId=? ORDER BY Id",
    )?;
    Ok(q.query_map([id], |r| {
        Ok(Format {
            id: r.get(0)?,
            name: r.get(1)?,
            data: r.get(2)?,
        })
    })?
    .collect::<Result<_, _>>()?)
}
pub fn detail(db: &Connection, id: i64) -> Result<Detail> {
    let img: Option<Vec<u8>> = db.query_row(
        "SELECT coalesce((SELECT Png FROM ButtonPreviews WHERE ItemId=Id),Thumbnail) FROM ClipboardItems WHERE Id=?",
        [id],
        |r| r.get(0),
    )?;
    let mut q = db.prepare("SELECT FormatName FROM ClipboardFormats WHERE ItemId=? ORDER BY Id")?;
    Ok(Detail {
        image: img
            .filter(|x| !x.is_empty())
            .map(|x| format!("data:image/png;base64,{}", STANDARD.encode(x))),
        formats: q.query_map([id], |r| r.get(0))?.collect::<Result<_, _>>()?,
    })
}

fn text_preview(formats: &[Format]) -> String {
    if let Some(drop) = formats.iter().find(|f| f.id == 15) {
        // A wide DROPFILES list is double-NUL terminated, with a byte offset
        // to the first path. Keep the preview bounded even for large selections.
        let data = &drop.data;
        if data.len() >= 20 && data[16..20] != [0; 4] {
            let start = u32::from_le_bytes(data[..4].try_into().unwrap()) as usize;
            if start >= 20
                && let Some(paths) = data.get(start..)
            {
                let units = paths
                    .chunks_exact(2)
                    .take(513)
                    .map(|b| u16::from_le_bytes([b[0], b[1]]))
                    .collect::<Vec<_>>();
                let end = units
                    .windows(2)
                    .position(|pair| pair == [0, 0])
                    .unwrap_or(units.len());
                return String::from_utf16_lossy(&units[..end]).replace('\0', "\n");
            }
        }
    }
    let Some(f) = formats.iter().find(|f| f.id == 13) else {
        return String::new();
    };
    let units = f
        .data
        .chunks_exact(2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .take_while(|n| *n != 0);
    char::decode_utf16(units)
        .map(|c| c.unwrap_or(char::REPLACEMENT_CHARACTER))
        .take(512)
        .collect()
}
fn kind(formats: &[Format]) -> i32 {
    let names = formats
        .iter()
        .map(|f| f.name.to_lowercase())
        .collect::<Vec<_>>()
        .join("\n");
    if names.contains("biff") || names.contains("xml spreadsheet") {
        3
    } else if names.contains("hwp") {
        4
    } else if formats.iter().any(|f| f.id == 15) {
        7
    } else if names.contains("html format") {
        2
    } else if names.contains("rich text") || names.contains("rtf") {
        1
    } else if formats
        .iter()
        .any(|f| matches!(f.id, 6 | 8 | 17) || f.name.eq_ignore_ascii_case("PNG"))
    {
        5
    } else if formats.iter().any(|f| matches!(f.id, 1 | 13)) {
        0
    } else {
        6
    }
}
fn thumbnail(formats: &[Format]) -> Option<Vec<u8>> {
    let dib = formats.iter().find(|f| matches!(f.id, 8 | 17))?;
    let decoder = BmpDecoder::new_without_file_header(Cursor::new(&dib.data)).ok()?;
    if decoder.total_bytes() > 128 * 1024 * 1024 {
        return None;
    }
    let mut out = Cursor::new(Vec::new());
    DynamicImage::from_decoder(decoder)
        .ok()?
        .thumbnail(480, 320)
        .write_to(&mut out, ImageFormat::Png)
        .ok()?;
    Some(out.into_inner())
}
fn digest(formats: &[Format]) -> String {
    // v1 already used this key for plain text. Ignore Win32's synthesized ANSI
    // and locale variants, plus unused allocation bytes after the terminating NUL.
    if formats.iter().all(|f| matches!(f.id, 1 | 7 | 13 | 16))
        && let Some(text) = formats.iter().find(|f| f.id == 13)
    {
        let length = text
            .data
            .chunks_exact(2)
            .position(|unit| unit == [0, 0])
            .map_or(text.data.len() & !1, |index| index * 2);
        let mut h = Sha256::new();
        h.update(b"unicode-text\n");
        h.update(&text.data[..length]);
        return format!("{:x}", h.finalize());
    }
    let mut parts: Vec<_> = formats
        .iter()
        .map(|f| {
            let key = if f.id >= 0xc000 {
                format!("name:{}", f.name)
            } else {
                format!("id:{}", f.id)
            };
            (key, Sha256::digest(&f.data))
        })
        .collect();
    parts.sort_unstable();
    let mut h = Sha256::new();
    for (key, bytes) in parts {
        h.update((key.len() as u64).to_le_bytes());
        h.update(key);
        h.update(bytes);
    }
    format!("v2:{:x}", h.finalize())
}

pub fn save(db: &mut Connection, formats: &[Format], source: &Source) -> Result<i64> {
    save_near(db, formats, source, None)
}
pub fn save_near(
    db: &mut Connection,
    formats: &[Format],
    source: &Source,
    pointer: Option<(i32, i32)>,
) -> Result<i64> {
    ensure!(!formats.is_empty(), "Empty clipboard");
    let hash = digest(formats);
    let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let mut id = tx
        .query_row(
            "SELECT Id FROM ClipboardItems WHERE ContentHash=? LIMIT 1",
            [&hash],
            |r| r.get(0),
        )
        .optional()?;
    // Older versions also assigned the plain-text key to rich clips. A plain
    // copy must never overwrite the source metadata for a different payload.
    let mut hash = hash;
    if !hash.starts_with("v2:")
        && let Some(candidate) = id
    {
        let stored = self::formats(&tx, candidate)?;
        if digest(&stored) != hash {
            hash = format!("v2:plain:{hash}");
            id = tx
                .query_row(
                    "SELECT Id FROM ClipboardItems WHERE ContentHash=? LIMIT 1",
                    [&hash],
                    |r| r.get(0),
                )
                .optional()?;
        }
    }
    let now: String = tx.query_row("SELECT strftime('%Y-%m-%dT%H:%M:%fZ','now')", [], |r| {
        r.get(0)
    })?;
    let id = if let Some(id) = id {
        tx.execute("UPDATE ClipboardItems SET LastCopiedAt=?,SourceApp=?,SourceWindow=?,ExecutablePath=? WHERE Id=?",params![now,source.app,source.title,source.exe,id])?;
        id
    } else {
        tx.execute("INSERT INTO ClipboardItems(SourceApp,SourceWindow,ExecutablePath,ContentHash,ContentType,CreatedAt,LastCopiedAt,Thumbnail,PreviewText,Tags,IsFavorite) VALUES(?,?,?,?,?,?,?,?,?,'',0)",params![source.app,source.title,source.exe,hash,kind(formats),now,now,thumbnail(formats),text_preview(formats)])?;
        let id = tx.last_insert_rowid();
        for f in formats {
            tx.execute(
                "INSERT INTO ClipboardFormats(ItemId,FormatId,FormatName,Data) VALUES(?,?,?,?)",
                params![id, f.id, f.name, f.data],
            )?;
        }
        tx.execute("DELETE FROM ClipboardItems WHERE Id IN (SELECT Id FROM ClipboardItems WHERE IsFavorite=0 ORDER BY LastCopiedAt DESC,Id DESC LIMIT -1 OFFSET 1000)",[])?;
        id
    };
    crate::board::place(&tx, &source.app, id, pointer)?;
    tx.commit()?;
    Ok(id)
}
