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
            .title("Today's stats")
            .inner_size(420.0, 560.0)
            .resizable(false)
            .center()
            .build();
        if let Ok(w) = built {
            let _ = w.set_focus();
        }
    });
}
