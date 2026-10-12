use chrono::{NaiveTime, Weekday};
use serde::{Deserialize, Deserializer, Serialize};

/// Written to `config.toml` on first run.
pub const DEFAULT_CONFIG_TOML: &str = include_str!("default_config.toml");

#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(default)]
pub struct Config {
    pub workday: Workday,
    pub eye: Eye,
    #[serde(rename = "move")]
    pub movement: Movement,
    pub overlay: Overlay,
    pub suppress: Suppress,
    pub toasts: Toasts,
    pub ntfy: Ntfy,
    pub offhours: Offhours,
}

impl Config {
    pub fn from_toml(s: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(s)
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct Workday {
    #[serde(deserialize_with = "weekdays")]
    pub days: Vec<Weekday>,
    #[serde(deserialize_with = "hhmm")]
    pub start: NaiveTime,
    #[serde(deserialize_with = "hhmm")]
    pub end: NaiveTime,
    pub overtime_idle_mins: u64,
}

impl Default for Workday {
    fn default() -> Self {
        Self {
            days: vec![Weekday::Mon, Weekday::Tue, Weekday::Wed, Weekday::Thu, Weekday::Fri],
            start: NaiveTime::from_hms_opt(8, 0, 0).unwrap(),
            end: NaiveTime::from_hms_opt(17, 0, 0).unwrap(),
            overtime_idle_mins: 30,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct Eye {
    pub enabled: bool,
    pub minutes: Vec<u32>,
    pub duration_secs: u32,
    pub idle_skip_secs: u64,
}

impl Default for Eye {
    fn default() -> Self {
        Self { enabled: true, minutes: vec![20, 40], duration_secs: 20, idle_skip_secs: 120 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MoveMode {
    All,
    Rotate,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Exercise {
    pub name: String,
    pub reps: u32,
}

impl Default for Exercise {
    fn default() -> Self {
        Self { name: "squats".into(), reps: 15 }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct Movement {
    pub enabled: bool,
    pub idle_skip_secs: u64,
    pub grace_mins: u64,
    pub mode: MoveMode,
    pub exercises: Vec<Exercise>,
}

impl Default for Movement {
    fn default() -> Self {
        Self {
            enabled: true,
            idle_skip_secs: 120,
            grace_mins: 15,
            mode: MoveMode::All,
            exercises: vec![Exercise::default()],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Monitors {
    All,
    Active,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct Overlay {
    pub monitors: Monitors,
    pub opacity: f32,
    pub input_grace_ms: u64,
}

impl Default for Overlay {
    fn default() -> Self {
        Self { monitors: Monitors::All, opacity: 0.85, input_grace_ms: 1000 }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct Offhours {
    pub enabled: bool,
    /// Non-work days become Off-hours after this time.
    #[serde(deserialize_with = "hhmm")]
    pub weekend_start: NaiveTime,
    /// Auto-sleep backstop. Must be after `weekend_start` and before midnight.
    #[serde(deserialize_with = "hhmm")]
    pub quiet_after: NaiveTime,
    /// Eye breaks in Off-hours too (only when active at the PC).
    pub eye: bool,
}

impl Default for Offhours {
    fn default() -> Self {
        Self {
            enabled: false,
            weekend_start: NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
            quiet_after: NaiveTime::from_hms_opt(22, 0, 0).unwrap(),
            eye: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct Ntfy {
    pub enabled: bool,
    pub server: String,
    /// Desktop → phone.
    pub alerts_topic: String,
    /// Phone → desktop.
    pub control_topic: String,
}

impl Default for Ntfy {
    fn default() -> Self {
        Self {
            enabled: false,
            server: "https://ntfy.sh".into(),
            alerts_topic: String::new(),
            control_topic: String::new(),
        }
    }
}

impl Ntfy {
    /// Enabled with two valid, distinct topics. Anything else means no phone traffic at all.
    pub fn is_ready(&self) -> bool {
        self.enabled
            && valid_topic(&self.alerts_topic)
            && valid_topic(&self.control_topic)
            && self.alerts_topic != self.control_topic
    }
}

/// ntfy topics are 1-64 characters of letters, digits, `_` and `-`.
pub fn valid_topic(t: &str) -> bool {
    (1..=64).contains(&t.len()) && t.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Toasts that explain why a movement break was held or missed.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(default)]
pub struct Toasts {
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct Suppress {
    pub camera_mic: bool,
    pub fullscreen: bool,
    /// Exe names the camera/mic probe skips (read by the Windows probe, not by core).
    pub ignore_mic_apps: Vec<String>,
}

impl Default for Suppress {
    fn default() -> Self {
        Self { camera_mic: true, fullscreen: true, ignore_mic_apps: Vec::new() }
    }
}

fn hhmm<'de, D: Deserializer<'de>>(d: D) -> Result<NaiveTime, D::Error> {
    let s = String::deserialize(d)?;
    NaiveTime::parse_from_str(&s, "%H:%M").map_err(serde::de::Error::custom)
}

fn weekdays<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<Weekday>, D::Error> {
    Vec::<String>::deserialize(d)?
        .iter()
        .map(|s| s.parse::<Weekday>().map_err(serde::de::Error::custom))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipped_default_file_matches_defaults() {
        assert_eq!(Config::from_toml(DEFAULT_CONFIG_TOML).unwrap(), Config::default());
    }

    #[test]
    fn partial_file_falls_back_to_defaults() {
        let c = Config::from_toml("[workday]\nstart = \"09:30\"\n[move]\nmode = \"rotate\"").unwrap();
        assert_eq!(c.workday.start, NaiveTime::from_hms_opt(9, 30, 0).unwrap());
        assert_eq!(c.workday.end, Workday::default().end);
        assert_eq!(c.movement.mode, MoveMode::Rotate);
        assert_eq!(c.eye, Eye::default());
    }

    #[test]
    fn bad_time_is_an_error() {
        assert!(Config::from_toml("[workday]\nstart = \"8am\"").is_err());
    }
}
