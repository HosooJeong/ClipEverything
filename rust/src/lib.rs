pub mod board;
#[cfg(windows)]
pub mod clipboard;
#[cfg(any(target_os = "macos", test))]
mod macos;
#[cfg(windows)]
pub mod preferences;
pub mod store;
