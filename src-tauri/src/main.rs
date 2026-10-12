#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod engine;
mod ntfy;
mod overlay;
mod win32;

use std::sync::Mutex;
use std::time::Duration;

use microbreaks_core::{BreakKind, BreakSpec, Input};
use serde::Serialize;
use tauri::menu::{MenuBuilder, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, RunEvent, State, WebviewWindow};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};

use engine::{Engine, Shared, TrayUi};

const TICK: Duration = Duration::from_secs(2);

#[derive(Serialize)]
struct BreakView {
    spec: BreakSpec,
    panel: bool,
    opacity: f32,
    input_grace_ms: u64,
}

#[tauri::command]
fn current_break(window: WebviewWindow, shared: State<Shared>) -> Option<BreakView> {
    let e = shared.0.lock().unwrap();
    Some(BreakView {
        spec: e.current.clone()?,
        panel: overlay::is_panel(window.label()),
        opacity: e.cfg.overlay.opacity,
        input_grace_ms: e.cfg.overlay.input_grace_ms,
    })
}

#[tauri::command]
async fn break_done(app: AppHandle, id: u64, reps: Vec<u32>) {
    engine::push(&app, Input::BreakDone { id, reps });
    engine::tick(&app);
}

#[tauri::command]
async fn break_skip(app: AppHandle, id: u64) {
    engine::push(&app, Input::BreakSkipped { id });
    engine::tick(&app);
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, None))
        .plugin(tauri_plugin_notification::init())
        .invoke_handler(tauri::generate_handler![current_break, break_done, break_skip])
        .setup(|app| {
            let (mut engine, first_run) = Engine::load();
            let status = engine.status(chrono::Local::now().naive_local());

            // Start on login by default. Only on first run, so a later opt-out sticks, and never
            // from debug builds, which would register the dev binary.
            if first_run && cfg!(not(debug_assertions)) {
                let _ = app.autolaunch().enable();
            }

            let header = MenuItem::with_id(app, "status", &status, false, None::<&str>)?;
            let day_toggle = MenuItem::with_id(app, "day_toggle", "Done for the day", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let mut menu = MenuBuilder::new(app).item(&header).separator().item(&day_toggle);
            // Debug builds only: trigger breaks on demand (also the focus-stealing spike).
            let test_eye = MenuItem::with_id(app, "test_eye", "Test eye break", true, None::<&str>)?;
            let test_move = MenuItem::with_id(app, "test_move", "Test movement break", true, None::<&str>)?;
            if cfg!(debug_assertions) {
                menu = menu.separator().item(&test_eye).item(&test_move);
            }
            let menu = menu.separator().item(&quit).build()?;

            let tray = TrayIconBuilder::with_id("main")
                .icon(app.default_window_icon().expect("bundle icon").clone())
                .tooltip(&status)
                .menu(&menu)
                .on_menu_event(|app, event| {
                    match event.id().as_ref() {
                        "day_toggle" => engine::toggle_day(app),
                        "test_eye" => engine::push(app, Input::TakeBreak(BreakKind::Eye)),
                        "test_move" => engine::push(app, Input::TakeBreak(BreakKind::Move)),
                        "quit" => return app.exit(0),
                        _ => return,
                    }
                    engine::tick(app);
                })
                .build(app)?;

            if engine.cfg.ntfy.is_ready() {
                ntfy::spawn_subscriber(app.handle().clone(), engine.cfg.ntfy.clone());
            } else if engine.cfg.ntfy.enabled {
                eprintln!("microbreaks: [ntfy] is enabled but needs two different topics (1-64 letters, digits, _ or -)");
            }

            engine.tray_ui = Some(TrayUi { tray, header, day_toggle });
            app.manage(Shared(Mutex::new(engine)));

            let handle = app.handle().clone();
            std::thread::spawn(move || loop {
                engine::tick(&handle);
                std::thread::sleep(TICK);
            });
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error building microbreaks")
        .run(|_app, event| {
            // Tray-only app: closing the last overlay must not quit.
            if let RunEvent::ExitRequested { api, code, .. } = event {
                if code.is_none() {
                    api.prevent_exit();
                }
            }
        });
}
