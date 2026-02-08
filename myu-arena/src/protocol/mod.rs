//! Communication protocol for Migoyugo engines.
//!
//! This module defines the message types used for communication between the arena
//! and engines. The protocol uses JSON serialization over stdin/stdout.

pub mod limits;
pub mod mv;

use serde::{Deserialize, Serialize};

use crate::protocol::{limits::Limits, mv::Mv};

/// Message sent from the engine to the arena.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineMsg {
    /// Sent at startup to identify the engine.
    About { name: Option<String> },
    /// Acknowledgment sent in response to a [`UserMsg::Sync`] message.
    Ready,
    /// The best move found by the search.
    Best(Option<Mv>),
    /// An error message.
    Error(String),
    /// Any other message type.
    #[serde(untagged)]
    Unknown(#[allow(dead_code)] serde_json::Value),
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
    /// Set engine settings.
    Set(serde_json::Map<String, serde_json::Value>),
}
