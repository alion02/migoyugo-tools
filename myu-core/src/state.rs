use crate::{Color, Mv, Outcome, ParseStateError, Piece, PieceKind, PlayError, Sq, WegoResult};

/// Complete board state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct State {
    white_migos: u64,
    black_migos: u64,
    white_yugos: u64,
    black_yugos: u64,
    white_score: u8,
    black_score: u8,
    side_to_move: Color,
}

impl Default for State {
    fn default() -> Self {
        Self::new()
    }
}

impl State {
    /// Create an empty board with white to move.
    pub fn new() -> Self {
        Self {
            white_migos: 0,
            black_migos: 0,
            white_yugos: 0,
            black_yugos: 0,
            white_score: 0,
            black_score: 0,
            side_to_move: Color::White,
        }
    }

    /// Side to move.
    #[inline]
    pub fn side_to_move(&self) -> Color {
        self.side_to_move
    }

    /// Score for a player.
    #[inline]
    pub fn score(&self, color: Color) -> u8 {
        match color {
            Color::White => self.white_score,
            Color::Black => self.black_score,
        }
    }

    /// All occupied squares.
    #[inline]
    pub fn occupied(&self) -> u64 {
        self.white_migos | self.black_migos | self.white_yugos | self.black_yugos
    }

    /// Migos for a player.
    #[inline]
    pub fn migos(&self, color: Color) -> u64 {
        match color {
            Color::White => self.white_migos,
            Color::Black => self.black_migos,
        }
    }

    /// Yugos for a player.
    #[inline]
    pub fn yugos(&self, color: Color) -> u64 {
        match color {
            Color::White => self.white_yugos,
            Color::Black => self.black_yugos,
        }
    }

    /// All pieces for a player.
    #[inline]
    pub fn all_pieces(&self, color: Color) -> u64 {
        self.migos(color) | self.yugos(color)
    }

    /// Check if a square is empty.
    #[inline]
    pub fn is_empty(&self, sq: Sq) -> bool {
        self.occupied() & sq.bit() == 0
    }

    /// Get the piece at a square.
    pub fn at(&self, sq: Sq) -> Option<Piece> {
        let bit = sq.bit();
        if self.white_migos & bit != 0 {
            Some(Piece::WHITE_MIGO)
        } else if self.black_migos & bit != 0 {
            Some(Piece::BLACK_MIGO)
        } else if self.white_yugos & bit != 0 {
            Some(Piece::WHITE_YUGO)
        } else if self.black_yugos & bit != 0 {
            Some(Piece::BLACK_YUGO)
        } else {
            None
        }
    }

    /// Check if placing at a square is legal (ignoring Igo check).
    pub fn is_legal_placement(&self, sq: Sq) -> bool {
        if !self.is_empty(sq) {
            return false;
        }
        !self.would_create_line_over_4(sq, self.side_to_move)
    }

    /// Get all legal moves.
    pub fn legal_moves(&self) -> impl Iterator<Item = Mv> + '_ {
        Sq::all().filter(|&sq| self.is_legal_placement(sq)).map(Mv::new)
    }

    /// Check if there are any legal moves.
    pub fn has_legal_moves(&self) -> bool {
        Sq::all().any(|sq| self.is_legal_placement(sq))
    }

    /// Play a move, returning the new state and number of yugos formed.
    pub fn play(&self, mv: Mv) -> Result<(State, u8), PlayError> {
        if !self.is_empty(mv.sq) {
            return Err(PlayError::Occupied);
        }
        if self.would_create_line_over_4(mv.sq, self.side_to_move) {
            return Err(PlayError::LineTooLong);
        }
        Ok(self.play_unchecked(mv))
    }

    /// Play a move without validation. Returns new state and yugos formed.
    pub fn play_unchecked(&self, mv: Mv) -> (State, u8) {
        let mut new_state = self.clone();
        let color = self.side_to_move;
        let bit = mv.sq.bit();

        // Place the migo
        match color {
            Color::White => new_state.white_migos |= bit,
            Color::Black => new_state.black_migos |= bit,
        }

        // Check for completed lines of 4
        let mut lines = [[Sq::A1; 4]; 4];
        let yugos_formed = new_state.find_completed_lines(mv.sq, color, &mut lines);

        if yugos_formed > 0 {
            // The placed piece becomes a yugo
            match color {
                Color::White => {
                    new_state.white_migos &= !bit;
                    new_state.white_yugos |= bit;
                }
                Color::Black => {
                    new_state.black_migos &= !bit;
                    new_state.black_yugos |= bit;
                }
            }

            // Remove all migos in completed lines (but not yugos)
            let mut to_remove = 0u64;
            for line in &lines[..yugos_formed as usize] {
                for &sq in line {
                    to_remove |= sq.bit();
                }
            }
            // Don't remove yugos
            to_remove &= !new_state.white_yugos;
            to_remove &= !new_state.black_yugos;

            match color {
                Color::White => {
                    new_state.white_migos &= !to_remove;
                    new_state.white_score += yugos_formed;
                }
                Color::Black => {
                    new_state.black_migos &= !to_remove;
                    new_state.black_score += yugos_formed;
                }
            }
        }

        new_state.side_to_move = color.flip();
        (new_state, yugos_formed)
    }

    /// Compute the game outcome.
    pub fn outcome(&self) -> Option<Outcome> {
        // Check for Igo (4 yugos in a line)
        for color in [Color::White, Color::Black] {
            if self.has_yugo_line_of_4(color) {
                return Some(Outcome::Igo(color));
            }
        }

        // Check for Wego (no legal moves)
        if !self.has_legal_moves() {
            let ws = self.white_score;
            let bs = self.black_score;
            return Some(Outcome::Wego(if ws > bs {
                WegoResult::Win(Color::White)
            } else if bs > ws {
                WegoResult::Win(Color::Black)
            } else {
                WegoResult::Draw
            }));
        }

        None
    }

    // === Private helpers ===

    fn would_create_line_over_4(&self, sq: Sq, color: Color) -> bool {
        let pieces = self.all_pieces(color) | sq.bit();

        // Check all 4 directions
        for (dc, dr) in [(1, 0), (0, 1), (1, 1), (1, -1)] {
            let count = 1 + self.count_in_direction(sq, dc, dr, pieces) + self.count_in_direction(sq, -dc, -dr, pieces);
            if count > 4 {
                return true;
            }
        }
        false
    }

    fn count_in_direction(&self, sq: Sq, dc: i8, dr: i8, pieces: u64) -> u8 {
        let mut count = 0;
        let mut c = sq.col() as i8 + dc;
        let mut r = sq.row() as i8 + dr;

        while (0..8).contains(&c) && (0..8).contains(&r) {
            let check_sq = Sq::from_col_row(c as u8, r as u8).unwrap();
            if pieces & check_sq.bit() == 0 {
                break;
            }
            count += 1;
            c += dc;
            r += dr;
        }
        count
    }

    fn find_completed_lines(&self, sq: Sq, color: Color, lines: &mut [[Sq; 4]; 4]) -> u8 {
        let pieces = self.all_pieces(color);
        let mut count: u8 = 0;

        for (dc, dr) in [(1i8, 0i8), (0, 1), (1, 1), (1, -1)] {
            let behind = self.count_in_direction(sq, -dc, -dr, pieces);
            let ahead = self.count_in_direction(sq, dc, dr, pieces);
            let total = 1 + behind + ahead;

            if total == 4 {
                // Exactly 4 in a line - collect the squares
                let start_c = sq.col() as i8 - dc * behind as i8;
                let start_r = sq.row() as i8 - dr * behind as i8;

                lines[count as usize] = std::array::from_fn(|i| {
                    let c = (start_c + dc * i as i8) as u8;
                    let r = (start_r + dr * i as i8) as u8;
                    Sq::from_col_row(c, r).unwrap()
                });
                count += 1;
            }
        }
        count
    }

    fn has_yugo_line_of_4(&self, color: Color) -> bool {
        let yugos = self.yugos(color);
        if yugos.count_ones() < 4 {
            return false;
        }

        // Check each yugo as potential start of a line
        for sq in Sq::all() {
            if yugos & sq.bit() == 0 {
                continue;
            }

            for (dc, dr) in [(1i8, 0i8), (0, 1), (1, 1), (1, -1)] {
                let behind = self.count_in_direction(sq, -dc, -dr, yugos);
                let ahead = self.count_in_direction(sq, dc, dr, yugos);
                if 1 + behind + ahead >= 4 {
                    return true;
                }
            }
        }
        false
    }
}

// === FEN-like parsing/formatting ===

/// Parse a FEN-like state string.
///
/// Format: `<board> <side> <white_score> <black_score>`
/// - Board: rows separated by `/`, top to bottom (row 8 first)
/// - Pieces: `w` = white migo, `W` = white yugo, `b` = black migo, `B` = black yugo
/// - Empty squares: digit 1-8 for run length
/// - Side: `w` for white, `b` for black
pub fn parse_state(s: &str) -> Result<State, ParseStateError> {
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.len() != 4 {
        return Err(ParseStateError::BadFormat);
    }

    let board = parts[0];
    let side = parts[1];
    let white_score = parts[2];
    let black_score = parts[3];

    let mut state = State::new();

    // Parse board
    let rows: Vec<&str> = board.split('/').collect();
    if rows.len() != 8 {
        return Err(ParseStateError::Board("expected 8 rows".into()));
    }

    for (row_idx, row) in rows.iter().enumerate() {
        let rank = 7 - row_idx as u8; // Top to bottom means row 8 first
        let mut col = 0u8;

        for ch in row.chars() {
            if col >= 8 {
                return Err(ParseStateError::Board(format!("row {} too long", row_idx + 1)));
            }

            match ch {
                '1'..='8' => {
                    col += ch as u8 - b'0';
                }
                'w' => {
                    let sq = Sq::from_col_row(col, rank).unwrap();
                    state.white_migos |= sq.bit();
                    col += 1;
                }
                'W' => {
                    let sq = Sq::from_col_row(col, rank).unwrap();
                    state.white_yugos |= sq.bit();
                    col += 1;
                }
                'b' => {
                    let sq = Sq::from_col_row(col, rank).unwrap();
                    state.black_migos |= sq.bit();
                    col += 1;
                }
                'B' => {
                    let sq = Sq::from_col_row(col, rank).unwrap();
                    state.black_yugos |= sq.bit();
                    col += 1;
                }
                _ => return Err(ParseStateError::Board(format!("invalid char: {ch}"))),
            }
        }

        if col != 8 {
            return Err(ParseStateError::Board(format!("row {} incomplete: only {col} cols", row_idx + 1)));
        }
    }

    // Parse side to move
    state.side_to_move = match side {
        "w" => Color::White,
        "b" => Color::Black,
        _ => return Err(ParseStateError::BadSide),
    };

    // Parse scores
    state.white_score = white_score.parse().map_err(|_| ParseStateError::BadScore(white_score.into()))?;
    state.black_score = black_score.parse().map_err(|_| ParseStateError::BadScore(black_score.into()))?;

    Ok(state)
}

/// Format state to FEN-like string.
pub fn format_state(state: &State) -> String {
    let mut rows = Vec::with_capacity(8);

    for rank in (0..8).rev() {
        let mut row = String::new();
        let mut empty_count = 0u8;

        for col in 0..8 {
            let sq = Sq::from_col_row(col, rank).unwrap();
            if let Some(piece) = state.at(sq) {
                if empty_count > 0 {
                    row.push((b'0' + empty_count) as char);
                    empty_count = 0;
                }
                row.push(match (piece.kind, piece.color) {
                    (PieceKind::Migo, Color::White) => 'w',
                    (PieceKind::Migo, Color::Black) => 'b',
                    (PieceKind::Yugo, Color::White) => 'W',
                    (PieceKind::Yugo, Color::Black) => 'B',
                });
            } else {
                empty_count += 1;
            }
        }

        if empty_count > 0 {
            row.push((b'0' + empty_count) as char);
        }
        rows.push(row);
    }

    let board = rows.join("/");
    let side = match state.side_to_move {
        Color::White => "w",
        Color::Black => "b",
    };

    format!("{board} {side} {} {}", state.white_score, state.black_score)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_state() {
        let state = State::new();
        assert_eq!(state.side_to_move(), Color::White);
        assert_eq!(state.score(Color::White), 0);
        assert_eq!(state.score(Color::Black), 0);
        assert_eq!(state.occupied(), 0);
    }

    #[test]
    fn test_fen_empty() {
        let fen = "8/8/8/8/8/8/8/8 w 0 0";
        let state = parse_state(fen).unwrap();
        assert_eq!(state.occupied(), 0);
        assert_eq!(state.side_to_move(), Color::White);
        assert_eq!(format_state(&state), fen);
    }

    #[test]
    fn test_fen_roundtrip() {
        let fen = "8/8/8/3w4/8/8/8/8 w 0 0";
        let state = parse_state(fen).unwrap();
        assert_eq!(format_state(&state), fen);

        let fen = "8/8/8/3Wb3/8/8/8/8 b 1 2";
        let state = parse_state(fen).unwrap();
        assert_eq!(format_state(&state), fen);
    }

    #[test]
    fn test_legal_moves_empty() {
        let state = State::new();
        assert_eq!(state.legal_moves().count(), 64);
    }

    #[test]
    fn test_play_simple() {
        let state = State::new();
        let mv = Mv::new(Sq::from_col_row(3, 3).unwrap()); // d4
        let (new_state, yugos) = state.play(mv).unwrap();

        assert_eq!(yugos, 0);
        assert_eq!(new_state.side_to_move(), Color::Black);
        assert!(new_state.at(mv.sq).is_some());
    }

    #[test]
    fn test_line_too_long() {
        // Set up a position where white has pieces at a1, b1, c1 and d1 would make 4
        // But we want to test line-of-5 prevention, so we need a different setup
        // Place white pieces non-consecutively first, then try to connect them into 5
        let mut state = State::new();

        // White: a1, b1, d1, e1 (with gap at c1)
        // Black: a8, b8, d8, e8
        // This doesn't form any lines of 4 yet
        for (w_col, b_col) in [(0, 0), (1, 1), (3, 3), (4, 4)] {
            let mv = Mv::new(Sq::from_col_row(w_col, 0).unwrap());
            (state, _) = state.play_unchecked(mv);
            let mv = Mv::new(Sq::from_col_row(b_col, 7).unwrap());
            (state, _) = state.play_unchecked(mv);
        }

        // Now white tries to play c1 - would make 5 in a row (a1-e1)
        let mv = Mv::new(Sq::from_col_row(2, 0).unwrap());
        assert!(matches!(state.play(mv), Err(PlayError::LineTooLong)));
    }

    #[test]
    fn test_yugo_formation() {
        // Set up a position where white can complete a line of 4
        let mut state = State::new();

        // White: a1, b1, c1 (needs d1 for line)
        // Black: a8, b8, c8
        let moves = ["a1", "a8", "b1", "b8", "c1", "c8"];
        for mv_str in moves {
            let sq = crate::parse_sq(mv_str).unwrap();
            let mv = Mv::new(sq);
            (state, _) = state.play_unchecked(mv);
        }

        // White plays d1 - completes line
        let sq = crate::parse_sq("d1").unwrap();
        let (new_state, yugos) = state.play(Mv::new(sq)).unwrap();

        assert_eq!(yugos, 1);
        assert_eq!(new_state.score(Color::White), 1);
        // d1 should be a yugo
        let piece = new_state.at(sq).unwrap();
        assert_eq!(piece.kind, PieceKind::Yugo);
        // a1, b1, c1 should be removed (they were migos)
        assert!(new_state.at(crate::parse_sq("a1").unwrap()).is_none());
        assert!(new_state.at(crate::parse_sq("b1").unwrap()).is_none());
        assert!(new_state.at(crate::parse_sq("c1").unwrap()).is_none());
    }
}
