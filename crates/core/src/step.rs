use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, Timelike};
use serde::Serialize;

use crate::config::{Config, Exercise, MoveMode};
use crate::event::{BlockReason, BreakKind, Event, ExerciseResult, Outcome, Phase, Via};

/// Longest gap between ticks that is scanned for due breaks.
const MAX_SCAN: Duration = Duration::hours(24);

/// A gap between ticks longer than this means the PC was asleep.
const SLEEP_GAP: Duration = Duration::seconds(60);

/// A snapshot of what the platform probes report at one tick.
#[derive(Debug, Clone, Copy, Default)]
pub struct Presence {
    pub idle_secs: u64,
    pub locked: bool,
    /// Camera or microphone in use.
    pub meeting: bool,
    /// Fullscreen app, presentation or Focus mode.
    pub busy: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Input {
    BreakDone { id: u64, reps: Vec<u32> },
    BreakSkipped { id: u64 },
    /// Done or Skip tapped on the phone; reps count as the targets.
    PhoneDone { id: u64 },
    PhoneSkip { id: u64 },
    DoneForDay,
    Resume,
    TakeBreak(BreakKind),
    /// Debug aid: start a movement break already held for `reason`, as if it came due while blocked.
    TakeHeldBreak(BlockReason),
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
    /// A toast explaining a held or missed break. Only emitted when `toasts.enabled`.
    Toast { title: String, body: String },
    /// A phone notification for a movement break held while nobody is at the PC. Only emitted
    /// when `ntfy.is_ready()`; the app owns topics and URLs.
    Push { id: u64, title: String, message: String },
    Log(Event),
}

#[derive(Debug)]
struct Active {
    spec: BreakSpec,
    /// When the overlay was shown; while pending, when the break came due.
    started: NaiveDateTime,
    /// Set while a movement break is held back (no overlay yet).
    pending: Option<BlockReason>,
    /// Why a shown break was held first.
    delayed_by: Option<BlockReason>,
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
    /// `first_id` seeds break ids. Phone replies carry them, so they must not repeat across
    /// restarts (the app seeds this from the clock).
    pub fn new(first_id: u64) -> Self {
        State { next_id: first_id, ..State::default() }
    }

    #[cfg(test)]
    pub(crate) fn active_id(&self) -> Option<u64> {
        self.active.as_ref().map(|a| a.spec.id)
    }

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
                    finish_break(state, now, Outcome::Done, Some(reps), Via::Desktop, &mut fx);
                }
            }
            Input::PhoneDone { id } => {
                if state.active.as_ref().is_some_and(|a| a.spec.id == *id) {
                    finish_break(state, now, Outcome::Done, Some(&[]), Via::Phone, &mut fx);
                }
            }
            Input::PhoneSkip { id } => {
                if state.active.as_ref().is_some_and(|a| a.spec.id == *id) {
                    finish_break(state, now, Outcome::Skipped, None, Via::Phone, &mut fx);
                }
            }
            Input::BreakSkipped { id } => {
                if state.active.as_ref().is_some_and(|a| a.spec.id == *id) {
                    finish_break(state, now, Outcome::Skipped, None, Via::Desktop, &mut fx);
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
            Input::TakeBreak(kind) => start_break(state, cfg, now, *kind, None, &mut fx),
            Input::TakeHeldBreak(reason) => hold_movement_break(state, cfg, now, *reason, &mut fx),
        }
    }

    let prev = state.last_tick.unwrap_or(now);
    let slept = state.last_tick.is_some() && now - prev > SLEEP_GAP;
    state.last_tick = Some(now);

    if slept {
        // Closing the lid after `end` is a shutdown; sleeping before `end` and waking after it is not.
        if is_workday(cfg, prev.date()) && state.ended_on != Some(prev.date()) && prev.time() >= cfg.workday.end {
            state.ended_on = Some(prev.date());
            fx.push(Effect::Log(Event::DayEnd { ts: prev, auto: true }));
        }
        // Logged at the slot time, not the wake time, so the stats show when the break was missed.
        for (t, kind) in due_slots(state, cfg, prev, now) {
            fx.push(Effect::Log(break_event(t, kind, Outcome::Asleep, 0, None, Vec::new())));
        }
    }

    // Overtime-aware automatic day end: a lock, or the first long idle period, after `end`.
    // Logged at the start of the idle period, i.e. the real shutdown time.
    if state.ended_on != Some(today)
        && is_workday(cfg, today)
        && now.time() >= cfg.workday.end
    {
        let idle_start = now - Duration::seconds(presence.idle_secs as i64);
        // A lock only counts if it started after `end`; a long lock is caught by the idle rule.
        let locked_after_end = presence.locked && idle_start.time() >= cfg.workday.end && idle_start.date() == today;
        if locked_after_end || presence.idle_secs >= cfg.workday.overtime_idle_mins * 60 {
            end_day(state, now, idle_start, true, &mut fx);
        }
    }

    resolve_pending(state, cfg, now, presence, &mut fx);

    // A gap was handled above. Otherwise only the latest slot fires, so a skipped tick yields at most one break.
    if !slept {
        if let Some((_, kind)) = due_slots(state, cfg, prev, now).pop() {
            match block_reason(cfg, presence, kind) {
                None => start_break(state, cfg, now, kind, None, &mut fx),
                Some(reason) if kind == BreakKind::Move => hold_movement_break(state, cfg, now, reason, &mut fx),
                Some(reason) => fx.push(Effect::Log(break_event(now, kind, reason.outcome(), 0, None, Vec::new()))),
            }
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

/// Every slot in `(prev, now]`, oldest first.
fn due_slots(state: &State, cfg: &Config, prev: NaiveDateTime, now: NaiveDateTime) -> Vec<(NaiveDateTime, BreakKind)> {
    let from = if now - prev > MAX_SCAN { now - MAX_SCAN } else { prev };
    let mut t = floor_minute(from) + Duration::minutes(1);
    let mut due = Vec::new();
    while t <= now {
        if let Some(kind) = slot_kind(cfg, state.ended_on, t) {
            due.push((t, kind));
        }
        t += Duration::minutes(1);
    }
    due
}

/// What holds a break back right now. Checked in priority order: someone on a call or
/// watching fullscreen video is usually idle too, and must not be treated as away.
fn block_reason(cfg: &Config, presence: Presence, kind: BreakKind) -> Option<BlockReason> {
    let idle_skip = match kind {
        BreakKind::Eye => cfg.eye.idle_skip_secs,
        BreakKind::Move => cfg.movement.idle_skip_secs,
    };
    if presence.locked {
        Some(BlockReason::Locked)
    } else if cfg.suppress.camera_mic && presence.meeting {
        Some(BlockReason::Meeting)
    } else if cfg.suppress.fullscreen && presence.busy {
        Some(BlockReason::Busy)
    } else if presence.idle_secs >= idle_skip {
        Some(BlockReason::Away)
    } else {
        None
    }
}

/// Shows a held movement break once its block clears, or gives up when the grace window ends.
fn resolve_pending(state: &mut State, cfg: &Config, now: NaiveDateTime, presence: Presence, fx: &mut Vec<Effect>) {
    let Some(active) = state.active.as_mut() else { return };
    let Some(reason) = active.pending else { return };
    if now - active.started > Duration::minutes(cfg.movement.grace_mins as i64) {
        state.active = None;
        fx.push(Effect::Log(break_event(now, BreakKind::Move, reason.outcome(), 0, None, Vec::new())));
        let body = format!("{} for the whole {} min grace window.", reason_phrase(reason), cfg.movement.grace_mins);
        toast(cfg, "Movement break missed", body, fx);
    } else if block_reason(cfg, presence, BreakKind::Move).is_none() {
        active.pending = None;
        active.delayed_by = Some(reason);
        active.started = now;
        fx.push(Effect::ShowOverlay(active.spec.clone()));
    }
}

fn reason_phrase(reason: BlockReason) -> &'static str {
    match reason {
        BlockReason::Away => "You were away",
        BlockReason::Locked => "The PC was locked",
        BlockReason::Asleep => "The PC was asleep",
        BlockReason::Meeting => "Camera or mic in use",
        BlockReason::Busy => "A fullscreen app or presentation was running",
    }
}

/// Starts a movement break held for `reason`, and tells the user where it matters.
fn hold_movement_break(state: &mut State, cfg: &Config, now: NaiveDateTime, reason: BlockReason, fx: &mut Vec<Effect>) {
    start_break(state, cfg, now, BreakKind::Move, Some(reason), fx);
    // Locked and away mean nobody is looking at the screen to read a toast; they get the phone instead.
    if matches!(reason, BlockReason::Meeting | BlockReason::Busy) {
        let body = format!("{}. Will show when you're free, up to {} min.", reason_phrase(reason), cfg.movement.grace_mins);
        toast(cfg, "Movement break held", body, fx);
    } else if cfg.ntfy.is_ready() {
        push_to_phone(state, reason, fx);
    }
}

fn push_to_phone(state: &State, reason: BlockReason, fx: &mut Vec<Effect>) {
    let Some(active) = &state.active else { return };
    let what: Vec<String> = active.spec.exercises.iter().map(|e| format!("{} x{}", e.name, e.reps)).collect();
    fx.push(Effect::Push {
        id: active.spec.id,
        title: "Movement break".into(),
        message: format!("{}. {}.", what.join(", "), reason_phrase(reason)),
    });
}

/// Body of the phone's Done/Skip action, e.g. `done:1760000000123`.
pub fn control_body(done: bool, id: u64) -> String {
    format!("{}:{id}", if done { "done" } else { "skip" })
}

/// Parses a message from the control topic. Anything unrecognised is ignored.
pub fn parse_control(body: &str) -> Option<Input> {
    let (verb, id) = body.trim().split_once(':')?;
    let id = id.parse().ok()?;
    match verb {
        "done" => Some(Input::PhoneDone { id }),
        "skip" => Some(Input::PhoneSkip { id }),
        _ => None,
    }
}

fn toast(cfg: &Config, title: &str, body: String, fx: &mut Vec<Effect>) {
    if cfg.toasts.enabled {
        fx.push(Effect::Toast { title: title.into(), body });
    }
}

/// Starts a break. With `hold` set it stays pending (no overlay) until the block clears.
fn start_break(
    state: &mut State,
    cfg: &Config,
    now: NaiveDateTime,
    kind: BreakKind,
    hold: Option<BlockReason>,
    fx: &mut Vec<Effect>,
) {
    // A new overlay replaces the old one, but a held break shows nothing, so close it explicitly.
    if let Some(old) = state.active.take() {
        let shown_ms = if old.pending.is_some() { 0 } else { (now - old.started).num_milliseconds() };
        fx.push(Effect::Log(break_event(now, old.spec.kind, Outcome::Replaced, shown_ms, None, Vec::new())));
        if old.pending.is_none() && hold.is_some() {
            fx.push(Effect::CloseOverlay);
        }
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
    if hold.is_none() {
        fx.push(Effect::ShowOverlay(spec.clone()));
    }
    state.active = Some(Active { spec, started: now, pending: hold, delayed_by: None });
}

/// Logs the active break with `outcome` and closes its overlay.
fn finish_break(
    state: &mut State,
    now: NaiveDateTime,
    outcome: Outcome,
    reps: Option<&[u32]>,
    via: Via,
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
    let kind = active.spec.kind;
    if let Some(reason) = active.pending {
        // Never shown, so there is no overlay to close. A phone reply resolves it; anything else
        // (the day ending) leaves it unresolved, logged with the reason that held it.
        let ev = if via == Via::Phone {
            break_event(now, kind, outcome, 0, Some(reason), results)
        } else {
            break_event(now, kind, reason.outcome(), 0, None, Vec::new())
        };
        fx.push(Effect::Log(with_via(ev, via)));
        return;
    }
    let shown_ms = (now - active.started).num_milliseconds();
    let ev = break_event(now, kind, outcome, shown_ms, active.delayed_by, results);
    fx.push(Effect::Log(with_via(ev, via)));
    fx.push(Effect::CloseOverlay);
}

fn end_day(state: &mut State, now: NaiveDateTime, ts: NaiveDateTime, auto: bool, fx: &mut Vec<Effect>) {
    state.ended_on = Some(now.date());
    // Ending the day while a break is showing counts as dismissing it.
    finish_break(state, now, Outcome::Skipped, None, Via::Desktop, fx);
    fx.push(Effect::Log(Event::DayEnd { ts, auto }));
}

fn with_via(mut ev: Event, new: Via) -> Event {
    if let Event::Break { via, .. } = &mut ev {
        *via = new;
    }
    ev
}

fn break_event(
    ts: NaiveDateTime,
    kind: BreakKind,
    outcome: Outcome,
    shown_ms: i64,
    delayed_by: Option<BlockReason>,
    exercises: Vec<ExerciseResult>,
) -> Event {
    Event::Break { ts, kind, phase: Phase::Work, outcome, delayed_by, via: Via::Desktop, shown_ms, exercises }
}
