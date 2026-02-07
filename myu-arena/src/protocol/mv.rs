use std::ops::Deref;

use serde::{Deserialize, Serialize};

/// A protocol move, equivalent to a square on the board.
/// Serializes to/from a string representation (e.g., "a1").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "&str", into = "String")]
pub struct Mv(myu_core::Sq);

impl Mv {
    pub fn from_raw(raw: u8) -> Option<Self> {
        myu_core::Sq::from_raw(raw).map(Self)
    }
}

impl Deref for Mv {
    type Target = myu_core::Sq;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<myu_core::Sq> for Mv {
    fn from(sq: myu_core::Sq) -> Self {
        Self(sq)
    }
}

impl From<Mv> for myu_core::Sq {
    fn from(value: Mv) -> Self {
        value.0
    }
}

impl TryFrom<&str> for Mv {
    type Error = myu_core::ParseSqError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        myu_core::parse_sq(value).map(Self)
    }
}

impl From<Mv> for String {
    fn from(value: Mv) -> Self {
        myu_core::format_sq(value.0)
    }
}
