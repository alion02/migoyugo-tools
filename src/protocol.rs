use std::{
    borrow::Cow,
    fmt::{self, Display, Formatter},
};

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub enum EngineMsg {
    Id { name: Option<String>, author: Option<String>, version: Option<String> },
    Ready(u32),
    Info { pv: Vec<Mv>, eval: i32, depth: u32, nodes: u64, time: u64 },
    Best(Mv),
    Error(Cow<'static, str>),
}

#[derive(Debug, Serialize, Deserialize)]
pub enum UserMsg {
    New,
    Sync(u32),
    State { undo: usize, play: Vec<Mv> },
    Go { node: Limits, time: Limits },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(try_from = "&str", into = "String")]
pub struct Mv(u8);

impl Mv {
    pub fn raw(self) -> u8 {
        self.0
    }
}

impl TryFrom<&str> for Mv {
    type Error = ParseMvError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let [c, r] = *value.as_bytes() else { return Err(ParseMvError::BadLen) };
        let c @ ..8 = c.wrapping_sub(b'a') else { return Err(ParseMvError::BadCol) };
        let r @ ..8 = r.wrapping_sub(b'1') else { return Err(ParseMvError::BadRow) };
        Ok(Mv(c | r << 3))
    }
}

impl From<Mv> for String {
    fn from(Mv(value): Mv) -> Self {
        let c = value & 7;
        let r = value >> 3;
        String::from_iter([c as char, r as char])
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

#[derive(Debug, Serialize, Deserialize)]
pub struct Limits {
    pub fixed: u64,
}

pub fn send(msg: &EngineMsg) {
    println!("{}", ron::to_string(msg).unwrap());
}

pub fn send_error(error: impl Into<Cow<'static, str>>) {
    send(&EngineMsg::Error(error.into()));
}
