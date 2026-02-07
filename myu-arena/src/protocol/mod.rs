//! Communication protocol for Migoyugo engines.
//!
//! This module defines the message types used for communication between the arena
//! and engines. The protocol uses JSON serialization over stdin/stdout.

pub mod limits;
pub mod mv;

use serde::{Deserialize, Serialize};
pub use serde_json::{from_str as deserialize, to_string as serialize};

use crate::protocol::{limits::Limits, mv::Mv};

/// Message sent from the engine to the arena.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineMsg {
    /// Sent at startup to identify the engine.
    About {
        name: Option<String>,
        author: Option<String>,
        version: Option<String>,
        #[serde(default)]
        features: Vec<String>,
    },
    /// Acknowledgment sent in response to a [`UserMsg::Sync`] message.
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
    Best(Option<Mv>),
    /// A warning message.
    Warn(String),
    /// An error message.
    Error(String),
}

/// Message sent from the arena to the engine.
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UserMsg {
    /// Play a sequence of moves on the current board.
    Play(Vec<Mv>),
    /// Reset the game state to the initial position.
    Reset,
    /// Synchronization barrier.
    Sync,
    /// Start a search with the given limits.
    Go(Limits),
    /// Stop the ongoing search immediately.
    Stop,
}

/// Evaluation score.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Eval {
    /// A regular score in engine-defined abstract units.
    Score(i32),
    /// A decisive score (forced win/loss).
    Decisive(i32),
}
