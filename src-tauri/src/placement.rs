//! Keeps the card at the bottom right of the primary monitor's work area.
//!
//! Win32 is used directly because showing through Tauri can activate the window,
//! which would steal keyboard focus whenever a count changes the card back on.

use tauri::{Runtime, WebviewWindow};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{
    SetWindowPos, ShowWindow, HWND_TOPMOST, SWP_NOACTIVATE, SWP_SHOWWINDOW, SW_HIDE,
};

// Logical pixels. HEADER_H and ROW_H mirror frontend/src/style.css.
const CARD_W: f64 = 240.0;
const HEADER_H: f64 = 28.0;
const ROW_H: f64 = 38.0;
const BOTTOM_PAD: f64 = 6.0;
const BORDER: f64 = 1.0;
const MARGIN: f64 = 12.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

pub fn card_rect(work_area: Rect, scale: f64, rows: usize) -> Rect {
    let w = (CARD_W * scale).round() as i32;
    let logical_h = HEADER_H + ROW_H * rows as f64 + BOTTOM_PAD + 2.0 * BORDER;
    let h = (logical_h * scale).round() as i32;
    let margin = (MARGIN * scale).round() as i32;
    Rect {
        x: work_area.x + work_area.w - w - margin,
        y: work_area.y + work_area.h - h - margin,
        w,
        h,
    }
}

pub fn hide<R: Runtime>(window: &WebviewWindow<R>) -> tauri::Result<()> {
    let hwnd = HWND(window.hwnd()?.0 as _);
    // SAFETY: the handle comes from a live Tauri window.
    let _ = unsafe { ShowWindow(hwnd, SW_HIDE) };
    Ok(())
}

pub fn show<R: Runtime>(window: &WebviewWindow<R>, rows: usize) -> Result<(), String> {
    let monitor = window
        .primary_monitor()
        .map_err(|e| e.to_string())?
        .ok_or("no primary monitor")?;
    let area = monitor.work_area();
    let rect = card_rect(
        Rect {
            x: area.position.x,
            y: area.position.y,
            w: area.size.width as i32,
            h: area.size.height as i32,
        },
        monitor.scale_factor(),
        rows,
    );
    let hwnd = HWND(window.hwnd().map_err(|e| e.to_string())?.0 as _);
    // SAFETY: the handle comes from a live Tauri window. Re-asserting TOPMOST on
    // every refresh also recovers from anything that dropped it from the band.
    unsafe {
        SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            rect.x,
            rect.y,
            rect.w,
            rect.h,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        )
    }
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL_HD: Rect = Rect {
        x: 0,
        y: 0,
        w: 1920,
        h: 1032,
    };

    #[test]
    fn sits_inside_the_bottom_right_corner() {
        let r = card_rect(FULL_HD, 1.0, 2);
        assert_eq!((r.w, r.h), (240, 112));
        assert_eq!((r.x + r.w, r.y + r.h), (1908, 1020));
    }

    #[test]
    fn height_follows_the_row_count() {
        assert_eq!(card_rect(FULL_HD, 1.0, 1).h, 74);
    }

    #[test]
    fn scales_with_dpi_and_respects_the_work_area_origin() {
        let area = Rect {
            x: 100,
            y: 50,
            w: 2560,
            h: 1400,
        };
        let r = card_rect(area, 1.5, 2);
        assert_eq!((r.w, r.h), (360, 168));
        assert_eq!((r.x + r.w, r.y + r.h), (100 + 2560 - 18, 50 + 1400 - 18));
    }
}
