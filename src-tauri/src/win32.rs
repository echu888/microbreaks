//! Windows-only probes and focus helpers. Non-Windows builds get inert fallbacks so the
//! crate still type-checks elsewhere.

#[cfg(windows)]
mod imp {
    use windows::core::PWSTR;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::RemoteDesktop::{
        ProcessIdToSessionId, WTSFreeMemory, WTSQuerySessionInformationW, WTSINFOEXW, WTSSessionInfoEx,
        WTS_CURRENT_SERVER_HANDLE,
    };
    use windows::Win32::System::SystemInformation::GetTickCount;
    use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentProcessId, GetCurrentThreadId};
    use windows::Win32::UI::Shell::{
        SHQueryUserNotificationState, QUNS_BUSY, QUNS_PRESENTATION_MODE, QUNS_RUNNING_D3D_FULL_SCREEN,
    };
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;
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

    /// Whether this session's workstation is locked (`WTSINFOEX` session flags).
    pub fn is_locked() -> bool {
        const WTS_SESSIONSTATE_LOCK: i32 = 0;
        unsafe {
            let mut session = 0;
            if ProcessIdToSessionId(GetCurrentProcessId(), &mut session).is_err() {
                return false;
            }
            let mut buf = PWSTR::null();
            let mut len = 0;
            if WTSQuerySessionInformationW(Some(WTS_CURRENT_SERVER_HANDLE), session, WTSSessionInfoEx, &mut buf, &mut len)
                .is_err()
                || buf.is_null()
            {
                return false;
            }
            let info = &*(buf.0 as *const WTSINFOEXW);
            let locked = info.Level == 1 && info.Data.WTSInfoExLevel1.SessionFlags == WTS_SESSIONSTATE_LOCK;
            WTSFreeMemory(buf.0 as _);
            locked
        }
    }

    /// Fullscreen app or presentation mode (`SHQueryUserNotificationState`).
    /// Ignored while the foreground window is ours, since the break overlay is itself fullscreen.
    pub fn is_busy() -> bool {
        unsafe {
            let mut pid = 0;
            GetWindowThreadProcessId(GetForegroundWindow(), Some(&mut pid));
            if pid == GetCurrentProcessId() {
                return false;
            }
            matches!(
                SHQueryUserNotificationState(),
                Ok(QUNS_BUSY | QUNS_RUNNING_D3D_FULL_SCREEN | QUNS_PRESENTATION_MODE)
            )
        }
    }

    const CONSENT_STORE: &str = r"Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore";

    /// Whether any app has the camera or microphone open right now (per-app `ConsentStore` entries).
    /// `ignore_mic_apps` are exe names (e.g. `discord.exe`) skipped for the microphone only.
    pub fn in_meeting(ignore_mic_apps: &[String]) -> bool {
        device_in_use("webcam", &[]) || device_in_use("microphone", ignore_mic_apps)
    }

    fn device_in_use(device: &str, ignore: &[String]) -> bool {
        let Ok(base) = RegKey::predef(HKEY_CURRENT_USER).open_subkey(format!(r"{CONSENT_STORE}\{device}")) else {
            return false;
        };
        base.enum_keys().flatten().any(|name| {
            let Ok(app) = base.open_subkey(&name) else { return false };
            if name != "NonPackaged" {
                return app_in_use(&app);
            }
            // Desktop apps: one subkey per exe, named like `C:#Program Files#Zoom#Zoom.exe`.
            app.enum_keys().flatten().any(|path| {
                let exe = path.rsplit('#').next().unwrap_or(&path);
                !ignore.iter().any(|i| i.eq_ignore_ascii_case(exe))
                    && app.open_subkey(&path).is_ok_and(|k| app_in_use(&k))
            })
        })
    }

    /// In use means started and not yet stopped.
    fn app_in_use(key: &RegKey) -> bool {
        key.get_value::<u64, _>("LastUsedTimeStart").is_ok_and(|t| t != 0)
            && key.get_value::<u64, _>("LastUsedTimeStop").is_ok_and(|t| t == 0)
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
pub use imp::{force_foreground, idle_secs, in_meeting, is_busy, is_locked};

#[cfg(not(windows))]
pub fn idle_secs() -> u64 {
    0
}

#[cfg(not(windows))]
pub fn is_locked() -> bool {
    false
}

#[cfg(not(windows))]
pub fn is_busy() -> bool {
    false
}

#[cfg(not(windows))]
pub fn in_meeting(_ignore_mic_apps: &[String]) -> bool {
    false
}

#[cfg(not(windows))]
pub fn force_foreground(_hwnd: isize) {}
