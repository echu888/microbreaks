use chrono::NaiveDateTime;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BreakKind {
    Eye,
    Move,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Outcome {
    Done,
    Skipped,
    Away,
    Replaced,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    Work,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Via {
    Desktop,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExerciseResult {
    pub name: String,
    pub target: u32,
    pub reps: u32,
}

/// One line of `events.jsonl`. Timestamps are local wall-clock time.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    Break {
        ts: NaiveDateTime,
        kind: BreakKind,
        phase: Phase,
        outcome: Outcome,
        via: Via,
        shown_ms: i64,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        exercises: Vec<ExerciseResult>,
    },
    DayEnd {
        ts: NaiveDateTime,
        auto: bool,
    },
    DayResume {
        ts: NaiveDateTime,
    },
}
