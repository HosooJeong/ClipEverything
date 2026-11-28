use crate::store::{self, ClipEdit, Format, PreviewEdit, Source};
use anyhow::{Context, Result, bail, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    ffi::{CStr, CString, c_char},
    path::Path,
};

const MAX_CLIP_BYTES: usize = 32 * 1024 * 1024;
const TEXT_FORMAT: u32 = 13;
const PNG_FORMAT: u32 = 0xc000;

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", content = "contents", rename_all = "snake_case")]
enum Payload {
    Text(String),
    Png(String),
}

#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
enum Request {
    List,
    Save { payload: Payload },
    Read { id: i64 },
    Edit { id: i64, name: String, tags: String },
    Delete { id: i64 },
}

fn format(payload: Payload) -> Result<Format> {
    let (id, name, data) = match payload {
        Payload::Text(text) => {
            ensure!(
                !text.is_empty() && !text.contains('\0'),
                "Unsupported clipboard text"
            );
            ensure!(
                text.len() <= MAX_CLIP_BYTES / 2,
                "Clipboard text is too large"
            );
            let data = text
                .encode_utf16()
                .chain(std::iter::once(0))
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>();
            (TEXT_FORMAT, "Unicode Text", data)
        }
        Payload::Png(encoded) => {
            ensure!(
                encoded.len() <= MAX_CLIP_BYTES.div_ceil(3) * 4,
                "Image exceeds 32 MiB"
            );
            let data = STANDARD.decode(encoded)?;
            ensure!(data.starts_with(b"\x89PNG\r\n\x1a\n"), "Invalid PNG image");
            (PNG_FORMAT, "PNG", data)
        }
    };
    ensure!(data.len() <= MAX_CLIP_BYTES, "Clip exceeds 32 MiB");
    Ok(Format {
        id,
        name: name.into(),
        data,
    })
}

fn request(directory: &Path, command: Request) -> Result<Value> {
    let mut db = store::open(directory, None)?;
    Ok(match command {
        Request::List => serde_json::to_value(store::items(&db)?)?,
        Request::Save { payload } => {
            let format = format(payload)?;
            let source = Source {
                app: "macOS".into(),
                ..Source::default()
            };
            json!(store::save(&mut db, &[format], &source)?)
        }
        Request::Read { id } => {
            let formats = store::formats(&db, id)?;
            let data = formats.first().context("Clip no longer exists")?;
            let payload = match data.id {
                TEXT_FORMAT => {
                    let units: Vec<_> = data
                        .data
                        .chunks_exact(2)
                        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                        .take_while(|&unit| unit != 0)
                        .collect();
                    Payload::Text(String::from_utf16(&units)?)
                }
                PNG_FORMAT if data.name == "PNG" => Payload::Png(STANDARD.encode(&data.data)),
                _ => bail!("Unsupported clipboard format"),
            };
            serde_json::to_value(payload)?
        }
        Request::Edit { id, name, tags } => {
            store::edit(
                &mut db,
                id,
                ClipEdit {
                    name: &name,
                    tags: &tags,
                    preview: PreviewEdit::Keep,
                },
            )?;
            json!(true)
        }
        Request::Delete { id } => {
            let deleted = db.execute("DELETE FROM ClipboardItems WHERE Id = ?1", [id])?;
            ensure!(deleted == 1, "Clip no longer exists");
            json!(true)
        }
    })
}

// The Swift caller passes two live, NUL-terminated UTF-8 strings and frees the
// returned allocation with ce_free_string after copying it.
#[unsafe(no_mangle)]
unsafe extern "C" fn ce_request(directory: *const c_char, command: *const c_char) -> *mut c_char {
    let result = std::panic::catch_unwind(|| -> Result<Value> {
        ensure!(
            !directory.is_null() && !command.is_null(),
            "Missing request"
        );
        let directory = unsafe { CStr::from_ptr(directory) }.to_str()?;
        let command = unsafe { CStr::from_ptr(command) }.to_bytes();
        request(Path::new(directory), serde_json::from_slice(command)?)
    });
    let reply = match result {
        Ok(Ok(value)) => json!({ "value": value }),
        Ok(Err(error)) => json!({ "error": error.to_string() }),
        Err(_) => json!({ "error": "The clip store could not complete the request" }),
    };
    CString::new(reply.to_string())
        .expect("JSON escapes NUL characters")
        .into_raw()
}

#[unsafe(no_mangle)]
unsafe extern "C" fn ce_free_string(text: *mut c_char) {
    if !text.is_null() {
        drop(unsafe { CString::from_raw(text) });
    }
}
