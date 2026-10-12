# microbreaks — spec (draft v0.1)

A small desktop app that enforces two habits during the work day:

- **20-20-20 eye rule** — every 20 minutes, look at something 20 feet away for 20 seconds.
- **Hourly squats** — on the hour, every hour, stand up and do squats.

Primary target is **Windows**. A phone companion (iOS / PWA) is a possible later phase.

## Goals

- Interrupt firmly enough that breaks actually happen, but **never lock the user out** — one keypress gets back to work.
- Zero-effort when not needed: stay quiet in meetings, fullscreen apps, and when the user is already away from the keyboard.
- Record what happened to each break (done / skipped) for simple stats.
- Small, low-resource, starts with Windows, lives in the tray.

## Non-goals (for now)

- Rep counting, exercise variety, or workout tracking.
- Multi-user, cloud accounts, or syncing between devices.
- Calendar integration (camera/mic detection covers most meetings).

## Behaviour

### Schedule

- Breaks follow the **wall clock**, not a rolling timer.
  - Eye break at `:20` and `:40`.
  - Squat break at `:00`. It also counts as the eye break for that slot.
- **Work day:** Mon–Fri, 08:00–17:00 (configurable in the config file).
  - First break of the day is 08:20 (eye); last is 17:00 (squat).
  - 08:00 does not fire, since the day is just starting.
- **No backfill:** if the PC was asleep or locked at a break time, that break is logged as `missed` and is not shown later.
- **One overlay at a time:** if a break is still on screen when the next one comes due, the old one is logged as `missed` and the new one takes its place. In practice this only matters when the user is away; see the idle rules below.

### The break overlay

- A borderless, always-on-top, semi-transparent dim window on **every monitor**. Looking away from all screens is the point.
- The **primary panel** (message, countdown, buttons) appears only on the monitor that has the mouse cursor. The other monitors are just dimmed.
- Configurable later: `overlay.monitors = "all" | "active"`.
- Content:
  - **Eye break:** "Look 20 ft away", with a 20 s countdown shown for guidance only.
  - **Squat break:** "Squat time", with the configured rep target (e.g. "10 squats").
- The overlay **does not auto-dismiss**. The user closes it with:
  - **Done** (button, or `Enter` / `Space` once the countdown finishes) → logged as `done`.
  - **Skip** (button, or `Esc` at any time) → logged as `skipped`.
- The overlay takes keyboard focus when it appears, so `Esc` works straight away.
- Alt-Tab, the Windows key, etc. are not blocked.

### Smart suppression

Checked at the moment a break comes due.

| Condition | Detection (Windows) | Action |
|---|---|---|
| User idle ≥ 20 s (eye) / ≥ 2 min (squat) | `GetLastInputInfo` | Log `auto_done` (already away), no overlay |
| Session locked / screensaver | `WTSRegisterSessionNotification` / lock state | Log `missed`, no overlay |
| Camera or mic in use (meeting) | `CapabilityAccessManager\ConsentStore` registry (see below) | Log `suppressed_meeting` |
| Fullscreen app / presentation / Focus mode | `SHQueryUserNotificationState` | Log `suppressed_busy` |

Deferred squats (decide later): optionally re-offer a suppressed squat break once the meeting ends, if that happens within e.g. 15 min.

#### Camera/mic detection

This is easy. Windows records per-app capability use under

```
HKCU\Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\webcam
HKCU\Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\microphone
```

- There is one subkey per app; desktop apps sit under a `NonPackaged` subkey.
- A device is **in use** when some app has `LastUsedTimeStop == 0` (with a non-zero `LastUsedTimeStart`).
- Reading these keys at break time is enough; no polling or permissions needed.
- Caveats:
  - The key is undocumented, but it has been stable since Win10 1903 and is what the privacy tray icon uses.
  - Some apps keep the mic open while idle, so we may need an app ignore list.

### Tray

- Tray icon showing state (active / paused / outside work hours).
- Menu:
  - Next break at HH:MM
  - Pause for 30 min / 1 h / rest of day
  - Take a break now
  - Open config / settings
  - Today's stats
  - Quit
- Starts on login (on by default).

### Event log & stats

- Each break is appended to `%APPDATA%\microbreaks\events.jsonl`:
  `{ "ts", "kind": "eye"|"squat", "outcome": "done"|"skipped"|"auto_done"|"missed"|"suppressed_meeting"|"suppressed_busy", "shown_ms" }`
- Stats view (later): per day/week counts of done vs skipped, squat completion rate.

## Configuration

`%APPDATA%\microbreaks\config.toml` is created with defaults on first run and reloaded when the file changes. A settings window comes later and edits the same file.

```toml
[workday]
days  = ["mon", "tue", "wed", "thu", "fri"]
start = "08:00"
end   = "17:00"

[eye]
enabled        = true
minutes        = [20, 40]     # wall-clock minutes; :00 is covered by squats
duration_secs  = 20
idle_skip_secs = 20

[squat]
enabled        = true
reps           = 10
idle_skip_secs = 120

[overlay]
monitors = "all"              # "all" | "active"
opacity  = 0.85

[suppress]
camera_mic        = true
fullscreen        = true
ignore_mic_apps   = []        # exe names to ignore for mic detection
```

## Architecture

**Stack:** Tauri 2. The Rust backend holds all logic; the overlay and settings UI are a thin TypeScript layer (Vite, plain TS or Svelte).

```
microbreaks/
  crates/core/        # pure Rust, no OS deps: config, schedule, break state machine, event model
  src-tauri/          # Tauri app: tray, overlay windows, Win32 probes, autostart, log writer
  ui/                 # overlay + settings + stats pages (web)
```

- **`core` stays platform-free and fully unit-tested.**
  - `next_break(now, config) -> (time, kind)`
  - The state machine: `Idle → Due → Showing → Resolved(outcome)`.
  - A future iOS app could reuse it (via uniffi), or a PWA (via wasm).
- **Platform probes** sit behind a trait (`Presence`: `idle_secs()`, `is_locked()`, `in_meeting()`, `is_busy()`), so the scheduler is testable with fakes.
- **Overlay windows:** one Tauri webview window per monitor, created at break time and destroyed afterwards. Not kept alive, to save memory.

**Why Tauri over alternatives:**
- Small binary.
- Native tray, autostart and notification plugins.
- The web UI is reusable for a PWA.
- Rust gives easy Win32 access via the `windows` crate.
- Rejected:
  - C#/WinUI: Windows-only.
  - Electron: heavy.
  - Pure-Rust UI (egui/Slint): no web reuse.

## Milestones

1. **M0 — Skeleton:** Tauri 2 project, tray icon with Quit, autostart, config file load with defaults.
2. **M1 — Core loop (MVP):** wall-clock scheduler, work hours, eye + squat overlays on all monitors, Done/Esc, event log. *This is the version to start using daily.*
3. **M2 — Smart suppression:** idle detection, lock detection, camera/mic, fullscreen/busy.
4. **M3 — Polish:**
   - Pause menu and "take a break now".
   - Today's stats.
   - Optional 10 s heads-up toast before each break.
   - Settings window.
5. **M4 — Distribution:** code signing, MSIX packaging, Microsoft Store submission (if it's worth it).
6. **Future — Phone:** see below.

## Future: phone companion (undecided)

Two candidate roles, which could be combined:

- **Off-hours reminder:** standalone schedule on the phone, notifications only.
  - iOS does not allow interrupting overlays; time-sensitive notifications are the ceiling.
  - Needs a native iOS app (Tauri 2 mobile or Swift + shared `core`), because iOS PWAs can't schedule local notifications without a push server.
- **Backup buzzer:** the phone alerts only when the desktop reports the user is away, or when a break went unacknowledged.
  - Needs a way for the desktop to reach the phone, e.g. a tiny push relay or a service like ntfy/Pushover.
  - A Pushover/ntfy call from the desktop is the cheapest prototype and needs no phone app at all.

## Open questions

- Squat rep target: is 10 a good default?
- Should a squat suppressed by a meeting be re-offered after it ends?
- Heads-up toast before the overlay: wanted, or just noise?
- Should `Esc` need a second press (or a hold) to avoid accidental skips while typing?
