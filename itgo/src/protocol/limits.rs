use serde::Deserialize;

pub const MAX_DEPTH: u32 = 64;

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct Limits {
    /// Search to a fixed depth.
    pub depth: u32,
    /// Search a fixed number of nodes.
    pub nodes: u64,
    /// Search for a fixed amount of time (in milliseconds).
    pub time: u64,
    /// Real-time clock (in milliseconds).
    pub clock: Option<Clock>,
}

#[derive(Debug, Default, Clone, Copy, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct Clock {
    /// Time left on the clock.
    pub left: u64,
    /// Time gained (or lost, if negative) after a move.
    pub incr: i64,
}

impl Default for Limits {
    fn default() -> Self {
        Self { depth: MAX_DEPTH, nodes: !0, time: !0, clock: None }
    }
}
