use thiserror::Error;

#[derive(Debug, Clone, Error)]
pub enum ParseSqError {
    #[error("square string must be exactly 2 characters")]
    BadLen,
    #[error("column must be a-h")]
    BadCol,
    #[error("row must be 1-8")]
    BadRow,
}

#[derive(Debug, Clone, Error)]
pub enum ParseMvError {
    #[error("invalid square: {0}")]
    Sq(#[from] ParseSqError),
    #[error("invalid yugo count format")]
    BadYugoFormat,
}

#[derive(Debug, Clone, Error)]
pub enum ParseStateError {
    #[error("invalid board: {0}")]
    Board(String),
    #[error("invalid side to move: expected 'w' or 'b'")]
    BadSide,
    #[error("invalid score: {0}")]
    BadScore(String),
    #[error("wrong number of components: expected 4")]
    BadFormat,
}

#[derive(Debug, Clone, Error)]
pub enum ParseGameError {
    #[error("invalid move at position {index}: {source}")]
    Move { index: usize, source: ParseMvError },
    #[error("illegal move at position {0}")]
    Illegal(usize),
    #[error("yugo count mismatch at move {index}: expected {expected}, got {got}")]
    YugoMismatch { index: usize, expected: u8, got: u8 },
    #[error("invalid result: {0}")]
    BadResult(String),
}

#[derive(Debug, Clone, Error)]
pub enum PlayError {
    #[error("square is occupied")]
    Occupied,
    #[error("move would create a line longer than 4")]
    LineTooLong,
    #[error("game is already over")]
    GameOver,
}
