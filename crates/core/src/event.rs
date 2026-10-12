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
    Locked,
    Asleep,
    Meeting,
    Busy,
    /// Dropped because the user had paused.
    Paused,
    Replaced,
}

/// Why a break was held back. Also the outcome of a break that was never resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BlockReason {
    Away,
    Locked,
    Asleep,
    Meeting,
    Busy,
}

impl BlockReason {
    pub fn outcome(self) -> Outcome {
        match self {
            BlockReason::Away => Outcome::Away,
            BlockReason::Locked => Outcome::Locked,
            BlockReason::Asleep => Outcome::Asleep,
            BlockReason::Meeting => Outcome::Meeting,
            BlockReason::Busy => Outcome::Busy,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    Work,
    /// Also what breaks started outside the Work window (e.g. a manual break on a weekend morning) log.
    Offhours,
}

/// Where a "going to sleep" came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SleepSource {
    Phone,
    Tray,
    Auto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Via {
    Desktop,
    Phone,
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
        delayed_by: Option<BlockReason>,
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
    /// A pause starts; it resumes by itself at `until`.
    Pause {
        ts: NaiveDateTime,
        until: NaiveDateTime,
    },
    /// A pause cut short by the user (a natural expiry logs nothing).
    PauseEnd {
        ts: NaiveDateTime,
    },
    /// The user's Sleep phase begins (not the PC sleeping, which logs `asleep` breaks).
    Sleep {
        ts: NaiveDateTime,
        source: SleepSource,
    },
}
