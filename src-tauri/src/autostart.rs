//! Start at sign-in, through `tauri-plugin-autostart`.
//!
//! The plugin registers `D:\Program Files\...\x.exe` without quotes, which Windows
//! would have to guess at, trying `D:\Program.exe` first. Each write is therefore
//! followed by quoting the value.

use std::fs;
use std::path::Path;
use tauri::{AppHandle, Runtime};
use tauri_plugin_autostart::ManagerExt;
use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE};
use winreg::RegKey;

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

/// The quoted form of a registered command, or `None` if it needs no change.
/// This app registers no arguments, so the whole value is the executable path.
fn quoted(value: &str) -> Option<String> {
    let path = value.trim();
    (!path.is_empty() && !path.starts_with('"')).then(|| format!("\"{path}\""))
}

fn quote_run_value(value_name: &str) -> std::io::Result<()> {
    let key = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(RUN_KEY, KEY_READ | KEY_SET_VALUE)?;
    let current: String = match key.get_value(value_name) {
        Ok(current) => current,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };
    match quoted(&current) {
        Some(fixed) => key.set_value(value_name, &fixed),
        None => Ok(()),
    }
}

pub fn set_enabled<R: Runtime>(app: &AppHandle<R>, enabled: bool) -> Result<(), String> {
    let manager = app.autolaunch();
    if !enabled {
        return manager.disable().map_err(|e| e.to_string());
    }
    manager.enable().map_err(|e| e.to_string())?;
    quote_run_value(&app.package_info().name).map_err(|e| e.to_string())
}

/// Switched on once, on the first release run, so that turning it off from the
/// tray stays off. Debug builds never touch the registry.
pub fn init<R: Runtime>(app: &AppHandle<R>, data_dir: &Path) -> Result<(), String> {
    if cfg!(debug_assertions) {
        return Ok(());
    }
    let marker = data_dir.join("autostart-initialized");
    if marker.exists() {
        return quote_run_value(&app.package_info().name).map_err(|e| e.to_string());
    }
    set_enabled(app, true)?;
    fs::write(&marker, "").map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_a_bare_path_and_drops_the_plugins_trailing_space() {
        assert_eq!(
            quoted(r"D:\Program Files\B\b.exe ").as_deref(),
            Some(r#""D:\Program Files\B\b.exe""#)
        );
    }

    #[test]
    fn leaves_an_already_quoted_value_alone() {
        assert_eq!(quoted(r#""D:\Program Files\B\b.exe""#), None);
    }

    #[test]
    fn leaves_an_empty_value_alone() {
        assert_eq!(quoted("  "), None);
    }
}
