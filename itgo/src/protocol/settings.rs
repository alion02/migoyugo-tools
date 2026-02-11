use serde::Deserialize;
use serde_json::Value;

use crate::tt;

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct SettingsPatch {
    pub blocking_command: Option<BlockingCommand>,
    pub dyn_mem: Option<usize>,
    #[serde(flatten)]
    pub unknown: Value,
}

#[derive(Debug, Clone, Copy)]
pub struct Settings {
    pub blocking_command: BlockingCommand,
    pub dyn_mem: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            blocking_command: Default::default(),
            dyn_mem: 1 << 14, // Arbitrary small amount
        }
    }
}

impl Settings {
    pub fn apply(&mut self, patch: &SettingsPatch) {
        *self = Self {
            blocking_command: patch.blocking_command.unwrap_or(self.blocking_command),
            dyn_mem: patch.dyn_mem.unwrap_or(self.dyn_mem),
        };
    }

    pub fn tt_len(&self) -> usize {
        self.dyn_mem / size_of::<tt::Entry>()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockingCommand {
    /// Assume it's a mistake and warn the user.
    #[default]
    Warn,
    /// Allow (e.g., for batch processing).
    Allow,
    /// Force stop current search (e.g., for easier interactive use).
    Stop,
}
