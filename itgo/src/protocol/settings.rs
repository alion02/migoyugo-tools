use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct SettingsPatch {
    pub blocking_command: Option<BlockingCommand>,
    #[serde(flatten)]
    pub unknown: Value,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Settings {
    pub blocking_command: BlockingCommand,
}

impl Settings {
    pub fn apply(&mut self, patch: &SettingsPatch) {
        *self = Self { blocking_command: patch.blocking_command.unwrap_or(self.blocking_command) };
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
