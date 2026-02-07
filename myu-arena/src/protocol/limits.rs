use serde::Serialize;

/// Search limits sent to the engine.
#[derive(Debug, Clone, Copy, Default, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct Limits {
    /// Search to a fixed depth.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub depth: Option<u32>,
    /// Search a fixed number of nodes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nodes: Option<u64>,
    /// Search for a fixed amount of time (in milliseconds).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time: Option<u64>,
}

impl Limits {
    pub fn time(ms: u64) -> Self {
        Self { time: Some(ms), ..Default::default() }
    }
}
