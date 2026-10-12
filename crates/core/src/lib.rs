//! Platform-free scheduling and state logic for microbreaks.

pub mod config;
pub mod event;
pub mod step;

pub use config::Config;
pub use event::{BlockReason, BreakKind, Event, Outcome};
pub use step::{control_body, next_break, parse_control, status_line, step, BreakSpec, Effect, Input, Presence, State};

#[cfg(test)]
mod step_tests;
