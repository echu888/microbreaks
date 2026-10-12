use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, Timelike};
use serde::Serialize;

use crate::config::{Config, Exercise, MoveMode};
use crate::event::{BreakKind, Event, ExerciseResult, Outcome, Phase, Via};

/// Longest gap between ticks that is scanned for due breaks.
const MAX_SCAN: Duration = Duration::hours(24);

#[derive(Debug, Clone, Copy)]
pub struct Presence {
    pub idle_secs: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Input {
    BreakDone { id: u64, reps: Vec<u32> },
    BreakSkipped { id: u64 },
    DoneForDay,
    Resume,
    TakeBreak(BreakKind),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BreakSpec {
    pub id: u64,
    pub kind: BreakKind,
    /// Countdown before Done is enabled (eye breaks only; 0 for movement).
    pub duration_secs: u32,
    pub exercises: Vec<Exercise>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    /// Replaces any overlay already showing.
    ShowOverlay(BreakSpec),
    CloseOverlay,
    Log(Event),
}

#[derive(Debug)]
struct Active {
    spec: BreakSpec,
    started: NaiveDateTime,
}

#[derive(Debug, Default)]
pub struct State {
    last_tick: Option<NaiveDateTime>,
    /// The date whose work day has ended (manually or automatically).
    ended_on: Option<NaiveDate>,
    active: Option<Active>,
    next_id: u64,
    rotate: usize,
}

impl State {
    /// Whether the work day for `today` has ended, manually or automatically.
    pub fn day_ended(&self, today: NaiveDate) -> bool {
        self.ended_on == Some(today)
    }
}

/// Advance the scheduler to `now` (local wall-clock time).
pub fn step(
    state: &mut State,
    cfg: &Config,
    now: NaiveDateTime,
    presence: Presence,
    inputs: &[Input],
) -> Vec<Effect> {
    let mut fx = Vec::new();
    let today = now.date();

    for input in inputs {
        match input {
            Input::BreakDone { id, reps } => {
                if state.active.as_ref().is_some_and(|a| a.spec.id == *id) {
                    finish_break(state, now, Outcome::Done, Some(reps), &mut fx);
                }
            }
            Input::BreakSkipped { id } => {
                if state.active.as_ref().is_some_and(|a| a.spec.id == *id) {
                    finish_break(state, now, Outcome::Skipped, None, &mut fx);
                }
            }
            Input::DoneForDay => {
                if state.ended_on != Some(today) {
                    end_day(state, now, now, false, &mut fx);
                }
            }
            Input::Resume => {
                if state.ended_on == Some(today) {
                    state.ended_on = None;
                    fx.push(Effect::Log(Event::DayResume { ts: now }));
                }
            }
            Input::TakeBreak(kind) => start_break(state, cfg, now, *kind, &mut fx),
        }
    }

    // Overtime-aware automatic day end: the first long idle period after `end`.
    // Logged at the start of the idle period, i.e. the real shutdown time.
    if state.ended_on != Some(today)
        && is_workday(cfg, today)
        && now.time() >= cfg.workday.end
        && presence.idle_secs >= cfg.workday.overtime_idle_mins * 60
    {
        let idle_start = now - Duration::seconds(presence.idle_secs as i64);
        end_day(state, now, idle_start, true, &mut fx);
    }

    let prev = state.last_tick.unwrap_or(now);
    state.last_tick = Some(now);
    // Only the latest slot in a gap fires, so a long gap yields at most one break.
    if let Some(kind) = latest_due(state, cfg, prev, now) {
        if kind == BreakKind::Eye && presence.idle_secs >= cfg.eye.idle_skip_secs {
            fx.push(Effect::Log(break_event(now, kind, Outcome::Away, 0, Vec::new())));
        } else {
            start_break(state, cfg, now, kind, &mut fx);
        }
    }

    fx
}

/// The next scheduled break strictly after `now`, if any within the next week.
pub fn next_break(state: &State, cfg: &Config, now: NaiveDateTime) -> Option<(BreakKind, NaiveDateTime)> {
    let mut t = floor_minute(now) + Duration::minutes(1);
    for _ in 0..8 * 24 * 60 {
        if let Some(kind) = slot_kind(cfg, state.ended_on, t) {
            return Some((kind, t));
        }
        t += Duration::minutes(1);
    }
    None
}

/// e.g. "Work · next: eye break 10:20" (shown in the tray).
pub fn status_line(state: &State, cfg: &Config, now: NaiveDateTime) -> String {
    let today = now.date();
    let phase = if state.ended_on == Some(today) {
        "Done for the day"
    } else if is_workday(cfg, today) {
        "Work"
    } else {
        "Off"
    };
    match next_break(state, cfg, now) {
        Some((kind, t)) => {
            let what = match kind {
                BreakKind::Eye => "eye break",
                BreakKind::Move => "movement break",
            };
            let when = if t.date() == today { t.format("%H:%M") } else { t.format("%a %H:%M") };
            format!("{phase} · next: {what} {when}")
        }
        None => phase.to_string(),
    }
}

fn is_workday(cfg: &Config, date: NaiveDate) -> bool {
    cfg.workday.days.contains(&date.weekday())
}

fn floor_minute(t: NaiveDateTime) -> NaiveDateTime {
    t.with_second(0).and_then(|t| t.with_nanosecond(0)).unwrap_or(t)
}

/// Which break, if any, is scheduled at the minute boundary `t`.
fn slot_kind(cfg: &Config, ended_on: Option<NaiveDate>, t: NaiveDateTime) -> Option<BreakKind> {
    let date = t.date();
    if !is_workday(cfg, date) || ended_on == Some(date) || t.time() <= cfg.workday.start {
        return None;
    }
    let minute = t.minute();
    if minute == 0 && cfg.movement.enabled {
        Some(BreakKind::Move)
    } else if cfg.eye.enabled && cfg.eye.minutes.contains(&minute) {
        Some(BreakKind::Eye)
    } else {
        None
    }
}

/// The latest slot in `(prev, now]`.
fn latest_due(state: &State, cfg: &Config, prev: NaiveDateTime, now: NaiveDateTime) -> Option<BreakKind> {
    let from = if now - prev > MAX_SCAN { now - MAX_SCAN } else { prev };
    let mut t = floor_minute(from) + Duration::minutes(1);
    let mut due = None;
    while t <= now {
        due = slot_kind(cfg, state.ended_on, t).or(due);
        t += Duration::minutes(1);
    }
    due
}

fn start_break(state: &mut State, cfg: &Config, now: NaiveDateTime, kind: BreakKind, fx: &mut Vec<Effect>) {
    // The new overlay replaces the old one, so only the log entry is needed.
    if let Some(old) = state.active.take() {
        let shown_ms = (now - old.started).num_milliseconds();
        fx.push(Effect::Log(break_event(now, old.spec.kind, Outcome::Replaced, shown_ms, Vec::new())));
    }
    let exercises = match (kind, cfg.movement.mode) {
        (BreakKind::Eye, _) => Vec::new(),
        (BreakKind::Move, MoveMode::All) => cfg.movement.exercises.clone(),
        (BreakKind::Move, MoveMode::Rotate) => {
            let list = &cfg.movement.exercises;
            let pick = list.get(state.rotate % list.len().max(1)).cloned();
            state.rotate = state.rotate.wrapping_add(1);
            pick.into_iter().collect()
        }
    };
    state.next_id += 1;
    let spec = BreakSpec {
        id: state.next_id,
        kind,
        duration_secs: if kind == BreakKind::Eye { cfg.eye.duration_secs } else { 0 },
        exercises,
    };
    fx.push(Effect::ShowOverlay(spec.clone()));
    state.active = Some(Active { spec, started: now });
}

/// Logs the active break with `outcome` and closes its overlay.
fn finish_break(
    state: &mut State,
    now: NaiveDateTime,
    outcome: Outcome,
    reps: Option<&[u32]>,
    fx: &mut Vec<Effect>,
) {
    let Some(active) = state.active.take() else { return };
    let results = match reps {
        Some(reps) if active.spec.kind == BreakKind::Move => active
            .spec
            .exercises
            .iter()
            .enumerate()
            .map(|(i, e)| ExerciseResult {
                name: e.name.clone(),
                target: e.reps,
                reps: reps.get(i).copied().unwrap_or(e.reps),
            })
            .collect(),
        _ => Vec::new(),
    };
    let shown_ms = (now - active.started).num_milliseconds();
    fx.push(Effect::Log(break_event(now, active.spec.kind, outcome, shown_ms, results)));
    fx.push(Effect::CloseOverlay);
}

fn end_day(state: &mut State, now: NaiveDateTime, ts: NaiveDateTime, auto: bool, fx: &mut Vec<Effect>) {
    state.ended_on = Some(now.date());
    // Ending the day while a break is showing counts as dismissing it.
    finish_break(state, now, Outcome::Skipped, None, fx);
    fx.push(Effect::Log(Event::DayEnd { ts, auto }));
}

fn break_event(
    ts: NaiveDateTime,
    kind: BreakKind,
    outcome: Outcome,
    shown_ms: i64,
    exercises: Vec<ExerciseResult>,
) -> Event {
    Event::Break { ts, kind, phase: Phase::Work, outcome, via: Via::Desktop, shown_ms, exercises }
}
