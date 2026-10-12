use chrono::{Duration, NaiveDate, NaiveDateTime, Weekday};

use crate::config::{Config, Exercise, MoveMode};
use crate::event::{BreakKind, Event, Outcome};
use crate::step::{next_break, status_line, step, Effect, Input, Presence, State};

/// 2026-10-12 is a Monday; days 17 and 18 are Sat/Sun.
fn at(day: u32, h: u32, m: u32, s: u32) -> NaiveDateTime {
    NaiveDate::from_ymd_opt(2026, 10, day).unwrap().and_hms_opt(h, m, s).unwrap()
}

#[derive(Default)]
struct Sim {
    state: State,
    cfg: Config,
    now: Option<NaiveDateTime>,
    shown: Vec<(NaiveDateTime, BreakKind)>,
    events: Vec<Event>,
    closed: usize,
    last_id: u64,
}

impl Sim {
    fn new() -> Self {
        Self::default()
    }

    fn tick(&mut self, now: NaiveDateTime, idle_secs: u64, inputs: &[Input]) {
        self.now = Some(now);
        let fx = step(&mut self.state, &self.cfg, now, Presence { idle_secs }, inputs);
        for e in fx {
            match e {
                Effect::ShowOverlay(spec) => {
                    self.shown.push((now, spec.kind));
                    self.last_id = spec.id;
                }
                Effect::CloseOverlay => self.closed += 1,
                Effect::Log(ev) => self.events.push(ev),
            }
        }
    }

    /// Tick every 30 s up to and including `to`; the user is always active.
    fn run_active(&mut self, to: NaiveDateTime) {
        self.run(to, |_| 0, false);
    }

    /// Tick every 30 s; `idle` gives idle seconds at each tick. With `auto_done`
    /// the user acknowledges every break the moment it shows.
    fn run(&mut self, to: NaiveDateTime, idle: impl Fn(NaiveDateTime) -> u64, auto_done: bool) {
        let mut t = self.now.unwrap() + Duration::seconds(30);
        while t <= to {
            let before = self.shown.len();
            self.tick(t, idle(t), &[]);
            if auto_done && self.shown.len() > before {
                let id = self.last_id;
                self.tick(t, idle(t), &[Input::BreakDone { id, reps: vec![] }]);
            }
            t += Duration::seconds(30);
        }
    }

    fn shown_times(&self) -> Vec<String> {
        self.shown.iter().map(|(t, k)| format!("{} {:?}", t.format("%d %H:%M"), k)).collect()
    }

    fn outcomes(&self) -> Vec<Outcome> {
        self.events
            .iter()
            .filter_map(|e| match e {
                Event::Break { outcome, .. } => Some(*outcome),
                _ => None,
            })
            .collect()
    }
}

#[test]
fn first_tick_never_fires() {
    let mut sim = Sim::new();
    sim.tick(at(12, 10, 20, 0), 0, &[]);
    assert!(sim.shown.is_empty());
}

#[test]
fn start_of_day_fires_first_at_0820_not_0800() {
    let mut sim = Sim::new();
    sim.tick(at(12, 7, 59, 0), 0, &[]);
    sim.run_active(at(12, 8, 19, 59));
    assert!(sim.shown.is_empty());
    sim.run_active(at(12, 8, 20, 30));
    assert_eq!(sim.shown_times(), ["12 08:20 Eye"]);
}

#[test]
fn movement_at_the_hour_replaces_the_eye_slot() {
    let mut sim = Sim::new();
    sim.tick(at(12, 8, 30, 0), 0, &[]);
    sim.run(at(12, 9, 30, 0), |_| 0, true);
    assert_eq!(sim.shown_times(), ["12 08:40 Eye", "12 09:00 Move", "12 09:20 Eye"]);
}

#[test]
fn full_day_with_overtime_and_automatic_day_end() {
    let mut sim = Sim::new();
    sim.tick(at(12, 7, 55, 0), 0, &[]);
    // Active until 17:10, then away.
    let idle = |t: NaiveDateTime| {
        let left = at(12, 17, 10, 0);
        if t > left { (t - left).num_seconds() as u64 } else { 0 }
    };
    sim.run(at(12, 19, 0, 0), idle, true);

    let eyes = sim.shown.iter().filter(|(_, k)| *k == BreakKind::Eye).count();
    let moves = sim.shown.iter().filter(|(_, k)| *k == BreakKind::Move).count();
    assert_eq!((eyes, moves), (18, 9), "{:?}", sim.shown_times());
    // 17:00 (still active) is shown; 17:20 is dropped as away; nothing after the day ends.
    assert_eq!(sim.shown.last().unwrap().0, at(12, 17, 0, 0));
    assert_eq!(sim.outcomes().iter().filter(|o| **o == Outcome::Away).count(), 1);

    let end = sim.events.iter().find_map(|e| match e {
        Event::DayEnd { ts, auto } => Some((*ts, *auto)),
        _ => None,
    });
    assert_eq!(end, Some((at(12, 17, 10, 0), true)));
}

#[test]
fn next_morning_starts_fresh() {
    let mut sim = Sim::new();
    sim.tick(at(12, 16, 59, 0), 0, &[]);
    sim.tick(at(12, 17, 30, 0), 3600, &[]); // ended automatically
    assert_eq!(sim.state.next_break_date(&sim.cfg), Some(at(13, 8, 20, 0)));
    sim.run(at(13, 8, 21, 0), |_| 0, false);
    assert_eq!(sim.shown_times(), ["13 08:20 Eye"]);
}

#[test]
fn manual_done_for_the_day_then_resume() {
    let mut sim = Sim::new();
    sim.tick(at(12, 10, 0, 30), 0, &[]);
    sim.tick(at(12, 10, 5, 0), 0, &[Input::DoneForDay]);
    sim.run_active(at(12, 11, 30, 0));
    assert!(sim.shown.is_empty());
    assert!(matches!(sim.events[..], [Event::DayEnd { auto: false, .. }]));

    sim.tick(at(12, 11, 31, 0), 0, &[Input::Resume]);
    assert!(matches!(sim.events[1], Event::DayResume { .. }));
    sim.run_active(at(12, 12, 21, 0));
    assert_eq!(sim.shown_times(), ["12 11:40 Eye", "12 12:00 Move", "12 12:20 Eye"]);
}

#[test]
fn resume_without_an_ended_day_is_ignored() {
    let mut sim = Sim::new();
    sim.tick(at(12, 10, 0, 0), 0, &[Input::Resume]);
    assert!(sim.events.is_empty());
}

#[test]
fn ending_the_day_closes_a_showing_break() {
    let mut sim = Sim::new();
    sim.tick(at(12, 8, 19, 30), 0, &[]);
    sim.tick(at(12, 8, 20, 1), 0, &[]);
    sim.tick(at(12, 8, 21, 0), 0, &[Input::DoneForDay]);
    assert_eq!(sim.closed, 1);
    assert_eq!(sim.outcomes(), [Outcome::Skipped]);
}

#[test]
fn unacknowledged_break_is_replaced_by_the_next() {
    let mut sim = Sim::new();
    sim.tick(at(12, 8, 19, 30), 0, &[]);
    sim.run_active(at(12, 8, 41, 0));
    assert_eq!(sim.shown_times(), ["12 08:20 Eye", "12 08:40 Eye"]);
    assert_eq!(sim.outcomes(), [Outcome::Replaced]);
    assert_eq!(sim.closed, 0, "the new overlay replaces the old one");
}

#[test]
fn stale_ack_for_a_replaced_break_is_ignored() {
    let mut sim = Sim::new();
    sim.tick(at(12, 8, 19, 30), 0, &[]);
    sim.run_active(at(12, 8, 41, 0));
    let stale = sim.last_id - 1;
    sim.tick(at(12, 8, 42, 0), 0, &[Input::BreakDone { id: stale, reps: vec![] }]);
    assert_eq!(sim.outcomes(), [Outcome::Replaced]);
    assert_eq!(sim.closed, 0);
}

#[test]
fn weekends_have_no_breaks() {
    let mut sim = Sim::new();
    sim.tick(at(17, 0, 0, 0), 0, &[]);
    sim.run_active(at(18, 23, 59, 0));
    assert!(sim.shown.is_empty());
    assert_eq!(sim.cfg.workday.days.contains(&Weekday::Sat), false);
}

#[test]
fn a_long_gap_fires_at_most_one_break() {
    let mut sim = Sim::new();
    sim.tick(at(12, 8, 30, 0), 0, &[]);
    sim.tick(at(12, 12, 10, 0), 0, &[]); // e.g. woke from sleep
    assert_eq!(sim.shown_times(), ["12 12:10 Move"]);
}

#[test]
fn eye_break_is_dropped_when_away_but_movement_is_shown() {
    let mut sim = Sim::new();
    sim.tick(at(12, 8, 19, 30), 0, &[]);
    sim.tick(at(12, 8, 20, 1), 60, &[]);
    assert!(sim.shown.is_empty());
    assert_eq!(sim.outcomes(), [Outcome::Away]);
    sim.tick(at(12, 8, 40, 1), 60, &[]);
    sim.tick(at(12, 8, 59, 30), 0, &[]);
    sim.tick(at(12, 9, 0, 1), 60, &[]);
    assert_eq!(sim.shown_times(), ["12 09:00 Move"]);
}

#[test]
fn done_logs_reps_against_targets() {
    let mut sim = Sim::new();
    sim.cfg.movement.exercises.push(Exercise { name: "pushups".into(), reps: 10 });
    sim.tick(at(12, 8, 59, 30), 0, &[]);
    sim.tick(at(12, 9, 0, 1), 0, &[]);
    let id = sim.last_id;
    sim.tick(at(12, 9, 0, 6), 0, &[Input::BreakDone { id, reps: vec![15, 7] }]);
    let json = serde_json::to_value(&sim.events[0]).unwrap();
    assert_eq!(json["type"], "break");
    assert_eq!(json["kind"], "move");
    assert_eq!(json["outcome"], "done");
    assert_eq!(json["shown_ms"], 5000);
    assert_eq!(json["exercises"][1]["name"], "pushups");
    assert_eq!(json["exercises"][1]["target"], 10);
    assert_eq!(json["exercises"][1]["reps"], 7);
}

#[test]
fn rotate_mode_cycles_through_exercises() {
    let mut sim = Sim::new();
    sim.cfg.movement.mode = MoveMode::Rotate;
    sim.cfg.movement.exercises.push(Exercise { name: "pushups".into(), reps: 10 });
    sim.tick(at(12, 8, 59, 30), 0, &[]);
    for (h, name) in [(9, "squats"), (10, "pushups"), (11, "squats")] {
        sim.tick(at(12, h, 0, 1), 0, &[]);
        let id = sim.last_id;
        sim.tick(at(12, h, 0, 2), 0, &[Input::BreakDone { id, reps: vec![1] }]);
        let Some(Event::Break { exercises, .. }) = sim.events.last() else { panic!() };
        assert_eq!(exercises.len(), 1);
        assert_eq!(exercises[0].name, name);
    }
}

#[test]
fn disabled_break_kinds_do_not_fire() {
    let mut sim = Sim::new();
    sim.cfg.eye.enabled = false;
    sim.tick(at(12, 8, 30, 0), 0, &[]);
    sim.run_active(at(12, 10, 30, 0));
    assert_eq!(sim.shown_times(), ["12 09:00 Move", "12 10:00 Move"]);
}

#[test]
fn status_line_and_next_break() {
    let sim = Sim::new();
    let now = at(12, 10, 5, 0);
    assert_eq!(status_line(&sim.state, &sim.cfg, now), "Work · next: eye break 10:20");
    assert_eq!(next_break(&sim.state, &sim.cfg, at(12, 10, 40, 0)).unwrap().0, BreakKind::Move);
    // Friday evening looks ahead to Monday.
    let fri_evening = at(16, 17, 5, 0);
    let mut sim = Sim::new();
    sim.tick(fri_evening, 3600, &[]);
    assert_eq!(status_line(&sim.state, &sim.cfg, fri_evening), "Done for the day · next: eye break Mon 08:20");
}

impl State {
    fn next_break_date(&self, cfg: &Config) -> Option<NaiveDateTime> {
        next_break(self, cfg, at(12, 17, 31, 0)).map(|(_, t)| t)
    }
}
