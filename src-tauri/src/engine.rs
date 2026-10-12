use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

use microbreaks_core::config::DEFAULT_CONFIG_TOML;
use microbreaks_core::{status_line, step, BreakSpec, Config, Effect, Event, Input, Presence, State};
use tauri::menu::MenuItem;
use tauri::tray::TrayIcon;
use tauri::{AppHandle, Manager};

use crate::{overlay, win32};

/// Tray widgets that mirror the status line.
pub struct TrayUi {
    pub tray: TrayIcon,
    pub header: MenuItem<tauri::Wry>,
    /// "Done for the day" while the day is running, "Resume" once it has ended.
    pub day_toggle: MenuItem<tauri::Wry>,
}

pub struct Engine {
    state: State,
    pub cfg: Config,
    inputs: Vec<Input>,
    /// The break whose overlay is up; overlay pages fetch this on load.
    pub current: Option<BreakSpec>,
    events_path: PathBuf,
    /// What the tray currently shows: status line and whether the day has ended.
    shown: (String, bool),
    pub tray_ui: Option<TrayUi>,
}

pub struct Shared(pub Mutex<Engine>);

/// `%APPDATA%\microbreaks`
fn data_dir() -> PathBuf {
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    base.join("microbreaks")
}

/// Loads `config.toml`, writing the commented defaults on first run.
/// Returns the config and whether this was the first run.
fn load_config(dir: &PathBuf) -> (Config, bool) {
    let path = dir.join("config.toml");
    let _ = fs::create_dir_all(dir);
    match fs::read_to_string(&path) {
        Ok(text) => match Config::from_toml(&text) {
            Ok(cfg) => (cfg, false),
            Err(e) => {
                eprintln!("microbreaks: {} is invalid ({e}); using defaults", path.display());
                (Config::default(), false)
            }
        },
        Err(_) => {
            if let Err(e) = fs::write(&path, DEFAULT_CONFIG_TOML) {
                eprintln!("microbreaks: could not write {}: {e}", path.display());
            }
            (Config::default(), true)
        }
    }
}

impl Engine {
    pub fn load() -> (Self, bool) {
        let dir = data_dir();
        let (cfg, first_run) = load_config(&dir);
        let engine = Engine {
            state: State::default(),
            cfg,
            inputs: Vec::new(),
            current: None,
            events_path: dir.join("events.jsonl"),
            shown: (String::new(), false),
            tray_ui: None,
        };
        (engine, first_run)
    }

    pub fn status(&self, now: chrono::NaiveDateTime) -> String {
        status_line(&self.state, &self.cfg, now)
    }

    fn log(&self, event: &Event) {
        let Ok(line) = serde_json::to_string(event) else { return };
        let file = OpenOptions::new().create(true).append(true).open(&self.events_path);
        if let Err(e) = file.and_then(|mut f| writeln!(f, "{line}")) {
            eprintln!("microbreaks: could not write event log: {e}");
        }
    }
}

pub fn push(app: &AppHandle, input: Input) {
    app.state::<Shared>().0.lock().unwrap().inputs.push(input);
}

/// The tray's single day-end item: ends the day, or resumes it if it has already ended.
pub fn toggle_day(app: &AppHandle) {
    let today = chrono::Local::now().date_naive();
    let shared = app.state::<Shared>();
    let mut e = shared.0.lock().unwrap();
    let input = if e.state.day_ended(today) { Input::Resume } else { Input::DoneForDay };
    e.inputs.push(input);
}

/// Run one scheduler step and carry out its effects.
pub fn tick(app: &AppHandle) {
    let shared = app.state::<Shared>();
    let now = chrono::Local::now().naive_local();
    let idle_secs = win32::idle_secs();

    // Only touch engine state under the lock. Overlay and tray calls post work to the main
    // thread (which also takes this lock), so they run after the guard is dropped.
    let (fx, cfg, new_status) = {
        let mut guard = shared.0.lock().unwrap();
        let e = &mut *guard;
        let inputs = std::mem::take(&mut e.inputs);
        let fx = step(&mut e.state, &e.cfg, now, Presence { idle_secs }, &inputs);
        for effect in &fx {
            match effect {
                Effect::ShowOverlay(spec) => e.current = Some(spec.clone()),
                Effect::CloseOverlay => e.current = None,
                Effect::Log(event) => e.log(event),
            }
        }
        let shown = (status_line(&e.state, &e.cfg, now), e.state.day_ended(now.date()));
        let changed = shown != e.shown;
        if changed {
            e.shown = shown.clone();
        }
        let tray = changed.then(|| {
            let ui = e.tray_ui.as_ref();
            (ui.map(|u| (u.tray.clone(), u.header.clone(), u.day_toggle.clone())), shown)
        });
        (fx, e.cfg.clone(), tray)
    };

    for effect in fx {
        match effect {
            Effect::ShowOverlay(spec) => overlay::show(app, &cfg, spec.id),
            Effect::CloseOverlay => overlay::close(app),
            Effect::Log(_) => {}
        }
    }
    if let Some((Some((tray, header, day_toggle)), (status, ended))) = new_status {
        let _ = header.set_text(&status);
        let _ = tray.set_tooltip(Some(&status));
        let _ = day_toggle.set_text(if ended { "Resume" } else { "Done for the day" });
    }
}
