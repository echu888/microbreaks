//! Summarises one day of `events.jsonl` for the stats window.

use chrono::{Duration, NaiveDate, NaiveDateTime};
use serde::Serialize;
use serde_json::Value;

/// How the breaks of one kind turned out. `missed` covers every outcome where the user never
/// got to do the break (away, locked, asleep, meeting, busy, replaced). `paused` is the user's own
/// choice, so it is counted separately and left out of `due`.
#[derive(Debug, Default, Clone, PartialEq, Serialize)]
pub struct KindStats {
    pub done: u32,
    pub skipped: u32,
    pub missed: u32,
    pub paused: u32,
    /// `done + skipped + missed`: the breaks the user was actually asked for.
    pub due: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ExerciseTotal {
    pub name: String,
    pub reps: u64,
    pub target: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DayStats {
    pub date: NaiveDate,
    /// Whether the log has any event for this day. A day the app never ran shows as "no data",
    /// not as a day of zero breaks.
    pub has_data: bool,
    pub eye: KindStats,
    pub movement: KindStats,
    /// Reps against targets, over the movement breaks that were done.
    pub exercises: Vec<ExerciseTotal>,
    /// The shutdown time, unless the day was resumed afterwards.
    pub day_end: Option<NaiveDateTime>,
    /// Bedtime, if the user went to sleep that day.
    pub sleep: Option<NaiveDateTime>,
}

/// The last seven days, newest first, with totals across all of them.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WeekStats {
    pub days: Vec<DayStats>,
    pub eye: KindStats,
    pub movement: KindStats,
    pub exercises: Vec<ExerciseTotal>,
}

const WEEK_DAYS: i64 = 7;

type Parsed = Vec<(NaiveDateTime, Value)>;

/// Keeps the lines that are valid JSON with a readable `ts`; blank or broken lines are skipped.
fn parse<S: AsRef<str>>(lines: impl IntoIterator<Item = S>) -> Parsed {
    lines
        .into_iter()
        .filter_map(|line| {
            let v = serde_json::from_str::<Value>(line.as_ref()).ok()?;
            let ts = NaiveDateTime::parse_from_str(v["ts"].as_str()?, "%Y-%m-%dT%H:%M:%S%.f").ok()?;
            Some((ts, v))
        })
        .collect()
}

/// Reads event lines (as written to `events.jsonl`) and totals the ones stamped on `date`.
pub fn summarize_day<S: AsRef<str>>(lines: impl IntoIterator<Item = S>, date: NaiveDate) -> DayStats {
    day_from(&parse(lines), date)
}

/// `end` and the six days before it, newest first. The log is read once.
pub fn summarize_week<S: AsRef<str>>(lines: impl IntoIterator<Item = S>, end: NaiveDate) -> WeekStats {
    let events = parse(lines);
    let days: Vec<DayStats> = (0..WEEK_DAYS).map(|back| day_from(&events, end - Duration::days(back))).collect();
    let mut week = WeekStats {
        days: Vec::new(),
        eye: KindStats::default(),
        movement: KindStats::default(),
        exercises: Vec::new(),
    };
    for d in &days {
        add_kind(&mut week.eye, &d.eye);
        add_kind(&mut week.movement, &d.movement);
        for e in &d.exercises {
            add_total(&mut week.exercises, &e.name, e.reps, e.target);
        }
    }
    week.days = days;
    week
}

fn add_kind(total: &mut KindStats, day: &KindStats) {
    total.done += day.done;
    total.skipped += day.skipped;
    total.missed += day.missed;
    total.paused += day.paused;
    total.due += day.due;
}

fn day_from(events: &Parsed, date: NaiveDate) -> DayStats {
    let mut stats = DayStats {
        date,
        has_data: false,
        eye: KindStats::default(),
        movement: KindStats::default(),
        exercises: Vec::new(),
        day_end: None,
        sleep: None,
    };
    for (ts, v) in events {
        if ts.date() != date {
            continue;
        }
        match v["type"].as_str() {
            Some("break") => {
                let kind = match v["kind"].as_str() {
                    Some("eye") => &mut stats.eye,
                    Some("move") => &mut stats.movement,
                    _ => continue,
                };
                stats.has_data = true;
                match v["outcome"].as_str() {
                    Some("done") => kind.done += 1,
                    Some("skipped") => kind.skipped += 1,
                    Some("paused") => kind.paused += 1,
                    Some("away" | "locked" | "asleep" | "meeting" | "busy" | "replaced") => kind.missed += 1,
                    _ => {}
                }
                if v["outcome"] == "done" {
                    for e in v["exercises"].as_array().into_iter().flatten() {
                        add_exercise(&mut stats.exercises, e);
                    }
                }
            }
            Some("day_end") => {
                stats.has_data = true;
                stats.day_end = Some(*ts);
            }
            Some("day_resume") => {
                stats.has_data = true;
                stats.day_end = None;
            }
            Some("sleep") => {
                stats.has_data = true;
                stats.sleep = Some(*ts);
            }
            _ => {}
        }
    }
    for kind in [&mut stats.eye, &mut stats.movement] {
        kind.due = kind.done + kind.skipped + kind.missed;
    }
    stats
}

fn add_exercise(totals: &mut Vec<ExerciseTotal>, e: &Value) {
    if let (Some(name), Some(reps), Some(target)) = (e["name"].as_str(), e["reps"].as_u64(), e["target"].as_u64()) {
        add_total(totals, name, reps, target);
    }
}

fn add_total(totals: &mut Vec<ExerciseTotal>, name: &str, reps: u64, target: u64) {
    match totals.iter_mut().find(|t| t.name == name) {
        Some(t) => {
            t.reps += reps;
            t.target += target;
        }
        None => totals.push(ExerciseTotal { name: name.to_string(), reps, target }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, 12).unwrap()
    }

    fn at(h: u32, m: u32) -> NaiveDateTime {
        day().and_hms_opt(h, m, 0).unwrap()
    }

    #[test]
    fn counts_outcomes_per_kind_and_keeps_paused_out_of_due() {
        let lines = [
            r#"{"type":"break","ts":"2026-10-12T08:20:05","kind":"eye","outcome":"done"}"#,
            r#"{"type":"break","ts":"2026-10-12T08:40:05","kind":"eye","outcome":"skipped"}"#,
            r#"{"type":"break","ts":"2026-10-12T09:20:00","kind":"eye","outcome":"meeting"}"#,
            r#"{"type":"break","ts":"2026-10-12T09:40:00","kind":"eye","outcome":"replaced"}"#,
            r#"{"type":"break","ts":"2026-10-12T10:20:00","kind":"eye","outcome":"paused"}"#,
            r#"{"type":"break","ts":"2026-10-12T10:00:00","kind":"move","outcome":"away"}"#,
        ];
        let s = summarize_day(lines, day());
        assert_eq!(s.eye, KindStats { done: 1, skipped: 1, missed: 2, paused: 1, due: 4 });
        assert_eq!(s.movement, KindStats { done: 0, skipped: 0, missed: 1, paused: 0, due: 1 });
    }

    #[test]
    fn totals_reps_against_targets_for_done_movement_breaks_only() {
        let lines = [
            r#"{"type":"break","ts":"2026-10-12T09:00:30","kind":"move","outcome":"done","exercises":[{"name":"squats","target":15,"reps":15},{"name":"pushups","target":10,"reps":7}]}"#,
            r#"{"type":"break","ts":"2026-10-12T10:00:30","kind":"move","outcome":"done","exercises":[{"name":"squats","target":15,"reps":12}]}"#,
            r#"{"type":"break","ts":"2026-10-12T11:00:30","kind":"move","outcome":"skipped","exercises":[{"name":"squats","target":15,"reps":99}]}"#,
        ];
        let s = summarize_day(lines, day());
        assert_eq!(s.movement.done, 2);
        assert_eq!(
            s.exercises,
            [
                ExerciseTotal { name: "squats".into(), reps: 27, target: 30 },
                ExerciseTotal { name: "pushups".into(), reps: 7, target: 10 },
            ]
        );
    }

    #[test]
    fn ignores_other_days_and_unreadable_lines() {
        let lines = [
            r#"{"type":"break","ts":"2026-10-11T23:59:59","kind":"eye","outcome":"done"}"#,
            r#"{"type":"break","ts":"2026-10-13T00:00:00","kind":"eye","outcome":"done"}"#,
            "",
            "not json",
            r#"{"type":"break","kind":"eye","outcome":"done"}"#,
            r#"{"type":"break","ts":"yesterday","kind":"eye","outcome":"done"}"#,
            r#"{"type":"break","ts":"2026-10-12T09:00:00","kind":"nap","outcome":"done"}"#,
            r#"{"type":"break","ts":"2026-10-12T09:00:00","kind":"eye","outcome":"???"}"#,
        ];
        let s = summarize_day(lines, day());
        assert_eq!(s.eye, KindStats::default());
        assert_eq!(s.movement, KindStats::default());
    }

    #[test]
    fn day_end_is_cleared_by_a_resume_and_sleep_is_kept() {
        let lines = [
            r#"{"type":"day_end","ts":"2026-10-12T17:10:00","auto":true}"#,
            r#"{"type":"day_resume","ts":"2026-10-12T17:30:00"}"#,
        ];
        assert_eq!(summarize_day(lines, day()).day_end, None);

        let lines = [
            r#"{"type":"day_end","ts":"2026-10-12T16:00:00","auto":false}"#,
            r#"{"type":"day_resume","ts":"2026-10-12T16:05:00"}"#,
            r#"{"type":"day_end","ts":"2026-10-12T18:40:00","auto":true}"#,
            r#"{"type":"sleep","ts":"2026-10-12T22:00:00","source":"auto"}"#,
        ];
        let s = summarize_day(lines, day());
        assert_eq!(s.day_end, Some(at(18, 40)));
        assert_eq!(s.sleep, Some(at(22, 0)));
    }

    #[test]
    fn reads_fractional_seconds_as_the_app_writes_them() {
        let line = r#"{"type":"break","ts":"2026-10-12T15:40:01.019365200","kind":"eye","outcome":"meeting","delayed_by":null,"via":"desktop","shown_ms":0}"#;
        assert_eq!(summarize_day([line], day()).eye.missed, 1);
    }

    #[test]
    fn a_day_with_events_has_data_and_an_empty_day_does_not() {
        let lines = [r#"{"type":"break","ts":"2026-10-12T09:00:00","kind":"eye","outcome":"paused"}"#];
        assert!(summarize_day(lines, day()).has_data);
        assert!(!summarize_day(lines, day() - Duration::days(1)).has_data);
        assert!(!summarize_day(["junk"], day()).has_data);
    }

    #[test]
    fn week_covers_seven_days_newest_first_and_totals_them() {
        let lines = [
            r#"{"type":"break","ts":"2026-10-12T09:00:30","kind":"move","outcome":"done","exercises":[{"name":"squats","target":15,"reps":15}]}"#,
            r#"{"type":"break","ts":"2026-10-10T09:00:30","kind":"move","outcome":"done","exercises":[{"name":"squats","target":15,"reps":10},{"name":"pushups","target":10,"reps":10}]}"#,
            r#"{"type":"break","ts":"2026-10-10T09:20:00","kind":"eye","outcome":"skipped"}"#,
            r#"{"type":"break","ts":"2026-10-06T09:00:30","kind":"move","outcome":"skipped"}"#,
            // Eight days back: outside the window.
            r#"{"type":"break","ts":"2026-10-04T09:00:30","kind":"move","outcome":"done","exercises":[{"name":"squats","target":15,"reps":15}]}"#,
        ];
        let w = summarize_week(lines, day());
        let dates: Vec<_> = w.days.iter().map(|d| d.date.to_string()).collect();
        assert_eq!(
            dates,
            ["2026-10-12", "2026-10-11", "2026-10-10", "2026-10-09", "2026-10-08", "2026-10-07", "2026-10-06"]
        );
        assert_eq!(
            w.days.iter().map(|d| d.has_data).collect::<Vec<_>>(),
            [true, false, true, false, false, false, true]
        );
        assert_eq!(w.movement, KindStats { done: 2, skipped: 1, missed: 0, paused: 0, due: 3 });
        assert_eq!(w.eye, KindStats { done: 0, skipped: 1, missed: 0, paused: 0, due: 1 });
        assert_eq!(
            w.exercises,
            [
                ExerciseTotal { name: "squats".into(), reps: 25, target: 30 },
                ExerciseTotal { name: "pushups".into(), reps: 10, target: 10 },
            ]
        );
    }

    #[test]
    fn week_with_no_events_is_all_empty() {
        let w = summarize_week(Vec::<String>::new(), day());
        assert_eq!(w.days.len(), 7);
        assert!(w.days.iter().all(|d| !d.has_data));
        assert_eq!(w.movement, KindStats::default());
        assert!(w.exercises.is_empty());
    }
}
