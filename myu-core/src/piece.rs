/// Player color.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Color {
    White,
    Black,
}

impl Color {
    /// The other color.
    #[inline]
    pub fn flip(self) -> Self {
        match self {
            Color::White => Color::Black,
            Color::Black => Color::White,
        }
    }
}

/// Kind of piece.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PieceKind {
    Migo,
    Yugo,
}

/// A piece on the board.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Piece {
    pub kind: PieceKind,
    pub color: Color,
}

impl Piece {
    pub const WHITE_MIGO: Self = Self { kind: PieceKind::Migo, color: Color::White };
    pub const BLACK_MIGO: Self = Self { kind: PieceKind::Migo, color: Color::Black };
    pub const WHITE_YUGO: Self = Self { kind: PieceKind::Yugo, color: Color::White };
    pub const BLACK_YUGO: Self = Self { kind: PieceKind::Yugo, color: Color::Black };
}
