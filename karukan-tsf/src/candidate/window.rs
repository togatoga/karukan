//! Win32 candidate window implementation.
//!
//! Creates a topmost popup window that displays conversion candidates
//! near the text cursor position. Uses GDI for rendering.

#[cfg(target_os = "windows")]
use windows::Win32::Foundation::*;
#[cfg(target_os = "windows")]
use windows::Win32::Graphics::Gdi::*;
#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::*;
#[cfg(target_os = "windows")]
use windows::core::*;

#[cfg(target_os = "windows")]
const CANDIDATE_ITEM_HEIGHT: i32 = 24;
#[cfg(target_os = "windows")]
const CANDIDATE_PADDING: i32 = 4;
#[cfg(target_os = "windows")]
const WINDOW_CLASS_NAME: PCWSTR = w!("KarukanCandidateWindow");

/// Render data shared between the candidate window and the WNDPROC.
#[cfg(target_os = "windows")]
#[derive(Default)]
struct CandidateRenderData {
    candidates: Vec<String>,
    selected: usize,
}

/// Candidate window for displaying conversion candidates.
#[cfg(target_os = "windows")]
pub struct CandidateWindow {
    hwnd: HWND,
}

#[cfg(target_os = "windows")]
#[allow(clippy::new_without_default)]
impl CandidateWindow {
    /// Create a new candidate window (hidden by default).
    ///
    /// Render data is stored per-window via `GWLP_USERDATA` (no global static).
    pub fn new() -> Self {
        register_window_class();
        let hwnd = create_candidate_window();

        // Allocate per-instance render data and attach to the window
        if hwnd.0 as usize != 0 {
            let data = Box::new(CandidateRenderData::default());
            unsafe {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(data) as isize);
            }
        }

        Self { hwnd }
    }

    /// Show the candidate window with the given candidates.
    pub fn show(&mut self, candidates: &[String], selected: usize) {
        if self.hwnd.0 as usize == 0 {
            return;
        }

        unsafe {
            let ptr = GetWindowLongPtrW(self.hwnd, GWLP_USERDATA) as *mut CandidateRenderData;
            if !ptr.is_null() {
                (*ptr).candidates = candidates.to_vec();
                (*ptr).selected = selected;
            }
        }

        let count = candidates.len().max(1) as i32;
        let height = count * CANDIDATE_ITEM_HEIGHT + CANDIDATE_PADDING * 2;
        let width = self.calculate_width(candidates);

        unsafe {
            let _ = SetWindowPos(
                self.hwnd,
                HWND_TOPMOST,
                0,
                0,
                width,
                height,
                SWP_NOMOVE | SWP_NOACTIVATE,
            );
            let _ = InvalidateRect(self.hwnd, None, true);
            let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
        }
    }

    /// Hide the candidate window.
    pub fn hide(&mut self) {
        if self.hwnd.0 as usize == 0 {
            return;
        }
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_HIDE);
        }
    }

    /// Update the selected candidate index.
    pub fn set_selected(&mut self, index: usize) {
        if self.hwnd.0 as usize == 0 {
            return;
        }
        unsafe {
            let ptr = GetWindowLongPtrW(self.hwnd, GWLP_USERDATA) as *mut CandidateRenderData;
            if !ptr.is_null() {
                (*ptr).selected = index;
            }
            let _ = InvalidateRect(self.hwnd, None, true);
        }
    }

    /// Move the window to the given screen coordinates.
    pub fn move_to(&mut self, x: i32, y: i32) {
        if self.hwnd.0 as usize == 0 {
            return;
        }
        unsafe {
            let _ = SetWindowPos(
                self.hwnd,
                HWND_TOPMOST,
                x,
                y,
                0,
                0,
                SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
    }

    /// Destroy the window and free the per-instance render data.
    pub fn destroy(&mut self) {
        if self.hwnd.0 as usize != 0 {
            unsafe {
                // Reclaim and drop the per-instance render data
                let ptr = GetWindowLongPtrW(self.hwnd, GWLP_USERDATA) as *mut CandidateRenderData;
                if !ptr.is_null() {
                    SetWindowLongPtrW(self.hwnd, GWLP_USERDATA, 0);
                    let _ = Box::from_raw(ptr);
                }
                let _ = DestroyWindow(self.hwnd);
            }
            self.hwnd = HWND::default();
        }
    }

    fn calculate_width(&self, candidates: &[String]) -> i32 {
        let max_chars = candidates
            .iter()
            .map(|c| c.chars().count())
            .max()
            .unwrap_or(4);
        // ~14px per CJK character + label width + padding
        let label_width = 28; // "9. "
        (max_chars as i32 * 14 + label_width + CANDIDATE_PADDING * 2).max(120)
    }
}

#[cfg(target_os = "windows")]
impl Drop for CandidateWindow {
    fn drop(&mut self) {
        self.destroy();
    }
}

#[cfg(target_os = "windows")]
fn register_window_class() {
    use std::sync::Once;
    static REGISTERED: Once = Once::new();
    REGISTERED.call_once(|| unsafe {
        let wc = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(candidate_wnd_proc),
            hbrBackground: GetSysColorBrush(COLOR_WINDOW),
            lpszClassName: WINDOW_CLASS_NAME,
            ..Default::default()
        };
        RegisterClassW(&wc);
    });
}

#[cfg(target_os = "windows")]
fn create_candidate_window() -> HWND {
    unsafe {
        CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            WINDOW_CLASS_NAME,
            w!(""),
            WS_POPUP | WS_BORDER,
            0,
            0,
            200,
            100,
            None,
            None,
            None,
            None,
        )
        .unwrap_or_default()
    }
}

#[cfg(target_os = "windows")]
unsafe extern "system" fn candidate_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_PAINT => {
            unsafe { paint_candidates(hwnd) };
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

#[cfg(target_os = "windows")]
unsafe fn paint_candidates(hwnd: HWND) {
    unsafe {
        let mut ps = PAINTSTRUCT::default();
        let hdc = BeginPaint(hwnd, &mut ps);

        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const CandidateRenderData;
        if !ptr.is_null() {
            let data = &*ptr;
            let _ = SetBkMode(hdc, TRANSPARENT);

            // Highlight color for selected item
            let highlight_brush = CreateSolidBrush(COLORREF(0x00D77800));

            for (i, candidate) in data.candidates.iter().enumerate() {
                let y = CANDIDATE_PADDING + i as i32 * CANDIDATE_ITEM_HEIGHT;

                if i == data.selected {
                    let rect = RECT {
                        left: 0,
                        top: y,
                        right: ps.rcPaint.right,
                        bottom: y + CANDIDATE_ITEM_HEIGHT,
                    };
                    FillRect(hdc, &rect, highlight_brush);
                    SetTextColor(hdc, COLORREF(0x00FFFFFF));
                } else {
                    SetTextColor(hdc, COLORREF(0x00000000));
                }

                // Draw "N. candidate" label
                let label = format!("{}. {}", i + 1, candidate);
                let label_wide: Vec<u16> = label.encode_utf16().collect();
                let _ = TextOutW(hdc, CANDIDATE_PADDING, y + 3, &label_wide);
            }

            let _ = DeleteObject(highlight_brush);
        }

        let _ = EndPaint(hwnd, &ps);
    }
}
