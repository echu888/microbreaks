//! Platform-free scheduling and state logic for microbreaks.

pub mod config;
pub mod event;
pub mod step;

pub use config::Config;
pub use event::{BreakKind, Event, Outcome};
pub use step::{next_break, status_line, step, BreakSpec, Effect, Input, Presence, State};

#[cfg(test)]
mod step_tests;
