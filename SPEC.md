# microbreaks — spec (draft v0.5)

A small desktop app that enforces two habits:

- **20-20-20 eye rule** — every 20 minutes, look at something 20 feet away for 20 seconds.
- **Hourly movement** — on the hour, every hour, stand up and exercise. Squats to start with; the list of exercises is configurable (e.g. squats + push-ups).

Primary target is **Windows**.
- The phone is reached through push notifications ([ntfy](https://ntfy.sh)), so no phone app is needed at first.
- A native phone app is a possible later phase.

## Goals

- Interrupt firmly enough that breaks actually happen, but **never lock the user out**.
- Zero-effort when not needed:
  - Stay quiet in meetings and fullscreen apps.
  - Hold movement breaks until you're back.
  - Follow you to your phone when you're away.
- Record what happened to each break for accurate stats, plus when the work day ended and when you went to sleep.
- Small, low-resource, starts with Windows, lives in the tray.

## Non-goals (for now)

- Automatic rep counting (camera/sensors) or full workout tracking. Reps are self-reported.
- Multi-user, cloud accounts, or syncing between devices.
- Calendar integration (camera/mic detection covers most meetings).

## Day phases

| Phase | When | Breaks | Ends when |
|---|---|---|---|
| **Work** | work days, `workday.start` → `day_end` | eye + movement | `day_end` (see below) |
| **Off-hours** | work days after `day_end`; non-work days from `offhours.weekend_start` | movement hourly (eye optional) | `sleep` (see below) |
| **Sleep** | after `sleep` | none | next morning: `workday.start`, or `offhours.weekend_start` on non-work days |

- **Weekends** (any non-work day) are Off-hours all day, from `weekend_start` (default 09:00) to `sleep`.
- **Pause** is a temporary overlay on any phase (tray "Pause for 30 min / 1 h"). It resumes automatically.

### Phase vs. presence

- **Away is not a phase.** Presence (at PC / away / locked / in a meeting / busy) is checked separately and only decides **how** a break is delivered:
  - **at PC:** desktop overlay.
  - **away / locked:** phone push for movement breaks.
  - **meeting / busy:** held as pending.
- This works the same in Work and Off-hours. The only difference is which breaks run.

### Status display

What the user sees is **phase + presence + next break**, e.g. "Work · next: eye break 10:20" or "Off-hours · away · next: movement 19:00 (→ phone)".

- **Tray icon:** a distinct icon per phase, with a small badge for paused / pending break.
  - Work: active colour. Off-hours: muted colour. Sleep: moon. Paused: pause badge. Pending: dot badge.
- **Tray tooltip:** the full status line above.
- **Tray menu:** the status line as a disabled header item at the top.
- **Phone:** no persistent status. Off-hours pushes say "Off-hours" in the title, so the context is clear.

### Ending the work day (`day_end`)

- **Manual:** tray "Done for the day".
- **Automatic (overtime-aware):** breaks continue normally past `workday.end` for as long as you're active.
  - The day ends at the first idle period of `overtime_idle_mins` or more (default 30), or when the PC locks or sleeps, after `workday.end`.
  - The logged timestamp is the **start** of that idle period, i.e. your real shutdown time.
- **Resume:** the tray "Resume" undoes it and logs `day_resume`.

### Going to sleep (`sleep`)

- **Manual:**
  - A **"Going to sleep"** action button on every Off-hours phone notification (see ntfy below).
  - The same item in the tray.
- **Backstop:** `offhours.quiet_after` (default 22:00). Reminders never run all night.
- The logged timestamp gives bedtime stats next to shutdown-time stats.

### Off-hours limitations

- Off-hours reminders come from the desktop app, so the PC must be on (usually true on weekday evenings, perhaps less so on weekends).
- If the PC is off or asleep, there are no Off-hours reminders. This is accepted for now; fixing it needs a native phone app (see Future).
- iOS Sleep Focus silences notifications anyway, which is a free extra safety net.

## Breaks

### Schedule

- Breaks follow the **wall clock**:
  - Eye break at `:20` and `:40`.
  - Movement break at `:00`. It also counts as the eye break for that slot.
- **Work day:** Mon–Fri, starting 08:00.
  - First break is 08:20; 08:00 does not fire.
  - Breaks continue past 17:00 until `day_end`.
- **Off-hours:** movement breaks hourly at `:00`, until `sleep`.
- **One break at a time:** a newly due break replaces any pending or showing one. The old one is logged `replaced`.

### Break lifecycle

```
Due ──► Blocked? ──no──► Showing ──► done / skipped / replaced
           │
          yes
           ▼
         eye:  logged with the block reason, dropped
         move: Pending ──(clears within grace)──► Showing
                  │
                  ├──(Done/Skip on phone)──► done / skipped  (via: phone)
                  └──(grace expires)───────► logged with the block reason
```

**Block reasons**, checked when a break comes due:

| Reason | Detection (Windows) | Eye break | Movement break |
|---|---|---|---|
| `away`: idle ≥ 20 s (eye) / ≥ 2 min (move) | `GetLastInputInfo` | logged, dropped (eyes already rested) | pending + **phone push** |
| `locked` | session lock state / `WTSRegisterSessionNotification` | logged, dropped | pending + **phone push** |
| `asleep`: PC was asleep at break time | tick gap (see Scheduler) | logged on wake | logged on wake, no push |
| `meeting`: camera or mic in use | `ConsentStore` registry (below) | logged, dropped | pending, no push |
| `busy`: fullscreen / presentation / Focus mode | `SHQueryUserNotificationState` | logged, dropped | pending, no push |

- **Grace window:** a pending movement break is shown as soon as the block clears, if that happens within `move.grace_mins` (default 15).
- **Delays are recorded:** a break that was held and then done is logged as `outcome: "done", delayed_by: "meeting"`.
- **No phone push when:**
  - the break was **explicitly skipped**;
  - the reason is `meeting` or `busy` (you're at the PC).

#### Camera/mic detection

Windows records per-app capability use under

```
HKCU\Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\webcam
HKCU\Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\microphone
```

- There is one subkey per app; desktop apps sit under `NonPackaged`.
- A device is **in use** when some app has `LastUsedTimeStop == 0` (with a non-zero `LastUsedTimeStart`).
- Reading these keys needs no special permissions.
- Caveats:
  - The key is undocumented, but it has been stable since Win10 1903 and backs the privacy tray icon.
  - Some apps keep the mic open while idle, so there's a config ignore list.

### The break overlay

- A borderless, always-on-top, semi-transparent dim window on **every monitor**.
  - The **primary panel** (message, countdown, buttons) appears only on the monitor with the mouse cursor.
  - The other monitors are just dimmed.
  - `overlay.monitors = "all" | "active"`.
- **Eye break:** "Look 20 ft away" with a 20 s countdown.
  - **Done is disabled until the countdown ends**, so `done` means the eyes actually rested.
- **Movement break:** "Movement time", with one row per exercise, each with a rep stepper prefilled with the target (e.g. `Squats [−] 15 [+]`).
  - If you did the target, Done is still one click.
  - Adjust a stepper only when the count differs; 0 means that exercise was skipped.
  - `Tab` moves between rows; digits edit the focused stepper.
- **Closing:** the overlay never auto-dismisses.
  - **Done:** button, or `Enter`.
  - **Skip:** button, or **`Esc` twice within 1 s**. The first `Esc` shows "Press Esc again to skip".
- **Input grace:** all keyboard input is ignored for the first `overlay.input_grace_ms` (default 1000) after it appears.
  - This stops in-flight typing (Enter, Space, digits) from acknowledging the break or editing reps.
  - The Done button follows the same rule.
- **Focus:** the overlay takes keyboard focus when it appears.
  - Alt-Tab, the Windows key, etc. are not blocked.
  - If you switch away and leave it, it stays until resolved or replaced.

> **Technical risk:** Windows' foreground lock can refuse focus to a background app and flash the taskbar instead. This is proven out first in M1 (see Milestones). Known workarounds: `AllowSetForegroundWindow` from the tray process, `AttachThreadInput`, or a simulated Alt keypress before `SetForegroundWindow`.

### Heads-up toast (optional)

- When enabled, a Windows toast appears N seconds before each overlay ("Eye break in 10 s").
- Disabled by default, since it may be noisy.

## Phone via ntfy

[ntfy](https://ntfy.sh) is a free, open-source pub/sub notification service.

- **How it works:** the desktop app publishes with a plain HTTP POST, and the ntfy iOS/Android app shows it as a push notification.
- **Cost:** none. The public `ntfy.sh` server needs no account; paid tiers (reserved topics, higher limits) and self-hosting are optional.
- **Your setup (one-time, ~2 min):**
  1. Install the ntfy app on the phone.
  2. Subscribe to a long random topic name. The app generates one; the name acts as the password, so don't use a guessable one.
  3. Paste that topic into `config.toml`.
- **Two topics:**
  - `alerts`: desktop → phone. Away buzzer and Off-hours reminders.
  - `control`: phone → desktop. The desktop keeps a streaming subscription (`GET /<topic>/json`).
- **Action buttons:** notifications carry ntfy `http` action buttons that publish to the control topic: **Done**, **Skip**, and in Off-hours **Going to sleep**.
  - The desktop receives these and resolves the pending break (logged `via: "phone"`; reps = target) or enters the Sleep phase.
  - *Verify early in M3 that the iOS ntfy app supports `http` actions.* Fallback: a `view` action opening a tiny local-network page, or notifications without buttons plus the tray / `quiet_after` for sleep.
- **Security:** anyone who learns the topic names could send fake Done/Sleep messages. That's acceptable for a personal tool. A reserved, password-protected topic (ntfy Pro or self-hosted) is the upgrade path.

## Tray

- Icon, tooltip and header item show the status (see Status display).
- Menu:
  - *Status line* (header, disabled)
  - Next break at HH:MM
  - Pause for 30 min / 1 h
  - Done for the day / Resume
  - Going to sleep
  - Take a break now
  - Open config / settings
  - Today's stats
  - Quit
- Starts on login (on by default).

## Event log & stats

Events are appended to `%APPDATA%\microbreaks\events.jsonl`. That's about 30 lines a day, so no database is needed.

**Break events:**

```json
{ "ts": "...", "type": "break", "kind": "eye"|"move", "phase": "work"|"offhours",
  "outcome": "...", "delayed_by": "away"|"locked"|"meeting"|"busy"|null,
  "via": "desktop"|"phone", "shown_ms": 0,
  "exercises": [{ "name": "squats", "target": 15, "reps": 15 }] }
```

- `exercises` appears on `move` breaks with outcome `done`.

| outcome | meaning |
|---|---|
| `done` | acknowledged (desktop or phone) |
| `skipped` | explicitly skipped |
| `away` | idle when due; never resolved |
| `locked` | session locked; never resolved |
| `asleep` | PC was asleep |
| `meeting` | camera/mic in use; never resolved |
| `busy` | fullscreen/presentation; never resolved |
| `replaced` | still pending or showing when the next break came due |

**Day events:**

- `{ "ts", "type": "day_end", "auto": bool }`, where `ts` is the real shutdown time.
- `{ "ts", "type": "day_resume" }`
- `{ "ts", "type": "sleep", "source": "phone"|"tray"|"auto" }`

**Stats view (later):**

- Per day/week: done vs skipped vs away.
- Reps per exercise, and completion rate (reps vs target).
- Shutdown time and bedtime per day.

## Configuration

`%APPDATA%\microbreaks\config.toml` is created with defaults on first run and reloaded when the file changes. The later settings window edits the same file via `toml_edit`, so comments are preserved.

```toml
[workday]
days               = ["mon", "tue", "wed", "thu", "fri"]
start              = "08:00"
end                = "17:00"   # breaks continue past this until you go idle
overtime_idle_mins = 30

[eye]
enabled        = true
minutes        = [20, 40]      # :00 is covered by the movement break
duration_secs  = 20
idle_skip_secs = 20

[move]
enabled        = true
idle_skip_secs = 120
grace_mins     = 15
mode           = "all"         # "all" | "rotate" (one exercise per break, cycling)

[[move.exercises]]
name = "squats"
reps = 15

# [[move.exercises]]
# name = "pushups"
# reps = 10

[offhours]
enabled       = false
weekend_start = "09:00"        # non-work days are Off-hours from here; work days switch at day_end
quiet_after   = "22:00"
eye           = false          # eye breaks in Off-hours when active at the PC

[heads_up]
enabled = false
secs    = 10

[overlay]
monitors       = "all"         # "all" | "active"
opacity        = 0.85
input_grace_ms = 1000

[suppress]
camera_mic      = true
fullscreen      = true
ignore_mic_apps = []           # exe names to ignore for mic detection

[ntfy]
enabled       = false
server        = "https://ntfy.sh"
alerts_topic  = ""             # long random string
control_topic = ""             # long random string
```

## Architecture

**Stack:** Tauri 2. The Rust backend holds all logic; the overlay, settings and stats UI are a thin TypeScript layer (Vite, plain TS or Svelte).

```
microbreaks/
  crates/core/        # pure Rust, no OS deps: config, schedule, phases, break state machine, event model
  src-tauri/          # Tauri app: tray, overlay windows, Win32 probes, ntfy client, autostart, log writer
  ui/                 # overlay + settings + stats pages (web)
```

- **Scheduler is a tick, not timers.**
  - Every ~2 s the app samples presence and calls `core::step(state, now, presence, inbox) -> (state, effects)`.
  - Effects are things like show overlay, push to phone, or write event.
  - This handles PC sleep, DST, clock changes and config reloads uniformly.
  - A gap between ticks larger than ~1 min means the PC was asleep, and the breaks due in that gap are logged `asleep`.
- **`core` stays platform-free and fully unit-tested**, including a simulated full day.
  - A future native phone app could reuse it via uniffi, or a PWA via wasm.
- **Platform traits keep the scheduler testable with fakes:**
  - `Presence`: `idle_secs()`, `is_locked()`, `in_meeting()`, `is_busy()`.
  - `Notifier`: the ntfy publish/subscribe.
- **Overlay windows:** one webview window per monitor, created at break time and destroyed afterwards.
  - If creation is noticeably slow, pre-create them hidden.

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

1. **M0 — Skeleton:** Tauri 2 project, tray icon with Quit, autostart, config load with defaults.
2. **M1 — Core loop (MVP).** *This is the version to start using daily.*
   - **First task:** a focus-stealing spike. Show a topmost window from the tray process while another app has focus, and confirm it receives `Esc`.
   - Tick scheduler, work hours, overtime-aware `day_end` (idle detection is needed here).
   - Eye + movement overlays on all monitors, rep steppers, input grace, Done / double-Esc skip.
   - Done for the day / Resume.
   - Event log.
3. **M2 — Smart suppression:** lock, asleep, camera/mic and fullscreen detection, plus the pending/grace mechanism.
4. **M3 — Phone via ntfy:**
   - Away buzzer with Done/Skip actions.
   - Off-hours phase (weekday evenings + weekends) and Going to sleep.
   - First, verify iOS `http` actions.
5. **M4 — Polish:**
   - Pause and "take a break now".
   - Today's stats.
   - Heads-up toast.
   - Settings window.
6. **M5 — Distribution:** code signing, MSIX packaging (autostart moves to a manifest `StartupTask`), Microsoft Store (if worth it).
7. **Future — native phone app:** reminders when the PC is off (weekends, evenings away from home), using the shared `core`.
   - iOS caps this at time-sensitive notifications.
   - iOS PWAs can't schedule local notifications without a push server.

## Open questions

- None blocking. Revisit weekend reliability (PC off) once M3 is in use.
