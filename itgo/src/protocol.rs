use std::{
    borrow::Cow,
    fmt::{self, Display, Formatter},
};

use serde::{Deserialize, Serialize};
pub use serde_json::{from_str as deserialize, to_string as serialize};

/// Messages sent from the engine to the user/GUI.
#[derive(Debug, Serialize, Deserialize)]
pub enum EngineMsg {
    /// Sent at startup to identify the engine.
    Id {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<Cow<'static, str>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        author: Option<Cow<'static, str>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        version: Option<Cow<'static, str>>,
    },
    /// Acknowledgment sent in response to a `UserMsg::Sync` message.
    /// Indicates that the engine has processed all previous messages and is ready for new commands.
    Ready,
    /// Information about the search progress.
    Info {
        /// The principal variation (best line found so far).
        pv: Vec<Mv>,
        /// The evaluation of the current position.
        eval: Eval,
        /// The depth of the search.
        depth: u32,
        /// The time elapsed in milliseconds.
        time: u64,
        /// The number of nodes searched.
        nodes: u64,
        /// Kilo-nodes per second.
        knps: u64,
        /// The number of evaluations performed.
        evals: u64,
        /// Kilo-evals per second.
        keps: u64,
    },
    /// The best move found by the search.
    /// `None` if no move is available (e.g., in a terminal state).
    Best(Option<Mv>),
    /// An error message.
    Error(Cow<'static, str>),
}

/// Messages sent from the user/GUI to the engine.
#[derive(Debug, Serialize, Deserialize)]
pub enum UserMsg {
    /// Resets the game state to the initial position.
    /// Stops any ongoing search.
    Reset,
    /// Synchronization barrier.
    /// The engine must respond with `EngineMsg::Ready` when it has processed all previous messages.
    /// This is used to ensure that the engine is in a known state.
    Sync,
    /// Undoes the specified number of half-moves (plies).
    /// Stops any ongoing search.
    Undo(usize),
    /// Plays a sequence of moves on the current board.
    /// Stops any ongoing search.
    Play(Vec<Mv>),
    /// Starts a search with the given limits.
    /// The engine will send `Info` messages during the search and finally `Best` when done.
    Go(Vec<Limit>),
    /// Stops the ongoing search immediately.
    Stop,
    /// Requests that the engine print debug information to stderr.
    Debug,
}

/// A wrapper around `myu_core::Mv` representing a square on the board.
/// Serializes to/from a string representation (e.g., "a1").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "&str", into = "String")]
pub struct Mv(u8);

impl Mv {
    pub fn from_raw(raw: u8) -> Option<Self> {
        if raw < 64 { Some(Self(raw)) } else { None }
    }

    pub fn from_col_row(col: u8, row: u8) -> Option<Self> {
        if col < 8 && row < 8 { Some(Self(col | row << 3)) } else { None }
    }

    pub fn raw(self) -> u8 {
        self.0
    }

    pub fn col(self) -> u8 {
        self.0 & 7
    }

    pub fn row(self) -> u8 {
        self.0 >> 3
    }
}

impl TryFrom<&str> for Mv {
    type Error = ParseMvError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let [col, row] = *value.as_bytes() else { return Err(ParseMvError::BadLen) };
        let c @ ..8 = col.wrapping_sub(b'a') else { return Err(ParseMvError::BadCol) };
        let r @ ..8 = row.wrapping_sub(b'1') else { return Err(ParseMvError::BadRow) };
        Ok(Self::from_col_row(c, r).unwrap())
    }
}

impl From<Mv> for String {
    fn from(value: Mv) -> Self {
        let col = b'a' + value.col();
        let row = b'1' + value.row();
        String::from_utf8(vec![col, row]).unwrap()
    }
}

pub enum ParseMvError {
    BadLen,
    BadCol,
    BadRow,
}

impl Display for ParseMvError {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.write_str(match self {
            ParseMvError::BadLen => "square too long or too short",
            ParseMvError::BadCol => "square column not a-h",
            ParseMvError::BadRow => "square row not 1-8",
        })
    }
}

/// Evaluation score.
#[derive(Debug, Serialize, Deserialize)]
pub enum Eval {
    /// A regular score in engine-defined abstract units.
    Score(i32),
    /// A decisive score (forced win/loss).
    /// The value indicates the distance to the end in plies.
    /// Positive for win, negative for loss.
    Decisive(i32),
}

/// Search limits.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum Limit {
    /// Search to a fixed depth.
    Depth(u32),
    /// Search a fixed number of nodes.
    Nodes(u64),
    /// Search for a fixed amount of time (in milliseconds).
    Ms(u64),
}
