//! Brings a target app forward, launching it only when it is not already running.
//!
//! Neither app guards against a second instance: English's second process cannot
//! start its backend, and Forscher's database layer assumes a single process.

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use windows::core::{BOOL, PWSTR};
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindow, GetWindowThreadProcessId, IsIconic, IsWindowVisible,
    SetForegroundWindow, ShowWindow, GW_OWNER, SW_RESTORE,
};

/// How long a launched app gets to show its window before another click may
/// launch it again. English's backend makes its first window slow to appear.
const LAUNCH_GRACE: Duration = Duration::from_secs(20);

#[derive(Debug, PartialEq, Eq)]
pub enum OpenError {
    NotFound,
    Failed(String),
}

/// Whether a launch may proceed given the previous launch of the same app.
pub fn launch_allowed(last: Option<Instant>, now: Instant) -> bool {
    last.is_none_or(|t| now.duration_since(t) >= LAUNCH_GRACE)
}

/// Focuses the app's window, or launches it. Returns whether a launch happened.
pub fn open(exe: &Path, last_launch: Option<Instant>) -> Result<bool, OpenError> {
    if let Some(hwnd) = find_window(exe) {
        // SAFETY: the handle was just enumerated. If the window has since closed
        // these calls fail harmlessly.
        unsafe {
            if IsIconic(hwnd).as_bool() {
                // Only when minimized: restoring a normal window would drop
                // Forscher out of fullscreen.
                let _ = ShowWindow(hwnd, SW_RESTORE);
            }
            let _ = SetForegroundWindow(hwnd);
        }
        return Ok(false);
    }
    if !launch_allowed(last_launch, Instant::now()) {
        return Ok(false);
    }
    if !exe.is_file() {
        return Err(OpenError::NotFound);
    }
    let mut command = Command::new(exe);
    if let Some(dir) = exe.parent() {
        command.current_dir(dir);
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| true)
        .map_err(|e| OpenError::Failed(e.to_string()))
}

struct Search {
    target: String,
    found: Option<HWND>,
}

fn normalized(path: &str) -> String {
    path.replace('/', "\\").to_lowercase()
}

/// Matches by executable path, not title: a browser tab or Explorer folder can
/// carry the same title as the app.
fn find_window(exe: &Path) -> Option<HWND> {
    let mut search = Search {
        target: normalized(&exe.to_string_lossy()),
        found: None,
    };
    // SAFETY: `search` outlives the synchronous enumeration. EnumWindows reports
    // an error when the callback stops it early, which is the success case here.
    let _ = unsafe { EnumWindows(Some(visit), LPARAM(&mut search as *mut Search as isize)) };
    search.found
}

unsafe extern "system" fn visit(hwnd: HWND, lparam: LPARAM) -> BOOL {
    // SAFETY: lparam is the &mut Search passed by find_window.
    let search = unsafe { &mut *(lparam.0 as *mut Search) };
    // SAFETY: the handle comes from EnumWindows.
    let is_main = unsafe { IsWindowVisible(hwnd).as_bool() && GetWindow(hwnd, GW_OWNER).is_err() };
    if is_main && process_path(hwnd).is_some_and(|p| normalized(&p) == search.target) {
        search.found = Some(hwnd);
        return false.into();
    }
    true.into()
}

fn process_path(hwnd: HWND) -> Option<String> {
    let mut pid = 0u32;
    // SAFETY: pid is a valid out pointer for the duration of the call.
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    // SAFETY: the process handle is closed before returning, and the buffer
    // length passed matches the buffer.
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buffer = [0u16; 1024];
        let mut len = buffer.len() as u32;
        let result = QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &mut len,
        );
        let _ = CloseHandle(process);
        result.ok()?;
        Some(String::from_utf16_lossy(&buffer[..len as usize]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launch_is_held_back_only_within_the_grace_period() {
        let now = Instant::now();
        assert!(launch_allowed(None, now));
        assert!(!launch_allowed(Some(now), now + Duration::from_secs(5)));
        assert!(launch_allowed(Some(now), now + LAUNCH_GRACE));
    }

    #[test]
    fn paths_compare_regardless_of_case_and_slashes() {
        assert_eq!(
            normalized("D:/Program Files/Forscher/forscher.EXE"),
            normalized(r"d:\program files\forscher\forscher.exe")
        );
    }

    #[test]
    fn missing_exe_reports_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let result = open(&dir.path().join("absent.exe"), None);
        assert_eq!(result, Err(OpenError::NotFound));
    }
}
