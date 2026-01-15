use std::{borrow::Cow, ops::Deref};

pub use ron::{from_str as deserialize, to_string as serialize};
use serde::{Deserialize, Serialize};

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
