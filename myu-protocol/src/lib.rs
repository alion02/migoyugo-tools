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
        eval: Eval,
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
        String::from_iter([col as char, row as char])
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

pub fn to_ron(msg: &EngineMsg) -> String {
    ron::to_string(msg).unwrap()
}

pub fn from_ron(line: &str) -> Result<UserMsg, ron::de::SpannedError> {
    ron::from_str(line)
}
