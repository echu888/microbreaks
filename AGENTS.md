# Notes for coding agents

- Read `SPEC.md` (what/how) and `STATUS.md` (progress) before starting work.
- Keep `STATUS.md` current. At the end of a session, tick milestones, update "Next up" and "Known issues", and add a one-line log entry.
- If implementation changes a decision, update `SPEC.md` in the same PR. Don't let it drift.
- Keep scheduling/state logic in `crates/core` (platform-free, unit-tested). Windows-specific code goes in `src-tauri`.
- Cloud sessions run on Linux: `cargo test -p core` works there, but Windows behaviour (overlay focus, tray, Win32 probes) must be flagged for manual verification.
