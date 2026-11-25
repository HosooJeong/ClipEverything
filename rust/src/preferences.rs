use serde::{Deserialize, Serialize};

use crate::clipboard::wide;
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OptionalExtension, params};
use std::{path::Path, ptr::null_mut};
use windows_sys::Win32::{
    Foundation::{ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND, HWND},
    System::Registry::*,
    UI::Input::KeyboardAndMouse::*,
};

pub const INSTALL_KEY: &str = r"Software\ClipEverythingRust";
pub const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
pub const RUN_NAME: &str = "ClipEverythingRust";

pub fn reg_string(key: &str, name: &str) -> Result<Option<String>> {
    let mut buf = vec![0u16; 32768];
    let mut len = (buf.len() * 2) as u32;
    let result = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            wide(key).as_ptr(),
            wide(name).as_ptr(),
            RRF_RT_REG_SZ,
            null_mut(),
            buf.as_mut_ptr().cast(),
            &mut len,
        )
    };
    if result == ERROR_FILE_NOT_FOUND || result == ERROR_PATH_NOT_FOUND {
        return Ok(None);
    }
    ensure!(result == 0, "Cannot read {name} ({result})");
    Ok(Some(
        String::from_utf16_lossy(&buf[..len as usize / 2])
            .trim_end_matches('\0')
            .into(),
    ))
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct Shortcut {
    pub mods: u32,
    pub vk: u32,
}
impl Shortcut {
    pub fn parse(input: &str) -> Result<Self> {
        let mut parts = input.split('+').map(str::trim).collect::<Vec<_>>();
        let key = parts.pop().unwrap_or("").as_bytes();
        ensure!(
            key.len() == 1 && key[0].is_ascii_alphanumeric(),
            "마지막 키는 영문자 또는 숫자여야 합니다."
        );
        let mut mods = 0;
        for part in parts {
            let bit = match part.to_ascii_lowercase().as_str() {
                "ctrl" => 2,
                "alt" => 1,
                "shift" => 4,
                "win" => 8,
                _ => anyhow::bail!("Ctrl, Alt, Shift, Win 중 하나 이상이 필요합니다."),
            };
            ensure!(mods & bit == 0, "보조 키가 중복되었습니다.");
            mods |= bit;
        }
        let s = Self {
            mods,
            vk: u32::from(key[0].to_ascii_uppercase()),
        };
        s.validate()?;
        Ok(s)
    }
    fn validate(self) -> Result<()> {
        ensure!(
            (1..=15).contains(&self.mods)
                && ((48..=57).contains(&self.vk) || (65..=90).contains(&self.vk)),
            "보조 키와 영문자 또는 숫자를 조합해야 합니다."
        );
        Ok(())
    }
    pub fn label(self) -> String {
        let mut parts = Vec::new();
        for (bit, label) in [(2, "Ctrl"), (1, "Alt"), (4, "Shift"), (8, "Win")] {
            if self.mods & bit != 0 {
                parts.push(label.to_owned());
            }
        }
        parts.push(char::from_u32(self.vk).unwrap_or('?').to_string());
        parts.join("+")
    }
}
impl Settings {
    pub fn keys(&self) -> [Shortcut; 2] {
        [
            Shortcut {
                mods: self.copy_mods,
                vk: self.copy_vk,
            },
            Shortcut {
                mods: self.paste_mods,
                vk: self.paste_vk,
            },
        ]
    }
}
pub fn register_pair(hwnd: isize, s: &Settings) -> Result<()> {
    let [a, b] = s.keys();
    a.validate()?;
    b.validate()?;
    ensure!(a != b, "등록과 버튼판에는 서로 다른 단축키가 필요합니다.");
    let hwnd = hwnd as HWND;
    unsafe {
        ensure!(
            RegisterHotKey(hwnd, 1, a.mods | MOD_NOREPEAT, a.vk) != 0,
            "등록 단축키 {} 조합을 사용할 수 없습니다. 다른 앱에서 사용 중이거나 Windows 예약 조합입니다.",
            a.label()
        );
        if RegisterHotKey(hwnd, 2, b.mods | MOD_NOREPEAT, b.vk) == 0 {
            UnregisterHotKey(hwnd, 1);
            anyhow::bail!(
                "버튼판 단축키 {} 조합을 사용할 수 없습니다. 다른 앱에서 사용 중이거나 Windows 예약 조합입니다.",
                b.label()
            );
        }
    }
    Ok(())
}
pub fn probe(s: Settings) -> Result<()> {
    std::thread::spawn(move || {
        register_pair(0, &s)?;
        unsafe {
            UnregisterHotKey(null_mut(), 2);
            UnregisterHotKey(null_mut(), 1);
        }
        Ok(())
    })
    .join()
    .map_err(|_| anyhow::anyhow!("단축키 검사에 실패했습니다."))?
}

#[derive(Serialize, Deserialize)]
pub struct InstallChoices {
    pub stamp: String,
    pub capture: Shortcut,
    pub board: Shortcut,
    pub close_to_tray: bool,
    pub open_after: bool,
}
impl InstallChoices {
    pub fn apply(&self, s: &mut Settings) -> Result<()> {
        self.capture.validate()?;
        self.board.validate()?;
        ensure!(
            self.capture != self.board,
            "등록과 버튼판에는 서로 다른 단축키가 필요합니다."
        );
        (s.copy_mods, s.copy_vk) = (self.capture.mods, self.capture.vk);
        (s.paste_mods, s.paste_vk) = (self.board.mods, self.board.vk);
        s.close_to_tray = self.close_to_tray;
        s.open_after = self.open_after;
        Ok(())
    }
}
pub fn recipe() -> Result<Option<InstallChoices>> {
    reg_string(INSTALL_KEY, "InitialSettings")?
        .filter(|s| !s.is_empty())
        .map(|s| serde_json::from_str(&s).context("설치 설정을 읽을 수 없습니다."))
        .transpose()
}
pub fn existing(db: &Connection) -> Result<Option<Settings>> {
    if !db.table_exists(None, "Preferences")? {
        return Ok(None);
    }
    let raw = db
        .query_row("SELECT Json FROM Preferences WHERE Id=1", [], |r| {
            r.get::<_, String>(0)
        })
        .optional()?;
    Ok(raw.map(|s| serde_json::from_str(&s)).transpose()?)
}
pub fn applied(db: &Connection) -> Result<Option<String>> {
    if !db.table_exists(None, "InstallDefaultsApplied")? {
        return Ok(None);
    }
    Ok(db
        .query_row(
            "SELECT Stamp FROM InstallDefaultsApplied WHERE Id=1",
            [],
            |r| r.get(0),
        )
        .optional()?)
}
pub fn apply_once(db: &Connection, s: &mut Settings, options: &InstallChoices) -> Result<()> {
    let tx = db.unchecked_transaction()?;
    if applied(&tx)?.as_ref() != Some(&options.stamp) {
        options.apply(s)?;
        tx.execute(
            "INSERT OR REPLACE INTO Preferences(Id,Json) VALUES(1,?)",
            params![serde_json::to_string(s)?],
        )?;
        tx.execute_batch("CREATE TABLE IF NOT EXISTS InstallDefaultsApplied(Id INTEGER PRIMARY KEY, Stamp TEXT NOT NULL)")?;
        tx.execute(
            "INSERT OR REPLACE INTO InstallDefaultsApplied VALUES(1,?)",
            [&options.stamp],
        )?;
    }
    tx.commit()?;
    Ok(())
}
pub fn same_dir(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a
            .to_string_lossy()
            .eq_ignore_ascii_case(&b.to_string_lossy()),
        _ => false,
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TileContent {
    #[default]
    Icon,
    Preview,
    None,
}

#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    pub open_after: bool,
    pub notify: bool,
    pub language: String,
    pub startup: bool,
    pub close_to_tray: bool,
    pub copy_mods: u32,
    pub copy_vk: u32,
    pub paste_mods: u32,
    pub paste_vk: u32,
    pub tile_content: TileContent,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            open_after: true,
            notify: true,
            language: "system".into(),
            startup: false,
            close_to_tray: true,
            copy_mods: 3,
            copy_vk: 67,
            paste_mods: 3,
            paste_vk: 86,
            tile_content: TileContent::default(),
        }
    }
}
