# Status

_Last updated: 2026-10-12_

## Current milestone

**M1** is hand-verified. **M2** is written on branch `m2-smart-suppression`: core logic has 35 passing tests, the app builds cleanly and stays running, but the **Windows probes are unverified by hand** (see Next up).

## Milestones

- [x] Spec (v0.5)
- [ ] M0 — Skeleton: Tauri 2 project, tray + Quit, autostart, config load. _Tray, status header and Quit verified. Autostart unverified: the config was created by a debug run, so the release build never saw `first_run` and didn't register the Run key._
- [x] M1 — Core loop (MVP): focus spike first, then scheduler, overlays, day end, event log. _Verified by hand 2026-10-12: focus-stealing works (acts modal), overlay on both monitors, Done gated by countdown, event log written. Movement break / Esc Esc not exercised yet, assumed same path._
- [ ] M2 — Smart suppression: lock / asleep / camera-mic / fullscreen, pending + grace. _Core + probes written; tick after the manual checks below._
- [ ] M3 — Phone via ntfy: away buzzer, Off-hours, Going to sleep. _Away buzzer verified on a real phone with the debug test item (2026-10-12): Done on a pending break, and Skip while the overlay was showing (it closed). The real idle-triggered push is assumed to work (not tested by choice); report if it doesn't. Off-hours phase, Going to sleep (tray, phone button, `quiet_after` backstop) written on branch `m3-offhours` with core tests; **not tried by hand**._
- [ ] M4 — Polish: pause, stats, heads-up toast, settings window. _Pause and "Take a break now" written on branch `m4-pause` (core tests pass); not tried by hand. Stats window (today view hand-checked, looks right) plus a last-7-days view written on branch `m4-week` (core tests pass; the week view is unverified, and the log only starts on 2026-10-12 so it is mostly empty rows). Heads-up toast and settings window not started._
- [ ] M5 — Distribution: signing, MSIX, Store

## Next up

1. Verify autostart: delete `%APPDATA%\microbreaks\config.toml`, run the release build once, check `HKCU\...\Run` has a `microbreaks` entry. Then tick M0.
2. Watch the first real movement break (on the hour) in the release build: reps steppers, Done, `exercises` in `events.jsonl`.
3. Verify M2 probes by hand (debug build, then tick M2):
   - **Lock:** Win+L during a movement slot, unlock within 15 min: break shows on return, event has `delayed_by: "locked"`.
   - **Meeting:** open Camera (or join a call); an eye break logs `meeting`, a movement break waits. Check whether any app needs adding to `ignore_mic_apps`.
   - **Busy:** play a fullscreen video/game at a break time; the break logs `busy` / waits. Confirm our own overlay does not make the probe report busy, and note which `QUNS_*` values fire. Focus assist and fullscreen Store apps are not detected (`QUNS_QUIET_TIME` / `QUNS_APP` left out on purpose); check whether that matters.
   - **Asleep:** sleep the PC across a slot; events show `asleep` at the slot times.
   - **Day end on lock/sleep:** after 17:00, lock the PC and check for an auto `day_end` at the last input time.
4. Block-reason toasts are written on branch `block-toasts` (core tests pass, app builds) but the **toast display is unverified**: add `[toasts]` with `enabled = true` to `%APPDATA%\microbreaks\config.toml`, restart the new build, and trigger a meeting/busy hold, then let a pending break expire. Check the toast shows and what app name it uses. The release build now running predates this.
5. M3 away buzzer is on branch `m3-ntfy`. To verify end to end: fill in `[ntfy]` in `%APPDATA%\microbreaks\config.toml` (`enabled = true`, your two topics), restart a fresh build (stop the running exe first, it locks the file), then sit idle past 2 min across the top of the hour. Expect a phone notification; tap Done and check `events.jsonl` for `"via":"phone"`. Also test Skip, and a tap after returning to the PC (overlay should close).
   Debug builds have a tray item "Test phone push (away)" that triggers the push without waiting for a real idle break. Since you are at the PC, the held break shows its overlay on the next tick (the block has cleared), so a phone Done then closes the overlay; to test the pending path itself, go idle for 2+ min instead.
8. Verify the stats window by hand (branches `m4-stats`, `m4-week`): tray "Stats" opens a small window with movement/eye done-of-due counts, skipped/missed/paused, reps vs target, work-day end and bedtime. Compare the numbers against today's lines in `events.jsonl`. Re-clicking should focus the same window, closing it must not quit the app, and it should refresh when it regains focus. Never run the debug and release builds at once: there is no single-instance guard yet.
7. Verify pause by hand (branch `m4-pause`): tray has "Take a break now" (a movement break), "Pause for 30 min" / "Pause for 1 h" and "End pause" (greyed out unless paused). While paused the status/tooltip reads "Paused until HH:MM", breaks at :20/:40/:00 log `"outcome":"paused"` and show nothing, and they resume by themselves. The old debug "Test movement break" item was replaced by the real "Take a break now".
6. Verify Off-hours by hand (branch `m3-offhours`, off by default): add `[offhours]` with `enabled = true` to `config.toml`, restart a fresh build, then check:
   - Tray has "Going to sleep"; clicking it after the day ends (or on a weekend after 09:00) logs a `sleep` event with `source: tray`, and no more reminders until morning. It does nothing during work hours.
   - After the work day ends, a movement break fires hourly (`phase: offhours`) until 22:00, then an auto `sleep` event.
   - An away/locked push in Off-hours is titled "Off-hours movement break" and has three buttons; tapping **Going to sleep** logs `source: phone`.
   - Status line reads "Off-hours · next: ..." and "Sleep · next: ...".

## Known issues / risks

- **Build environment (this machine):** build from `C:\work\microbreaks` (ThreatLocker blocked unsigned build-script exes elsewhere). Cargo picks the Enterprise VS 18 instance, which has no libs, so load the Build Tools env first: run `vcvars64.bat` from `C:\Program Files (x86)\Microsoft Visual Studio\18\BuildTools\VC\Auxiliary\Build` (or use its Developer PowerShell). Git Bash's coreutils `link` shadows MSVC's `link.exe`; use PowerShell.
- Windows foreground lock may stop the overlay from taking focus (see SPEC → The break overlay). `win32::force_foreground` tries `AttachThreadInput`, then a synthetic Alt press.
- ntfy: the `http` action button works on iOS (verified 2026-10-12). Tapping it makes the phone POST to the control topic, which the desktop watches. No fallback needed.
- Overlay, tray and Win32 probes can only be tested on Windows. Cloud/Linux sessions can only test `crates/core`.
- The Sleep phase and `day_end` live in memory only: restarting the app (e.g. a reboot at 21:30 after tapping sleep) brings reminders back.
- A mistaken tray "Going to sleep" after `day_end` has no undo (no "wake up" item yet).
- Remaining gaps: no config reload, no single-instance guard, one tray icon for all states (no pending badge yet), no phone push for away/locked pending breaks (M3).
- The camera/mic registry scan runs every 2 s tick. Fine for now; probe only when a break is due if it ever matters.
- A phone notification is not withdrawn when the break is resolved at the desktop (ntfy can replace or delete by sequence id; not done). A late tap is ignored.
- An expired pending movement break is logged with the reason that held it first, not the last one.
- The config file is only read at startup. An invalid file falls back to defaults (message on stderr only).

## Log

- 2026-10-12: Spec drafted through v0.5.
- 2026-10-12: M0 + M1 code written (core scheduler with 20 passing tests, Tauri tray/overlay/event log, plain-JS overlay UI). Builds and launches on Windows; manual verification pending.
- 2026-10-12: Eye `idle_skip_secs` default 20 → 120 (readers who don't touch input were losing breaks); spec, default config and tests updated, 21 core tests pass.
- 2026-10-12: Hand-verified tray, focus-stealing overlay, eye-break gating and event log; M1 ticked, M0 waits on autostart.
- 2026-10-12: M2 written: `Presence` snapshot, block priority, pending/grace for movement breaks, asleep via tick gap, `delayed_by`, lock/sleep day end, Win32 probes for lock, camera/mic and busy. 35 core tests pass, app builds. Probes await manual verification.
- 2026-10-12: Block-reason toasts: `Effect::Notify`, `[notify]` config (off by default), `tauri-plugin-notification`. 38 core tests pass. Display unverified.
- 2026-10-12: Verified on a real iPhone that an ntfy `http` action button POSTs to a second topic, which the PC saw. M3 design confirmed.
- 2026-10-12: M3 away buzzer: `Effect::Push`, phone Done/Skip inputs, clock-seeded break ids, ureq-based ntfy publish and control-topic listener. 47 core tests pass, app builds. Phone round trip unverified.
- 2026-10-12: Hand-verified the phone round trip: push arrived, Done from the iPhone resolved the pending break (`via: phone`, `delayed_by: away`, reps = targets). The camera was on, so the `meeting` block kept the break pending, which exercised the pending path.
- 2026-10-12: Hand-verified phone Skip closing a showing overlay (`via: phone`, `shown_ms` 6656).
- 2026-10-12: M3 Off-hours: phase model (Work / Off-hours / Sleep), auto and manual sleep, wake boundaries, phase on break events, sleep events, tray item, phone "Going to sleep" button. 62 core tests pass, app compiles (debug exe was running, so not relinked). Unverified by hand.
- 2026-10-12: PR #4 merged (M2 + M3). M4 started: pause (30 min / 1 h / end early, `paused` outcome, `pause` events) and a real "Take a break now" tray item. 69 core tests pass, app compiles (release exe was running, not relinked). Unverified by hand.
- 2026-10-12: M4 today's stats: `stats::summarize_day` in core (5 tests), `day_stats` command, `stats.html` window, tray item. 74 core tests pass, debug build compiles. The window is unverified by hand.
- 2026-10-12: Stats window: Today view hand-checked (numbers match the log). Added a Last 7 days tab: `stats::summarize_week` (3 tests, log parsed once, `has_data` so empty days read as "no data"), `week_stats` command, tray item renamed "Stats". 77 core tests pass, app compiles. Week view unverified.
