use microbreaks_core::config::{Config, Monitors};
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder};

use crate::win32;

const LABEL_PREFIX: &str = "overlay-";
const PANEL_SUFFIX: &str = "-panel";

/// True when `label` names the overlay window that carries the break panel.
pub fn is_panel(label: &str) -> bool {
    label.starts_with(LABEL_PREFIX) && label.ends_with(PANEL_SUFFIX)
}

/// Replace any overlay with a fresh one for break `id`: a dim window on every monitor (or just
/// the active one), with the panel on the monitor under the cursor.
pub fn show(app: &AppHandle, cfg: &Config, id: u64) {
    let app = app.clone();
    let which = cfg.overlay.monitors;
    // Window creation must happen on the main thread.
    let _ = app.clone().run_on_main_thread(move || {
        close_now(&app);
        let monitors = app.available_monitors().unwrap_or_default();
        let cursor = app.cursor_position().ok();
        let on_cursor = |m: &tauri::Monitor| {
            let (p, s) = (m.position(), m.size());
            cursor.is_some_and(|c| {
                c.x >= p.x as f64 && c.x < (p.x + s.width as i32) as f64 && c.y >= p.y as f64 && c.y < (p.y + s.height as i32) as f64
            })
        };
        let panel_idx = monitors.iter().position(on_cursor).unwrap_or(0);
        for (i, m) in monitors.iter().enumerate() {
            let panel = i == panel_idx;
            if which == Monitors::Active && !panel {
                continue;
            }
            let label = format!("{LABEL_PREFIX}{id}-{i}{}", if panel { PANEL_SUFFIX } else { "" });
            let built = WebviewWindowBuilder::new(&app, &label, WebviewUrl::App("overlay.html".into()))
                .decorations(false)
                .always_on_top(true)
                .skip_taskbar(true)
                .transparent(true)
                .resizable(false)
                .shadow(false)
                .visible(false)
                .build();
            let Ok(w) = built else { continue };
            // Physical units, so mixed-DPI monitors are covered exactly.
            let _ = w.set_position(PhysicalPosition::new(m.position().x, m.position().y));
            let _ = w.set_size(PhysicalSize::new(m.size().width, m.size().height));
            let _ = w.show();
            if panel {
                let _ = w.set_focus();
                if let Ok(hwnd) = w.hwnd() {
                    win32::force_foreground(hwnd.0 as isize);
                }
            }
        }
    });
}

pub fn close(app: &AppHandle) {
    let app = app.clone();
    let _ = app.clone().run_on_main_thread(move || close_now(&app));
}

fn close_now(app: &AppHandle) {
    for (label, w) in app.webview_windows() {
        if label.starts_with(LABEL_PREFIX) {
            let _ = w.destroy();
        }
    }
}
