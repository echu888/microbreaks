//! Windows-only probes and focus helpers. Non-Windows builds get inert fallbacks so the
//! crate still type-checks elsewhere.

#[cfg(windows)]
mod imp {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::SystemInformation::GetTickCount;
    use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetLastInputInfo, SetFocus, keybd_event, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP, LASTINPUTINFO, VK_MENU,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        BringWindowToTop, GetForegroundWindow, GetWindowThreadProcessId, SetForegroundWindow,
    };

    /// Seconds since the last keyboard/mouse input (`GetLastInputInfo`).
    pub fn idle_secs() -> u64 {
        let mut info = LASTINPUTINFO { cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
        unsafe {
            if !GetLastInputInfo(&mut info).as_bool() {
                return 0;
            }
            u64::from(GetTickCount().wrapping_sub(info.dwTime)) / 1000
        }
    }

    /// Get keyboard focus for `hwnd` despite the foreground lock. Must run on the thread
    /// that owns the window. Tries `AttachThreadInput` first, then a synthetic Alt press.
    pub fn force_foreground(hwnd: isize) {
        let hwnd = HWND(hwnd as _);
        unsafe {
            let this_thread = GetCurrentThreadId();
            let fg_thread = GetWindowThreadProcessId(GetForegroundWindow(), None);
            let attached = fg_thread != 0
                && fg_thread != this_thread
                && AttachThreadInput(this_thread, fg_thread, true).as_bool();
            let _ = SetForegroundWindow(hwnd);
            let _ = BringWindowToTop(hwnd);
            let _ = SetFocus(Some(hwnd));
            if attached {
                let _ = AttachThreadInput(this_thread, fg_thread, false);
            }
            if GetForegroundWindow() != hwnd {
                keybd_event(VK_MENU.0 as u8, 0, KEYBD_EVENT_FLAGS(0), 0);
                keybd_event(VK_MENU.0 as u8, 0, KEYEVENTF_KEYUP, 0);
                let _ = SetForegroundWindow(hwnd);
            }
        }
    }
}

#[cfg(windows)]
pub use imp::{force_foreground, idle_secs};

#[cfg(not(windows))]
pub fn idle_secs() -> u64 {
    0
}

#[cfg(not(windows))]
pub fn force_foreground(_hwnd: isize) {}
