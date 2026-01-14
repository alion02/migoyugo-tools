use std::{
    borrow::Cow,
    fmt::{self, Display, Formatter},
};

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
        pv: Vec<Mv>,
        eval: i32,
        depth: u32,
        nodes: u64,
        time: u64,
        knps: u64,
    },
    Best(Option<Mv>),
    Error(Cow<'static, str>),
}

#[derive(Debug, Serialize, Deserialize)]
pub enum UserMsg {
    Reset,
    Sync,
    Undo(usize),
    Play(Vec<Mv>),
    Go(Vec<Limit>),
    Stop,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(try_from = "&str", into = "String")]
pub struct Mv(u8);

impl Mv {
    pub fn new(raw: u8) -> Option<Self> {
        if raw < 64 { Some(Self(raw)) } else { None }
    }

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
        let c = (value & 7) + b'a';
        let r = (value >> 3) + b'1';
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum Limit {
    Nodes(u64),
    Ms(u64),
}

pub fn send(msg: &EngineMsg) {
    println!("{}", ron::to_string(msg).unwrap());
}

pub fn send_error(error: impl Into<Cow<'static, str>>) {
    send(&EngineMsg::Error(error.into()));
}
