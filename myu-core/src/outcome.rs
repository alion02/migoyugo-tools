use crate::Color;

/// Game outcome (terminal state).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Igo: 4 yugos in a line.
    Igo(Color),
    /// Wego: no legal moves remaining.
    Wego(WegoResult),
}

/// Result of a Wego (endgame by no legal moves).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WegoResult {
    Win(Color),
    Draw,
}

impl Outcome {
    /// The winner, if any.
    pub fn winner(self) -> Option<Color> {
        match self {
            Outcome::Igo(c) => Some(c),
            Outcome::Wego(WegoResult::Win(c)) => Some(c),
            Outcome::Wego(WegoResult::Draw) => None,
        }
    }
}
