use std::{borrow::Cow, ops::Deref};

pub use ron::{from_str as deserialize, to_string as serialize};
use serde::{Deserialize, Serialize};

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
        pv: Vec<Sq>,
        /// The evaluation of the current position.
        eval: Eval,
        /// The depth of the search.
        depth: u32,
        /// The number of nodes searched.
        nodes: u64,
        /// The time elapsed in milliseconds.
        time: u64,
        /// Kilo-nodes per second.
        knps: u64,
    },
    /// The best move found by the search.
    /// `None` if no move is available (e.g., in a terminal state).
    Best(Option<Sq>),
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
    Play(Vec<Sq>),
    /// Starts a search with the given limits.
    /// The engine will send `Info` messages during the search and finally `Best` when done.
    Go(Vec<Limit>),
    /// Stops the ongoing search immediately.
    Stop,
}

/// A wrapper around `myu_core::Sq` representing a square on the board.
/// Serializes to/from a string representation (e.g., "a1").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "&str", into = "String")]
pub struct Sq(myu_core::Sq);

impl Sq {
    pub fn from_raw(raw: u8) -> Option<Self> {
        myu_core::Sq::from_raw(raw).map(Self)
    }
}

impl Deref for Sq {
    type Target = myu_core::Sq;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<myu_core::Sq> for Sq {
    fn from(sq: myu_core::Sq) -> Self {
        Self(sq)
    }
}

impl From<Sq> for myu_core::Sq {
    fn from(value: Sq) -> Self {
        value.0
    }
}

impl TryFrom<&str> for Sq {
    type Error = myu_core::ParseSqError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        myu_core::parse_sq(value).map(Self)
    }
}

impl From<Sq> for String {
    fn from(value: Sq) -> Self {
        myu_core::format_sq(value.0)
    }
}

/// Evaluation score.
#[derive(Debug, Serialize, Deserialize)]
pub enum Eval {
    /// A regular score in centipawns (or similar unit).
    Score(i32),
    /// A decisive score (mate or forced win/loss).
    /// The value indicates the distance to the end (e.g., plies).
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
