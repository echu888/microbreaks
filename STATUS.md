# Status

_Last updated: 2026-10-12_

## Current milestone

**M0 + M1** are written and **build cleanly** (`cargo build -p microbreaks`, no warnings; 20 core tests pass). The debug exe launches, stays running and writes the first-run `config.toml`. The tray, overlay, focus-stealing and event log are **not yet verified by hand**.

## Milestones

- [x] Spec (v0.5)
- [ ] M0 — Skeleton: Tauri 2 project, tray + Quit, autostart, config load. _Builds and launches; tray/autostart unverified._
- [ ] M1 — Core loop (MVP): focus spike first, then scheduler, overlays, day end, event log. _Builds; overlay and focus-stealing unverified._
- [ ] M2 — Smart suppression: lock / asleep / camera-mic / fullscreen, pending + grace
- [ ] M3 — Phone via ntfy: away buzzer, Off-hours, Going to sleep
- [ ] M4 — Polish: pause, stats, heads-up toast, settings window
- [ ] M5 — Distribution: signing, MSIX, Store

## Next up

1. `cargo install tauri-cli`, or just run `target\debug\microbreaks.exe`.
2. Verify by hand:
   - tray icon, status header, Quit;
   - tray "Test eye break" / "Test movement break" (debug builds only) with another app focused: the overlay must get keyboard focus and `Esc` `Esc` / `Enter` must work (the focus-stealing spike);
   - overlay on every monitor, with the panel on the cursor's monitor, at mixed DPI;
   - first-run `%APPDATA%\microbreaks\config.toml` and `events.jsonl`.
3. Release build: check autostart registers (first run only).
4. Tick M0 and M1 only after that.

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
