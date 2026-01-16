use std::borrow::Cow;

pub use ron::{from_str as deserialize, to_string as serialize};
use serde::{Deserialize, Serialize};

pub use myu_core::Sq;

#[derive(Debug, Serialize, Deserialize)]
pub enum EngineMsg {
    Id {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<Cow<'static, str>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        author: Option<Cow<'static, str>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        version: Option<Cow<'static, str>>,
    },
    Ready,
    Info {
        pv: Vec<Sq>,
        eval: Eval,
        depth: u32,
        nodes: u64,
        time: u64,
        knps: u64,
    },
    Best(Option<Sq>),
    Error(Cow<'static, str>),
}

#[derive(Debug, Serialize, Deserialize)]
pub enum UserMsg {
    Reset,
    Sync,
    Undo(usize),
    Play(Vec<Sq>),
    Go(Vec<Limit>),
    Stop,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Eval {
    Score(i32),
    Decisive(i32),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum Limit {
    Depth(u32),
    Nodes(u64),
    Ms(u64),
}
