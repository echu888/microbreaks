//! Summarises one day of `events.jsonl` for the stats window.

use chrono::{NaiveDate, NaiveDateTime};
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
    pub eye: KindStats,
    pub movement: KindStats,
    /// Reps against targets, over the movement breaks that were done.
    pub exercises: Vec<ExerciseTotal>,
    /// The shutdown time, unless the day was resumed afterwards.
    pub day_end: Option<NaiveDateTime>,
    /// Bedtime, if the user went to sleep that day.
    pub sleep: Option<NaiveDateTime>,
}

/// Reads event lines (as written to `events.jsonl`) and totals the ones stamped on `date`.
/// Lines that are blank or not valid events are skipped.
pub fn summarize_day<S: AsRef<str>>(lines: impl IntoIterator<Item = S>, date: NaiveDate) -> DayStats {
    let mut stats = DayStats {
        date,
        eye: KindStats::default(),
        movement: KindStats::default(),
        exercises: Vec::new(),
        day_end: None,
        sleep: None,
    };
    for line in lines {
        let Ok(v) = serde_json::from_str::<Value>(line.as_ref()) else { continue };
        let Some(ts) = v["ts"].as_str().and_then(|s| NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f").ok())
        else {
            continue;
        };
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
            Some("day_end") => stats.day_end = Some(ts),
            Some("day_resume") => stats.day_end = None,
            Some("sleep") => stats.sleep = Some(ts),
            _ => {}
        }
    }
    for kind in [&mut stats.eye, &mut stats.movement] {
        kind.due = kind.done + kind.skipped + kind.missed;
    }
    stats
}

fn add_exercise(totals: &mut Vec<ExerciseTotal>, e: &Value) {
    let (Some(name), Some(reps), Some(target)) = (e["name"].as_str(), e["reps"].as_u64(), e["target"].as_u64()) else {
        return;
    };
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
}
