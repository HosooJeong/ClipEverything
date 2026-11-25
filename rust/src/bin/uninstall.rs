#![windows_subsystem = "windows"]

use anyhow::{Context, Result, bail, ensure};
use clip_everything::{
    clipboard::wide,
    preferences::{INSTALL_KEY, reg_string, same_dir},
};
use std::{
    os::windows::ffi::OsStringExt,
    path::{Path, PathBuf},
    process::Command,
    ptr::null_mut,
};
use windows_sys::Win32::{
    Foundation::{ERROR_NO_MORE_ITEMS, ERROR_SUCCESS},
    System::{
        ApplicationInstallationAndServicing::MsiEnumRelatedProductsW,
        SystemInformation::GetSystemDirectoryW,
    },
    UI::WindowsAndMessaging::{
        IDYES, MB_DEFBUTTON2, MB_ICONERROR, MB_ICONQUESTION, MB_YESNO, MessageBoxW,
    },
};

// The MSI product code changes on upgrades. Match the current registration to
// both this install directory and the ClipEverything upgrade family.
fn installed_product() -> Result<String> {
    let missing = "현재 사용자의 ClipEverything 설치 정보를 찾을 수 없습니다.";
    let dir = reg_string(INSTALL_KEY, "InstallLocation")?.context(missing)?;
    let exe = std::env::current_exe()?;
    ensure!(
        exe.parent().is_some_and(|p| same_dir(p, Path::new(&dir))),
        "설치 폴더의 Uninstall.exe를 실행해 주십시오."
    );
    let expected = reg_string(INSTALL_KEY, "ProductCode")?.context(missing)?;
    let family = wide("{F87E8F7A-8788-43C4-B20E-D483C3E324D0}");
    for index in 0.. {
        let mut id = [0u16; 39];
        let result = unsafe { MsiEnumRelatedProductsW(family.as_ptr(), 0, index, id.as_mut_ptr()) };
        if result == ERROR_NO_MORE_ITEMS {
            break;
        }
        ensure!(
            result == ERROR_SUCCESS,
            "설치 정보를 조회할 수 없습니다({result})."
        );
        let end = id.iter().position(|&ch| ch == 0).unwrap_or(id.len());
        let code = String::from_utf16(&id[..end])?;
        if expected.eq_ignore_ascii_case(&code) {
            return Ok(code);
        }
    }
    bail!(missing)
}

fn run() -> Result<()> {
    let product = installed_product()?;
    let confirmed = unsafe {
        MessageBoxW(
            null_mut(),
            wide("ClipEverything을 제거하시겠습니까?\n개인 보관함과 버튼 배치는 유지됩니다.")
                .as_ptr(),
            wide("ClipEverything 제거").as_ptr(),
            MB_YESNO | MB_ICONQUESTION | MB_DEFBUTTON2,
        )
    };
    if confirmed != IDYES {
        return Ok(());
    }
    let mut buffer = [0u16; 260];
    let n = unsafe { GetSystemDirectoryW(buffer.as_mut_ptr(), buffer.len() as u32) } as usize;
    ensure!(
        n > 0 && n < buffer.len(),
        "Windows 시스템 폴더를 찾을 수 없습니다."
    );
    let system = PathBuf::from(std::ffi::OsString::from_wide(&buffer[..n]));
    // Exit as soon as Windows Installer takes over so it can remove this EXE.
    Command::new(system.join("msiexec.exe"))
        .current_dir(&system)
        .args(["/x", &product, "/qb", "/norestart"])
        .spawn()
        .context("Windows Installer를 실행할 수 없습니다.")?;
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        unsafe {
            MessageBoxW(
                null_mut(),
                wide(&format!("{error:#}")).as_ptr(),
                wide("ClipEverything 제거").as_ptr(),
                MB_ICONERROR,
            );
        }
        std::process::exit(1);
    }
}
