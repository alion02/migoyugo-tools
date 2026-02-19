pub mod limits;
pub mod mv;
pub mod settings;

use std::borrow::Cow;

use serde::{Deserialize, Serialize};
pub use serde_json::{from_str as deserialize, to_string as serialize};

use crate::protocol::{limits::Limits, mv::Mv, settings::SettingsPatch};

/// Message sent from the engine to the user/harness.
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineMsg {
    /// Sent at startup to identify the engine.
    About { name: &'static str, author: &'static str, version: &'static str, features: &'static [&'static str] },
    /// Acknowledgment sent in response to a [`UserMsg::Sync`] message.
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
        /// The number of PV nodes visited.
        pv_nodes: u64,
    },
    /// The best move found by the search.
    /// `None` if no move is available (e.g., in a terminal state).
    Best(Option<Mv>),
    /// A warning message.
    Warn(Cow<'static, str>),
    /// An error message.
    Error(Cow<'static, str>),
}

/// Message sent from the user/harness to the engine.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserMsg {
    /// Change a set of settings.
    Set(SettingsPatch),
    /// Play a sequence of moves on the current board.
    Play(Vec<Mv>),
    /// Undo the specified number of half-moves (plies).
    Undo(usize),
    /// Discard all played moves and replace them with the sequence.
    Moves(Vec<Mv>),
    /// Reset the game state to the initial position.
    Reset,
    /// Synchronization barrier.
    /// The engine must respond with `EngineMsg::Ready` when it has processed all previous messages.
    /// This is used to ensure that the engine is in a known state.
    Sync,
    /// Start a search with the given limits.
    /// The engine may send `Info` messages during search and finally `Best` when done.
    Go(Limits),
    /// Stop the ongoing search immediately.
    Stop,
    /// Request printing debug information to stderr.
    Debug,
}

/// Evaluation score.
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Eval {
    /// A regular score in engine-defined abstract units.
    Score(i32),
    /// A decisive score (forced win/loss).
    /// The value indicates the distance to the end in plies.
    /// Positive for win, negative for loss.
    Decisive(i32),
}
