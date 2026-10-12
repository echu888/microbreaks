# Status

_Last updated: 2026-10-12_

## Current milestone

**M0 + M1** are written and **build cleanly** (`cargo build -p microbreaks`, no warnings; 20 core tests pass). The debug exe launches, stays running and writes the first-run `config.toml`. The tray, overlay, focus-stealing and event log are **not yet verified by hand**.

## Milestones

- [x] Spec (v0.5)
- [ ] M0 — Skeleton: Tauri 2 project, tray + Quit, autostart, config load. _Tray, status header and Quit verified. Autostart unverified: the config was created by a debug run, so the release build never saw `first_run` and didn't register the Run key._
- [x] M1 — Core loop (MVP): focus spike first, then scheduler, overlays, day end, event log. _Verified by hand 2026-10-12: focus-stealing works (acts modal), overlay on both monitors, Done gated by countdown, event log written. Movement break / Esc Esc not exercised yet, assumed same path._
- [ ] M2 — Smart suppression: lock / asleep / camera-mic / fullscreen, pending + grace
- [ ] M3 — Phone via ntfy: away buzzer, Off-hours, Going to sleep
- [ ] M4 — Polish: pause, stats, heads-up toast, settings window
- [ ] M5 — Distribution: signing, MSIX, Store

## Next up

1. Verify autostart: delete `%APPDATA%\microbreaks\config.toml`, run the release build once, check `HKCU\...\Run` has a `microbreaks` entry. Then tick M0.
2. Watch the first real movement break (on the hour) in the release build: reps steppers, Done, `exercises` in `events.jsonl`.
3. Start M2 (smart suppression), core logic first in `crates/core`.

## Known issues / risks

- **Build environment (this machine):** build from `C:\work\microbreaks` (ThreatLocker blocked unsigned build-script exes elsewhere). Cargo picks the Enterprise VS 18 instance, which has no libs, so load the Build Tools env first: run `vcvars64.bat` from `C:\Program Files (x86)\Microsoft Visual Studio\18\BuildTools\VC\Auxiliary\Build` (or use its Developer PowerShell). Git Bash's coreutils `link` shadows MSVC's `link.exe`; use PowerShell.
- Windows foreground lock may stop the overlay from taking focus (see SPEC → The break overlay). `win32::force_foreground` tries `AttachThreadInput`, then a synthetic Alt press.
- Not yet verified whether the iOS ntfy app supports `http` action buttons (needed in M3).
- Overlay, tray and Win32 probes can only be tested on Windows. Cloud/Linux sessions can only test `crates/core`.
- M1 scope gaps (all M2+): no lock/sleep/meeting/busy detection, no pending/grace for movement breaks (they show even when away), a PC that sleeps through the end of the day never logs `day_end`, no config reload, no single-instance guard, one tray icon for all states.
- The config file is only read at startup. An invalid file falls back to defaults (message on stderr only).

## Log

- 2026-10-12: Spec drafted through v0.5.
- 2026-10-12: M0 + M1 code written (core scheduler with 20 passing tests, Tauri tray/overlay/event log, plain-JS overlay UI). Builds and launches on Windows; manual verification pending.
- 2026-10-12: Eye `idle_skip_secs` default 20 → 120 (readers who don't touch input were losing breaks); spec, default config and tests updated, 21 core tests pass.
- 2026-10-12: Hand-verified tray, focus-stealing overlay, eye-break gating and event log; M1 ticked, M0 waits on autostart.
