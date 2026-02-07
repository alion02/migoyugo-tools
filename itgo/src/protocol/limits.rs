use serde::Deserialize;

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct Limits {
    /// Search to a fixed depth.
    pub depth: u32,
    /// Search a fixed number of nodes.
    pub nodes: u64,
    /// Search for a fixed amount of time (in milliseconds).
    pub time: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self { depth: 64, nodes: !0, time: !0 }
    }
}
