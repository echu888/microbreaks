# Status

_Last updated: 2026-10-12_

## Current milestone

**M0 — Skeleton** (not started)

## Milestones

- [x] Spec (v0.5)
- [ ] M0 — Skeleton: Tauri 2 project, tray + Quit, autostart, config load
- [ ] M1 — Core loop (MVP): focus spike first, then scheduler, overlays, day end, event log
- [ ] M2 — Smart suppression: lock / asleep / camera-mic / fullscreen, pending + grace
- [ ] M3 — Phone via ntfy: away buzzer, Off-hours, Going to sleep
- [ ] M4 — Polish: pause, stats, heads-up toast, settings window
- [ ] M5 — Distribution: signing, MSIX, Store

## Next up

- Scaffold the repo layout from SPEC.md (`crates/core`, `src-tauri`, `ui`).
- M1's first task: the focus-stealing spike (must be verified on Windows).

## Known issues / risks

- Windows foreground lock may stop the overlay from taking focus (see SPEC → The break overlay).
- Not yet verified whether the iOS ntfy app supports `http` action buttons (needed in M3).
- Overlay, tray and Win32 probes can only be tested on Windows. Cloud/Linux sessions can only test `crates/core`.

## Log

- 2026-10-12: Spec drafted through v0.5.
