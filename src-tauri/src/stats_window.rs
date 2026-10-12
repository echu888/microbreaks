use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

const LABEL: &str = "stats";

/// Opens the stats window, or brings the existing one to the front.
pub fn open(app: &AppHandle) {
    let app = app.clone();
    // Window creation must happen on the main thread.
    let _ = app.clone().run_on_main_thread(move || {
        if let Some(w) = app.get_webview_window(LABEL) {
            let _ = w.show();
            let _ = w.set_focus();
            return;
        }
        let built = WebviewWindowBuilder::new(&app, LABEL, WebviewUrl::App("stats.html".into()))
            .title("Stats")
            .inner_size(480.0, 600.0)
            .resizable(false)
            .center()
            .build();
        if let Ok(w) = built {
            let _ = w.set_focus();
        }
    });
}
