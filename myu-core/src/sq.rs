/// A square on the 8×8 board.
///
/// Internal representation: `col | (row << 3)` where col and row are 0-7.
/// - Column 0 = 'a', column 7 = 'h'
/// - Row 0 = '1', row 7 = '8'
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Sq(u8);

impl Sq {
    /// Create from raw index (0-63). Returns `None` if out of range.
    #[inline]
    pub fn from_raw(raw: u8) -> Option<Self> {
        (raw < 64).then_some(Self(raw))
    }

    /// Create from column (0-7) and row (0-7). Returns `None` if out of range.
    #[inline]
    pub fn from_col_row(col: u8, row: u8) -> Option<Self> {
        (col < 8 && row < 8).then_some(Self(col | (row << 3)))
    }

    /// Raw index (0-63).
    #[inline]
    pub fn raw(self) -> u8 {
        self.0
    }

    /// Column index (0-7).
    #[inline]
    pub fn col(self) -> u8 {
        self.0 & 7
    }

    /// Row index (0-7).
    #[inline]
    pub fn row(self) -> u8 {
        self.0 >> 3
    }

    /// Bitboard mask for this square.
    #[inline]
    pub fn bit(self) -> u64 {
        1 << self.0
    }

    /// All 64 squares.
    pub fn all() -> impl Iterator<Item = Self> {
        (0..64).map(Self)
    }
}

use crate::ParseSqError;

/// Parse algebraic notation (e.g., "a1", "h8").
pub fn parse_sq(s: &str) -> Result<Sq, ParseSqError> {
    let bytes = s.as_bytes();
    if bytes.len() != 2 {
        return Err(ParseSqError::BadLen);
    }
    let col = bytes[0].wrapping_sub(b'a');
    if col >= 8 {
        return Err(ParseSqError::BadCol);
    }
    let row = bytes[1].wrapping_sub(b'1');
    if row >= 8 {
        return Err(ParseSqError::BadRow);
    }
    Ok(Sq::from_col_row(col, row).unwrap())
}

/// Format to algebraic notation (e.g., "a1", "h8").
pub fn format_sq(sq: Sq) -> String {
    let col = b'a' + sq.col();
    let row = b'1' + sq.row();
    String::from_iter([col as char, row as char])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sq_construction() {
        let sq = Sq::from_col_row(0, 0).unwrap();
        assert_eq!(sq.raw(), 0);
        assert_eq!(sq.col(), 0);
        assert_eq!(sq.row(), 0);

        let sq = Sq::from_col_row(7, 7).unwrap();
        assert_eq!(sq.raw(), 63);
        assert_eq!(sq.col(), 7);
        assert_eq!(sq.row(), 7);

        let sq = Sq::from_col_row(3, 4).unwrap();
        assert_eq!(sq.col(), 3);
        assert_eq!(sq.row(), 4);
    }

    #[test]
    fn test_sq_out_of_range() {
        assert!(Sq::from_raw(64).is_none());
        assert!(Sq::from_col_row(8, 0).is_none());
        assert!(Sq::from_col_row(0, 8).is_none());
    }

    #[test]
    fn test_algebraic_parse() {
        assert_eq!(parse_sq("a1").unwrap(), Sq::from_col_row(0, 0).unwrap());
        assert_eq!(parse_sq("h8").unwrap(), Sq::from_col_row(7, 7).unwrap());
        assert_eq!(parse_sq("d4").unwrap(), Sq::from_col_row(3, 3).unwrap());
        assert_eq!(parse_sq("e5").unwrap(), Sq::from_col_row(4, 4).unwrap());
    }

    #[test]
    fn test_algebraic_format() {
        assert_eq!(format_sq(Sq::from_col_row(0, 0).unwrap()), "a1");
        assert_eq!(format_sq(Sq::from_col_row(7, 7).unwrap()), "h8");
        assert_eq!(format_sq(Sq::from_col_row(3, 3).unwrap()), "d4");
    }

    #[test]
    fn test_algebraic_roundtrip() {
        for sq in Sq::all() {
            let s = format_sq(sq);
            assert_eq!(parse_sq(&s).unwrap(), sq);
        }
    }

    #[test]
    fn test_parse_errors() {
        assert!(matches!(parse_sq("a"), Err(ParseSqError::BadLen)));
        assert!(matches!(parse_sq("a1b"), Err(ParseSqError::BadLen)));
        assert!(matches!(parse_sq("i1"), Err(ParseSqError::BadCol)));
        assert!(matches!(parse_sq("a0"), Err(ParseSqError::BadRow)));
        assert!(matches!(parse_sq("a9"), Err(ParseSqError::BadRow)));
    }
}
