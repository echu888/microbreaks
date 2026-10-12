use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, Timelike};
use serde::Serialize;

use crate::config::{Config, Exercise, MoveMode};
use crate::event::{BlockReason, BreakKind, Event, ExerciseResult, Outcome, Phase, SleepSource, Via};

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
    /// Pause all breaks for `mins` minutes (replaces a pause already running).
    Pause { mins: u32 },
    EndPause,
    /// "Going to sleep" from the tray. Ignored during Work and while already asleep.
    GoingToSleep,
    /// "Going to sleep" tapped on the phone. Same gate.
    PhoneSleep,
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
    /// `sleep_button` adds "Going to sleep" (Off-hours only).
    Push { id: u64, title: String, message: String, sleep_button: bool },
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
    /// Phase when it started; it keeps logging as that even if the phase changes.
    phase: Phase,
}

#[derive(Debug, Default)]
pub struct State {
    last_tick: Option<NaiveDateTime>,
    /// The date whose work day has ended (manually or automatically).
    ended_on: Option<NaiveDate>,
    /// Breaks are dropped (logged `paused`) until this time.
    paused_until: Option<NaiveDateTime>,
    /// The `day_end` timestamp for `ended_on` (the real shutdown time).
    ended_ts: Option<NaiveDateTime>,
    /// The user's Sleep phase lasts until this wake time. Not the PC sleeping.
    sleep_until: Option<NaiveDateTime>,
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

    /// Whether a pause is running at `now`.
    pub fn is_paused(&self, now: NaiveDateTime) -> bool {
        self.paused_until.is_some_and(|until| now < until)
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
                    state.ended_ts = None;
                    fx.push(Effect::Log(Event::DayResume { ts: now }));
                }
            }
            Input::Pause { mins } if *mins > 0 => {
                let until = now + Duration::minutes(i64::from(*mins));
                state.paused_until = Some(until);
                // Like ending the day: a showing break counts as dismissed.
                finish_break(state, now, Outcome::Skipped, None, Via::Desktop, &mut fx);
                fx.push(Effect::Log(Event::Pause { ts: now, until }));
            }
            Input::Pause { .. } => {}
            Input::EndPause => {
                if state.is_paused(now) {
                    state.paused_until = None;
                    fx.push(Effect::Log(Event::PauseEnd { ts: now }));
                }
            }
            Input::GoingToSleep => try_sleep(state, cfg, now, SleepSource::Tray, &mut fx),
            Input::PhoneSleep => try_sleep(state, cfg, now, SleepSource::Phone, &mut fx),
            Input::TakeBreak(kind) => {
                let phase = phase_now(cfg, state, now);
                start_break(state, cfg, now, *kind, None, phase, &mut fx);
            }
            Input::TakeHeldBreak(reason) => {
                let phase = phase_now(cfg, state, now);
                hold_movement_break(state, cfg, now, *reason, phase, &mut fx);
            }
        }
    }

    let prev = state.last_tick.unwrap_or(now);
    let slept = state.last_tick.is_some() && now - prev > SLEEP_GAP;
    state.last_tick = Some(now);

    if slept {
        // Closing the lid after `end` is a shutdown; sleeping before `end` and waking after it is not.
        if is_workday(cfg, prev.date()) && state.ended_on != Some(prev.date()) && prev.time() >= cfg.workday.end {
            state.ended_on = Some(prev.date());
            state.ended_ts = Some(prev);
            fx.push(Effect::Log(Event::DayEnd { ts: prev, auto: true }));
        }
        // Logged at the slot time, not the wake time, so the stats show when the break was missed.
        for (t, kind, phase) in due_slots(state, cfg, prev, now) {
            fx.push(Effect::Log(break_event(t, phase, kind, Outcome::Asleep, 0, None, Vec::new())));
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

    auto_sleep(state, cfg, now, &mut fx);

    resolve_pending(state, cfg, now, presence, &mut fx);

    // A gap was handled above. Otherwise only the latest slot fires, so a skipped tick yields at most one break.
    if !slept {
        if let Some((_, kind, phase)) = due_slots(state, cfg, prev, now).pop() {
            // Paused: nothing is shown, held or pushed. Logged so the stats show why.
            if state.is_paused(now) {
                fx.push(Effect::Log(break_event(now, phase, kind, Outcome::Paused, 0, None, Vec::new())));
                return fx;
            }
            match block_reason(cfg, presence, kind) {
                None => start_break(state, cfg, now, kind, None, phase, &mut fx),
                Some(reason) if kind == BreakKind::Move => {
                    hold_movement_break(state, cfg, now, reason, phase, &mut fx)
                }
                Some(reason) => {
                    fx.push(Effect::Log(break_event(now, phase, kind, reason.outcome(), 0, None, Vec::new())))
                }
            }
        }
    }

    fx
}

/// The next scheduled break strictly after `now`, if any within the next week.
pub fn next_break(state: &State, cfg: &Config, now: NaiveDateTime) -> Option<(BreakKind, NaiveDateTime)> {
    // A running pause drops everything until it ends, so the next real break comes after it.
    let from = state.paused_until.filter(|u| *u > now).unwrap_or(now);
    let mut t = floor_minute(from) + Duration::minutes(1);
    for _ in 0..8 * 24 * 60 {
        if let Some((kind, _)) = slot_kind(cfg, state, t) {
            return Some((kind, t));
        }
        t += Duration::minutes(1);
    }
    None
}

/// e.g. "Work · next: eye break 10:20" (shown in the tray).
pub fn status_line(state: &State, cfg: &Config, now: NaiveDateTime) -> String {
    let today = now.date();
    let phase = if let Some(until) = state.paused_until.filter(|u| *u > now) {
        format!("Paused until {}", until.format("%H:%M"))
    } else if is_sleeping(state, now) {
        "Sleep".to_string()
    } else if in_offhours(cfg, state, now) {
        "Off-hours".to_string()
    } else if state.ended_on == Some(today) {
        "Done for the day".to_string()
    } else if is_workday(cfg, today) {
        "Work".to_string()
    } else {
        "Off".to_string()
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
        None => phase,
    }
}

fn is_workday(cfg: &Config, date: NaiveDate) -> bool {
    cfg.workday.days.contains(&date.weekday())
}

fn floor_minute(t: NaiveDateTime) -> NaiveDateTime {
    t.with_second(0).and_then(|t| t.with_nanosecond(0)).unwrap_or(t)
}

/// Whether the user's Sleep phase covers `t`.
fn is_sleeping(state: &State, t: NaiveDateTime) -> bool {
    state.sleep_until.is_some_and(|wake| t < wake)
}

/// Off-hours: movement hourly after the work day ends, or on a non-work day after `weekend_start`,
/// until `quiet_after` or sleep.
fn in_offhours(cfg: &Config, state: &State, t: NaiveDateTime) -> bool {
    cfg.offhours.enabled && !is_sleeping(state, t) && t.time() < cfg.offhours.quiet_after && offhours_day(cfg, state, t)
}

/// The day-level part of Off-hours, ignoring `quiet_after` and sleep.
fn offhours_day(cfg: &Config, state: &State, t: NaiveDateTime) -> bool {
    let date = t.date();
    if is_workday(cfg, date) {
        state.ended_on == Some(date)
    } else {
        t.time() > cfg.offhours.weekend_start
    }
}

/// Work: a work day that hasn't ended, after `workday.start`.
fn in_work(cfg: &Config, state: &State, t: NaiveDateTime) -> bool {
    is_workday(cfg, t.date()) && state.ended_on != Some(t.date()) && t.time() > cfg.workday.start
}

/// Phase for a break started outside the schedule (manual or debug): anything but Work is Off-hours.
fn phase_now(cfg: &Config, state: &State, now: NaiveDateTime) -> Phase {
    if in_work(cfg, state, now) {
        Phase::Work
    } else {
        Phase::Offhours
    }
}

/// Which break, if any, is scheduled at the minute boundary `t`, and in which phase.
fn slot_kind(cfg: &Config, state: &State, t: NaiveDateTime) -> Option<(BreakKind, Phase)> {
    if is_sleeping(state, t) {
        return None;
    }
    let minute = t.minute();
    if in_work(cfg, state, t) {
        if minute == 0 && cfg.movement.enabled {
            Some((BreakKind::Move, Phase::Work))
        } else if cfg.eye.enabled && cfg.eye.minutes.contains(&minute) {
            Some((BreakKind::Eye, Phase::Work))
        } else {
            None
        }
    } else if in_offhours(cfg, state, t) {
        if minute == 0 && cfg.movement.enabled {
            Some((BreakKind::Move, Phase::Offhours))
        } else if cfg.offhours.eye && cfg.eye.enabled && cfg.eye.minutes.contains(&minute) {
            Some((BreakKind::Eye, Phase::Offhours))
        } else {
            None
        }
    } else {
        None
    }
}

/// Every slot in `(prev, now]`, oldest first.
fn due_slots(
    state: &State,
    cfg: &Config,
    prev: NaiveDateTime,
    now: NaiveDateTime,
) -> Vec<(NaiveDateTime, BreakKind, Phase)> {
    let from = if now - prev > MAX_SCAN { now - MAX_SCAN } else { prev };
    let mut t = floor_minute(from) + Duration::minutes(1);
    let mut due = Vec::new();
    while t <= now {
        if let Some((kind, phase)) = slot_kind(cfg, state, t) {
            due.push((t, kind, phase));
        }
        t += Duration::minutes(1);
    }
    due
}

/// The first morning after `t`: `workday.start` on a work day, `weekend_start` otherwise.
pub(crate) fn wake_after(cfg: &Config, t: NaiveDateTime) -> NaiveDateTime {
    let mut date = t.date();
    loop {
        let boundary = if is_workday(cfg, date) { cfg.workday.start } else { cfg.offhours.weekend_start };
        let wake = date.and_time(boundary);
        if wake > t {
            return wake;
        }
        date += Duration::days(1);
    }
}

/// Starts the Sleep phase. `ts` is the logged bedtime; `now` is when it was noticed.
fn go_to_sleep(
    state: &mut State,
    cfg: &Config,
    now: NaiveDateTime,
    ts: NaiveDateTime,
    source: SleepSource,
    fx: &mut Vec<Effect>,
) {
    state.sleep_until = Some(wake_after(cfg, ts));
    // Like ending the day: a showing break counts as dismissed, a pending one stays unresolved.
    finish_break(state, now, Outcome::Skipped, None, Via::Desktop, fx);
    fx.push(Effect::Log(Event::Sleep { ts, source }));
}

/// A manual sleep request. Ignored during Work (a misclick would silence the rest of the day,
/// and nothing undoes it) and while already asleep.
fn try_sleep(state: &mut State, cfg: &Config, now: NaiveDateTime, source: SleepSource, fx: &mut Vec<Effect>) {
    if in_work(cfg, state, now) || is_sleeping(state, now) {
        return;
    }
    go_to_sleep(state, cfg, now, now, source, fx);
}

/// The `quiet_after` backstop. Bedtime is `quiet_after`, but never before the day's shutdown.
fn auto_sleep(state: &mut State, cfg: &Config, now: NaiveDateTime, fx: &mut Vec<Effect>) {
    if !cfg.offhours.enabled
        || is_sleeping(state, now)
        || now.time() < cfg.offhours.quiet_after
        || !offhours_day(cfg, state, now)
    {
        return;
    }
    let mut ts = now.date().and_time(cfg.offhours.quiet_after);
    if let Some(ended) = state.ended_ts.filter(|e| e.date() == now.date()) {
        ts = ts.max(ended);
    }
    go_to_sleep(state, cfg, now, ts, SleepSource::Auto, fx);
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
        let phase = active.phase;
        state.active = None;
        fx.push(Effect::Log(break_event(now, phase, BreakKind::Move, reason.outcome(), 0, None, Vec::new())));
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
fn hold_movement_break(
    state: &mut State,
    cfg: &Config,
    now: NaiveDateTime,
    reason: BlockReason,
    phase: Phase,
    fx: &mut Vec<Effect>,
) {
    start_break(state, cfg, now, BreakKind::Move, Some(reason), phase, fx);
    // Locked and away mean nobody is looking at the screen to read a toast; they get the phone instead.
    if matches!(reason, BlockReason::Meeting | BlockReason::Busy) {
        let body = format!("{}. Will show when you're free, up to {} min.", reason_phrase(reason), cfg.movement.grace_mins);
        toast(cfg, "Movement break held", body, fx);
    } else if cfg.ntfy.is_ready() {
        push_to_phone(state, cfg, reason, fx);
    }
}

fn push_to_phone(state: &State, cfg: &Config, reason: BlockReason, fx: &mut Vec<Effect>) {
    let Some(active) = &state.active else { return };
    let what: Vec<String> = active.spec.exercises.iter().map(|e| format!("{} x{}", e.name, e.reps)).collect();
    // Off-hours pushes say so, and can end the evening with "Going to sleep".
    let offhours = cfg.offhours.enabled && active.phase == Phase::Offhours;
    fx.push(Effect::Push {
        id: active.spec.id,
        title: if offhours { "Off-hours movement break" } else { "Movement break" }.into(),
        message: format!("{}. {}.", what.join(", "), reason_phrase(reason)),
        sleep_button: offhours,
    });
}

/// A button on the phone notification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reply {
    Done,
    Skip,
    Sleep,
}

/// Body of a phone action, e.g. `done:1760000000123`. The id is the notification's break.
pub fn control_body(reply: Reply, id: u64) -> String {
    let verb = match reply {
        Reply::Done => "done",
        Reply::Skip => "skip",
        Reply::Sleep => "sleep",
    };
    format!("{verb}:{id}")
}

/// Parses a message from the control topic. Anything unrecognised is ignored.
pub fn parse_control(body: &str) -> Option<Input> {
    let (verb, id) = body.trim().split_once(':')?;
    let id = id.parse().ok()?;
    match verb {
        "done" => Some(Input::PhoneDone { id }),
        "skip" => Some(Input::PhoneSkip { id }),
        // Not tied to the break, so a tap after Done on the desktop still works.
        "sleep" => Some(Input::PhoneSleep),
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
    phase: Phase,
    fx: &mut Vec<Effect>,
) {
    // A new overlay replaces the old one, but a held break shows nothing, so close it explicitly.
    if let Some(old) = state.active.take() {
        let shown_ms = if old.pending.is_some() { 0 } else { (now - old.started).num_milliseconds() };
        let ev = break_event(now, old.phase, old.spec.kind, Outcome::Replaced, shown_ms, None, Vec::new());
        fx.push(Effect::Log(ev));
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
    state.active = Some(Active { spec, started: now, pending: hold, delayed_by: None, phase });
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
            break_event(now, active.phase, kind, outcome, 0, Some(reason), results)
        } else {
            break_event(now, active.phase, kind, reason.outcome(), 0, None, Vec::new())
        };
        fx.push(Effect::Log(with_via(ev, via)));
        return;
    }
    let shown_ms = (now - active.started).num_milliseconds();
    let ev = break_event(now, active.phase, kind, outcome, shown_ms, active.delayed_by, results);
    fx.push(Effect::Log(with_via(ev, via)));
    fx.push(Effect::CloseOverlay);
}

fn end_day(state: &mut State, now: NaiveDateTime, ts: NaiveDateTime, auto: bool, fx: &mut Vec<Effect>) {
    state.ended_on = Some(now.date());
    state.ended_ts = Some(ts);
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
    phase: Phase,
    kind: BreakKind,
    outcome: Outcome,
    shown_ms: i64,
    delayed_by: Option<BlockReason>,
    exercises: Vec<ExerciseResult>,
) -> Event {
    Event::Break { ts, kind, phase, outcome, delayed_by, via: Via::Desktop, shown_ms, exercises }
}
