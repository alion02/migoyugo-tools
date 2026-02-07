use serde::Deserialize;

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct SettingsPatch {
    pub blocking_command: Option<BlockingCommand>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Settings {
    pub blocking_command: BlockingCommand,
}

impl Settings {
    pub fn apply(&mut self, patch: SettingsPatch) {
        *self = Self { blocking_command: patch.blocking_command.unwrap_or(self.blocking_command) };
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockingCommand {
    /// Assume it's a mistake and warn the user.
    #[default]
    Warn,
    /// Allow, for e.g. batch processing.
    Allow,
    /// Force stop current search, for e.g. easier interactive use.
    Stop,
}
