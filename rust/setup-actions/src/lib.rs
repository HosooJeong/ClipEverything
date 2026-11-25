use anyhow::{Context, Result, bail, ensure};
use clip_everything::{
    clipboard::wide,
    preferences::{self as prefs, InstallChoices, Settings, Shortcut},
};
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use std::{
    env,
    path::{Path, PathBuf},
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Foundation::*,
    System::{ApplicationInstallationAndServicing::*, Com::*, Registry::*},
};

fn property(session: MSIHANDLE, key: &str) -> Result<String> {
    let key = wide(key);
    let mut data = vec![0u16; 1024];
    loop {
        let mut n = data.len() as u32;
        let code = unsafe { MsiGetPropertyW(session, key.as_ptr(), data.as_mut_ptr(), &mut n) };
        if code == ERROR_MORE_DATA {
            data.resize(n as usize + 1, 0);
            continue;
        }
        ensure!(code == 0, "설치 속성을 읽을 수 없습니다({code}).");
        return Ok(String::from_utf16_lossy(&data[..n as usize]));
    }
}
fn set(session: MSIHANDLE, key: &str, value: &str) -> Result<()> {
    let code = unsafe { MsiSetPropertyW(session, wide(key).as_ptr(), wide(value).as_ptr()) };
    ensure!(
        code == 0,
        "설치 속성 {key}을(를) 설정할 수 없습니다({code})."
    );
    Ok(())
}
fn action(session: MSIHANDLE, f: fn(MSIHANDLE) -> Result<()>) -> u32 {
    match std::panic::catch_unwind(|| f(session)) {
        Ok(Ok(())) => 0,
        result => {
            let error = match result {
                Ok(Err(e)) => format!("{e:#}"),
                _ => "설치 설정을 처리하는 중 오류가 발생했습니다.".into(),
            };
            unsafe {
                let r = MsiCreateRecord(1);
                MsiRecordSetStringW(r, 0, wide("[1]").as_ptr());
                MsiRecordSetStringW(r, 1, wide(&error).as_ptr());
                MsiProcessMessage(session, INSTALLMESSAGE_ERROR, r);
                MsiCloseHandle(r);
            }
            ERROR_INSTALL_FAILURE
        }
    }
}
fn initialize(session: MSIHANDLE) -> Result<()> {
    let appdata =
        PathBuf::from(env::var_os("APPDATA").context("앱 데이터 경로를 찾을 수 없습니다.")?);
    let dbfile = appdata.join("ClipEverythingRust/clips.db");
    let mut s = Settings::default();
    let mut applied = None;
    if dbfile.is_file() {
        let db = Connection::open_with_flags(dbfile, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        if let Some(stored) = prefs::existing(&db)? {
            s = stored
        }
        applied = prefs::applied(&db)?;
    } else if let Ok(bytes) = std::fs::read(appdata.join("ClipEverything/settings.json"))
        && let Ok(j) = serde_json::from_slice::<serde_json::Value>(&bytes)
    {
        s.open_after = j["openOverlayAfterCopy"].as_bool().unwrap_or(s.open_after);
    }
    if let Some(old) = prefs::recipe()?
        && applied.as_ref() != Some(&old.stamp)
    {
        old.apply(&mut s)?;
    }
    let previous = previous_dir()?;
    let requested = property(session, "INSTALLFOLDER")?;
    let path = if requested.is_empty() {
        previous.clone()
    } else {
        PathBuf::from(requested)
    };
    let run = prefs::reg_string(prefs::RUN_KEY, prefs::RUN_NAME)?;
    let enabled = run.as_deref().is_some_and(|v| {
        matches_exe(v, &previous.join("ClipEverything.exe"))
            || matches_exe(v, &path.join("ClipEverything.exe"))
    });
    let [a, b] = s.keys();
    for (key, default) in [("CE_COPY", a.label()), ("CE_PASTE", b.label())] {
        if property(session, key)?.is_empty() {
            set(session, key, &default)?;
        }
    }
    for (key, default) in [
        (
            "CE_MENU_GROUP",
            prefs::reg_string(prefs::INSTALL_KEY, "FlatShortcut")?.is_none(),
        ),
        ("CE_STARTUP", enabled),
        ("CE_DESKTOP", desktop_shortcut_selected()?),
        ("CE_TRAY", s.close_to_tray),
        ("CE_EDIT", s.open_after),
    ] {
        let value = match property(session, key)?.as_str() {
            "" => default,
            "1" => true,
            "0" => false,
            _ => bail!("{key} 값은 0 또는 1이어야 합니다."),
        };
        set(session, key, if value { "1" } else { "" })?;
    }
    set(session, "INSTALLFOLDER", &path.to_string_lossy())?;

    set(session, "CE_INITIALIZED", "1")
}
fn desktop_shortcut_selected() -> Result<bool> {
    Ok(prefs::reg_string(prefs::INSTALL_KEY, "DesktopShortcut")?.is_some())
}
fn matches_exe(command: &str, exe: &Path) -> bool {
    let value = command.trim_start();
    let path = if let Some(quoted) = value.strip_prefix('"') {
        quoted.split('"').next().unwrap_or("")
    } else {
        value.split_whitespace().next().unwrap_or("")
    };
    path.eq_ignore_ascii_case(&exe.to_string_lossy())
}
fn previous_dir() -> Result<PathBuf> {
    Ok(
        match prefs::reg_string(prefs::INSTALL_KEY, "InstallLocation")? {
            Some(path) => path.into(),
            None => PathBuf::from(
                env::var_os("LOCALAPPDATA").context("로컬 앱 데이터 경로를 찾을 수 없습니다.")?,
            )
            .join("Programs")
            .join("ClipEverything"),
        },
    )
}
fn choices(session: MSIHANDLE) -> Result<InstallChoices> {
    let capture = Shortcut::parse(&property(session, "CE_COPY")?).context("등록 단축키")?;
    let board = Shortcut::parse(&property(session, "CE_PASTE")?).context("버튼판 단축키")?;
    Ok(InstallChoices {
        capture,
        board,
        stamp: String::new(),
        close_to_tray: property(session, "CE_TRAY")? == "1",
        open_after: property(session, "CE_EDIT")? == "1",
    })
}
fn verify_keys(session: MSIHANDLE) -> Result<InstallChoices> {
    let recipe = choices(session)?;
    let mut s = Settings::default();
    recipe.apply(&mut s)?;
    prefs::probe(s)?;
    Ok(recipe)
}
fn valid_path(text: &str) -> Result<PathBuf> {
    let p = PathBuf::from(text.trim());
    ensure!(
        p.is_absolute()
            && p.parent().is_some()
            && !p
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir)),
        "설치 폴더의 전체 경로가 필요합니다."
    );
    ensure!(
        !p.to_string_lossy().contains(['"', '<', '>', '|', '*', '?']) && !p.is_file(),
        "설치할 수 없는 경로입니다."
    );
    ensure!(
        !p.join("portable.flag").exists(),
        "포터블 데이터 폴더에는 설치할 수 없습니다."
    );
    Ok(p)
}
fn validate(session: MSIHANDLE) -> Result<InstallChoices> {
    let options = verify_keys(session)?;
    let path = valid_path(&property(session, "INSTALLFOLDER")?)?;
    if path.join("ClipEverything.exe").exists() || path.join("Uninstall.exe").exists() {
        ensure!(
            !property(session, "WIX_UPGRADE_DETECTED")?.is_empty()
                && prefs::same_dir(&path, &previous_dir()?),
            "이 폴더에 같은 이름의 실행 파일이 있습니다. 다른 설치 폴더를 선택해 주십시오."
        );
    }
    let target = wide(&path.to_string_lossy());
    let code =
        unsafe { MsiSetTargetPathW(session, wide("INSTALLFOLDER").as_ptr(), target.as_ptr()) };
    ensure!(code == 0, "선택한 설치 폴더를 사용할 수 없습니다({code}).");
    Ok(options)
}
fn feedback(session: MSIHANDLE, result: Result<()>) -> Result<()> {
    let ok = result.is_ok();
    let text = match result {
        Ok(()) => "두 단축키 모두 사용할 수 있습니다.".into(),
        Err(e) => format!("{e:#}"),
    };
    set(session, "CE_STATUS", &text)?;
    set(session, "CE_VALID", if ok { "1" } else { "" })
}
#[unsafe(no_mangle)]
pub extern "system" fn InitializeSetup(s: MSIHANDLE) -> u32 {
    action(s, initialize)
}
#[unsafe(no_mangle)]
pub extern "system" fn LoadMaintenance(s: MSIHANDLE) -> u32 {
    action(s, |s| {
        set(
            s,
            "CE_DESKTOP",
            if desktop_shortcut_selected()? {
                "1"
            } else {
                ""
            },
        )?;
        set(
            s,
            "CE_MENU_GROUP",
            if prefs::reg_string(prefs::INSTALL_KEY, "FlatShortcut")?.is_none() {
                "1"
            } else {
                ""
            },
        )?;
        if let Some(path) = prefs::reg_string(prefs::INSTALL_KEY, "InstallLocation")? {
            set(s, "INSTALLFOLDER", &path)?;
        }
        if let Some(recipe) = prefs::reg_string(prefs::INSTALL_KEY, "InitialSettings")? {
            set(s, "CE_RECIPE", &recipe)?;
        }
        Ok(())
    })
}
#[unsafe(no_mangle)]
pub extern "system" fn CheckKeys(s: MSIHANDLE) -> u32 {
    action(s, |s| feedback(s, verify_keys(s).map(|_| ())))
}
#[unsafe(no_mangle)]
pub extern "system" fn CheckOptions(s: MSIHANDLE) -> u32 {
    action(s, |s| feedback(s, validate(s).map(|_| ())))
}

#[derive(Serialize, Deserialize)]
struct RunChange {
    old: Option<String>,
    new: Option<String>,
}
fn prepare(s: MSIHANDLE) -> Result<()> {
    let mut choices = validate(s)?;
    // Reinstalling the same MSI is a new choice, while MSI repair must keep the old stamp.
    let mut id = windows_sys::core::GUID::default();
    ensure!(
        unsafe { CoCreateGuid(&mut id) } >= 0,
        "설치 ID를 생성할 수 없습니다."
    );
    choices.stamp = format!(
        "{:08x}{:04x}{:04x}{:016x}",
        id.data1,
        id.data2,
        id.data3,
        u64::from_be_bytes(id.data4)
    );
    set(s, "CE_RECIPE", &serde_json::to_string(&choices)?)?;
    let exe = valid_path(&property(s, "INSTALLFOLDER")?)?.join("ClipEverything.exe");
    let old = prefs::reg_string(prefs::RUN_KEY, prefs::RUN_NAME)?;
    let previous = previous_dir()?;
    let new = if property(s, "CE_STARTUP")? == "1" {
        Some(format!("\"{}\" --background", exe.display()))
    } else if old.as_deref().is_some_and(|v| {
        matches_exe(v, &exe) || matches_exe(v, &previous.join("ClipEverything.exe"))
    }) {
        None
    } else {
        old.clone()
    };
    let json = serde_json::to_string(&RunChange { old, new })?;
    set(s, "ApplyStartup", &json)?;
    set(s, "RollbackStartup", &json)
}
#[unsafe(no_mangle)]
pub extern "system" fn PrepareSetup(s: MSIHANDLE) -> u32 {
    action(s, prepare)
}
fn change(s: MSIHANDLE, undo: bool) -> Result<()> {
    let data: RunChange = serde_json::from_str(&property(s, "CustomActionData")?)?;
    let (from, to) = if undo {
        (data.new, data.old)
    } else {
        (data.old, data.new)
    };
    if from == to {
        return Ok(());
    }
    let current = prefs::reg_string(prefs::RUN_KEY, prefs::RUN_NAME)?;
    if current != from {
        if undo || current == to {
            return Ok(());
        }
        bail!("자동 시작 설정이 변경되었습니다. 설치를 다시 실행해 주십시오.")
    }
    unsafe {
        let mut key = null_mut();
        let code = RegCreateKeyExW(
            HKEY_CURRENT_USER,
            wide(prefs::RUN_KEY).as_ptr(),
            0,
            null(),
            0,
            KEY_SET_VALUE,
            null(),
            &mut key,
            null_mut(),
        );
        ensure!(code == 0, "자동 시작 설정을 열 수 없습니다({code}).");
        let name = wide(prefs::RUN_NAME);
        let result = match to {
            Some(cmd) => {
                let value = wide(&cmd);
                RegSetValueExW(
                    key,
                    name.as_ptr(),
                    0,
                    REG_SZ,
                    value.as_ptr().cast(),
                    (value.len() * 2) as u32,
                )
            }
            None => RegDeleteValueW(key, name.as_ptr()),
        };
        RegCloseKey(key);
        ensure!(
            result == 0,
            "자동 시작 설정을 변경할 수 없습니다({result})."
        );
    }
    Ok(())
}
#[unsafe(no_mangle)]
pub extern "system" fn ApplyStartup(s: MSIHANDLE) -> u32 {
    action(s, |s| change(s, false))
}
#[unsafe(no_mangle)]
pub extern "system" fn RollbackStartup(s: MSIHANDLE) -> u32 {
    action(s, |s| change(s, true))
}
