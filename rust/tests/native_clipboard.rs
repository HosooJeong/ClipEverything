use anyhow::{Result, ensure};
use clip_everything::{
    clipboard::{restore, restore_if, snapshot, wide},
    store::Format,
};
use std::ptr::{null, null_mut};
use windows_sys::Win32::{
    System::{DataExchange::*, StationsAndDesktops::*},
    UI::WindowsAndMessaging::*,
};

// A private window station owns its own clipboard. Never run a round-trip test on the user's one.
#[test]
fn isolated_windows_clipboard_round_trip() -> Result<()> {
    unsafe {
        let station = CreateWindowStationW(null(), 0, 0xF037F, null());
        ensure!(
            !station.is_null() && SetProcessWindowStation(station) != 0,
            "Cannot create isolated window station"
        );
        let desktop = CreateDesktopW(wide("test").as_ptr(), null(), null(), 0, 0x000F01FF, null());
        ensure!(
            !desktop.is_null() && SetThreadDesktop(desktop) != 0,
            "Cannot enter isolated desktop"
        );
        let window = CreateWindowExW(
            0,
            wide("STATIC").as_ptr(),
            null(),
            0,
            0,
            0,
            1,
            1,
            HWND_MESSAGE,
            null_mut(),
            null_mut(),
            null(),
        );
        ensure!(!window.is_null(), "Cannot create clipboard owner");
        let text = Format {
            id: 13,
            name: "UnicodeText".into(),
            data: wide("한글 round trip")
                .into_iter()
                .flat_map(u16::to_le_bytes)
                .collect(),
        };
        let html = Format {
            id: 0xca00,
            name: "HTML Format".into(),
            data: b"<b>round trip</b>\0".to_vec(),
        };
        let palette = Format {
            id: 9,
            name: "Palette".into(),
            data: vec![0, 3, 1, 0, 18, 52, 86, 0],
        };
        restore(
            window as isize,
            &[text.clone(), html.clone(), palette.clone()],
        )?;
        let saved = snapshot(window as isize)?;
        for expected in [text, html, palette] {
            let data = &saved
                .iter()
                .find(|f| f.name == expected.name)
                .expect("missing format")
                .data;
            assert!(data.starts_with(&expected.data));
        }
        assert_ne!(GetClipboardOwner(), null_mut());
        let native = Format {
            id: 0xca10,
            name: "Hwp Native".into(),
            data: vec![1, 2],
        };
        let info = Format {
            id: 0xca11,
            name: "Hwp_Native_Info".into(),
            data: vec![0; 8],
        };
        restore(window as isize, &[info, native])?;
        let saved = snapshot(window as isize)?;
        assert!(saved.iter().any(|f| f.name == "Hwp Native"));
        assert!(saved.iter().all(|f| f.name != "Hwp_Native_Info"));
        let mut dropfiles = vec![0u8; 20];
        dropfiles[..4].copy_from_slice(&20u32.to_le_bytes());
        dropfiles[16] = 1;
        for path in [r"C:\문서\첫 번째.pdf", r"C:\문서\두 번째.pdf", ""] {
            dropfiles.extend(wide(path).into_iter().flat_map(u16::to_le_bytes));
        }
        let file = Format {
            id: 15,
            name: "FileDrop".into(),
            data: dropfiles,
        };
        let shell_id = Format {
            id: 0xca03,
            name: "Shell IDList Array".into(),
            data: vec![0; 8],
        };
        let cut = Format {
            id: 0xca04,
            name: "Preferred DropEffect".into(),
            data: 2u32.to_le_bytes().to_vec(),
        };
        restore(window as isize, &[file.clone(), shell_id.clone(), cut])?;
        let saved = snapshot(window as isize)?;
        assert!(
            saved
                .iter()
                .find(|f| f.id == 15)
                .unwrap()
                .data
                .starts_with(&file.data)
        );
        assert!(!saved.iter().any(|f| f.name == "Shell IDList Array"));
        assert_eq!(
            &saved
                .iter()
                .find(|f| f.name == "Preferred DropEffect")
                .unwrap()
                .data[..4],
            &1u32.to_le_bytes()
        );
        let seq = GetClipboardSequenceNumber();
        let mut broken = file.clone();
        broken.data.truncate(20);
        assert!(restore(window as isize, &[broken, shell_id.clone()]).is_err());
        assert!(restore_if(window as isize, &[file, shell_id], || false)?.is_none());
        assert_eq!(GetClipboardSequenceNumber(), seq);
        let marker = Format {
            id: 0xca01,
            name: "CanIncludeInClipboardHistory".into(),
            data: vec![0; 4],
        };
        restore(window as isize, &[marker])?;
        assert!(snapshot(window as isize).is_err());
        DestroyWindow(window);
        Ok(())
    }
}
