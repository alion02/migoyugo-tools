#[derive(Debug, Clone, Copy, Default)]
pub struct Settings {
    pub blocking_command: BlockingCommand,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BlockingCommand {
    #[default]
    /// Assume it's a mistake and warn the user.
    Warn,
    /// Allow, for e.g. batch processing.
    Allow,
    /// Force stop current search, for e.g. easier interactive use.
    Stop,
}
