use crate::{Color, Mv, MvFormat, Outcome, ParseGameError, PlayError, State, WegoResult, format_mv, parse_mv_auto};

/// A complete game: move history with derived states.
#[derive(Debug, Clone)]
pub struct Game {
    moves: Vec<Mv>,
    states: Vec<State>, // states[0] = initial, states[i+1] = after moves[i]
}

impl Default for Game {
    fn default() -> Self {
        Self::new()
    }
}

impl Game {
    /// Create an empty game.
    pub fn new() -> Self {
        Self { moves: Vec::new(), states: vec![State::new()] }
    }

    /// Create a game from a sequence of moves.
    pub fn from_moves(moves: impl IntoIterator<Item = Mv>) -> Result<Self, PlayError> {
        let mut game = Self::new();
        for mv in moves {
            game.play(mv)?;
        }
        Ok(game)
    }

    /// All moves played.
    pub fn moves(&self) -> &[Mv] {
        &self.moves
    }

    /// All states (initial + after each move).
    pub fn states(&self) -> &[State] {
        &self.states
    }

    /// Current (latest) state.
    pub fn current_state(&self) -> &State {
        self.states.last().unwrap()
    }

    /// Initial state.
    pub fn initial_state(&self) -> &State {
        &self.states[0]
    }

    /// Number of moves played.
    pub fn len(&self) -> usize {
        self.moves.len()
    }

    /// Whether no moves have been played.
    pub fn is_empty(&self) -> bool {
        self.moves.is_empty()
    }

    /// Play a move, appending to history.
    pub fn play(&mut self, mv: Mv) -> Result<(), PlayError> {
        if self.outcome().is_some() {
            return Err(PlayError::GameOver);
        }
        let (new_state, yugos) = self.current_state().play(mv)?;
        self.moves.push(Mv::with_yugos(mv.sq, yugos));
        self.states.push(new_state);
        Ok(())
    }

    /// Undo the last move.
    pub fn undo(&mut self) -> Option<Mv> {
        if self.moves.is_empty() {
            return None;
        }
        self.states.pop();
        self.moves.pop()
    }

    /// Current game outcome.
    pub fn outcome(&self) -> Option<Outcome> {
        self.current_state().outcome()
    }
}

// === PGN-like parsing/formatting ===

/// Configuration for PGN-like move list format.
#[derive(Debug, Clone, Default)]
pub struct PgnFormat {
    /// Include move numbers: "1. d4 c3 2. e4 e5"
    pub move_numbers: bool,
    /// Use newlines between move pairs
    pub newlines: bool,
    /// Format for individual moves
    pub mv_format: MvFormat,
}

/// Parse a PGN-like move list.
///
/// Automatically detects:
/// - Move numbers (optional)
/// - Move format (Plain, YugosParens, YugosPlus)
///
/// Returns the game and detected move format.
pub fn parse_game(s: &str, validate_yugos: bool) -> Result<(Game, Option<MvFormat>), ParseGameError> {
    let mut game = Game::new();
    let mut detected_format: Option<MvFormat> = None;

    // Tokenize: split on whitespace, handle move numbers
    let tokens: Vec<&str> = s.split_whitespace().collect();
    let mut i = 0;
    let mut move_idx = 0;

    while i < tokens.len() {
        let token = tokens[i];

        // Skip move numbers like "1." or "1"
        if token.ends_with('.') || token.chars().all(|c| c.is_ascii_digit()) {
            i += 1;
            continue;
        }

        // Check for result
        if matches!(token, "1-0" | "0-1" | "1/2-1/2") {
            // Skip result and any outcome type in parens
            i += 1;
            if i < tokens.len() && tokens[i].starts_with('(') {
                i += 1;
            }
            continue;
        }

        // Handle parenthesized yugo counts that span tokens
        let mv_str = if i + 2 < tokens.len() && tokens[i + 1] == "(" {
            // "d6 ( 1 yugo )" or "d6 (1 yugo)"
            // Find closing paren
            let mut end = i + 1;
            while end < tokens.len() && !tokens[end].contains(')') {
                end += 1;
            }
            let combined: String = tokens[i..=end].join(" ");
            i = end + 1;
            combined
        } else if i + 1 < tokens.len() && tokens[i + 1].starts_with('(') {
            // "d6 (1" "yugo)"
            let combined = format!("{} {}", token, tokens[i + 1]);
            i += 2;
            // Continue if next token has closing paren
            if i < tokens.len() && tokens[i - 1].contains(')') {
                combined
            } else if i < tokens.len() {
                let combined = format!("{} {}", combined, tokens[i]);
                i += 1;
                combined
            } else {
                combined
            }
        } else {
            i += 1;
            token.to_string()
        };

        // Try to parse the move
        let mv_str = mv_str.trim_end_matches(['.', ',']);
        let mv = parse_mv_auto(mv_str).map_err(|e| ParseGameError::Move { index: move_idx, source: e })?;

        // Detect format from first yugo-annotated move
        if detected_format.is_none() && mv.yugos_formed > 0 {
            detected_format = Some(if mv_str.contains('(') { MvFormat::YugosParens } else { MvFormat::YugosPlus });
        }

        // Play the move
        game.play(mv).map_err(|_| ParseGameError::Illegal(move_idx))?;

        // Validate yugo count if requested
        if validate_yugos && mv.yugos_formed > 0 {
            let actual = game.moves().last().unwrap().yugos_formed;
            if actual != mv.yugos_formed {
                return Err(ParseGameError::YugoMismatch { index: move_idx, expected: actual, got: mv.yugos_formed });
            }
        }

        move_idx += 1;
    }

    Ok((game, detected_format))
}

/// Result string for a game outcome.
pub fn format_result(outcome: Option<Outcome>, include_type: bool) -> String {
    match outcome {
        None => "*".into(),
        Some(Outcome::Igo(Color::White)) => {
            if include_type {
                "1-0 (igo)".into()
            } else {
                "1-0".into()
            }
        }
        Some(Outcome::Igo(Color::Black)) => {
            if include_type {
                "0-1 (igo)".into()
            } else {
                "0-1".into()
            }
        }
        Some(Outcome::Wego(WegoResult::Win(Color::White))) => {
            if include_type {
                "1-0 (wego)".into()
            } else {
                "1-0".into()
            }
        }
        Some(Outcome::Wego(WegoResult::Win(Color::Black))) => {
            if include_type {
                "0-1 (wego)".into()
            } else {
                "0-1".into()
            }
        }
        Some(Outcome::Wego(WegoResult::Draw)) => {
            if include_type {
                "1/2-1/2 (wego)".into()
            } else {
                "1/2-1/2".into()
            }
        }
    }
}

/// Parse a result string.
pub fn parse_result(s: &str) -> Result<Option<(Option<Color>, bool)>, ParseGameError> {
    let s = s.trim();
    if s == "*" {
        return Ok(None);
    }

    let (result_part, is_igo) = if let Some(rest) = s.strip_suffix("(igo)") {
        (rest.trim(), true)
    } else if let Some(rest) = s.strip_suffix("(wego)") {
        (rest.trim(), false)
    } else {
        (s, false) // Default to wego if no type specified
    };

    match result_part {
        "1-0" => Ok(Some((Some(Color::White), is_igo))),
        "0-1" => Ok(Some((Some(Color::Black), is_igo))),
        "1/2-1/2" => Ok(Some((None, false))),
        _ => Err(ParseGameError::BadResult(s.into())),
    }
}

/// Format a game to PGN-like string.
pub fn format_game(game: &Game, fmt: &PgnFormat, include_result: bool, include_result_type: bool) -> String {
    let mut result = String::new();
    let separator = if fmt.newlines { "\n" } else { " " };

    for (i, mv) in game.moves().iter().enumerate() {
        let move_num = i / 2 + 1;
        let is_white = i % 2 == 0;

        if fmt.move_numbers && is_white {
            if !result.is_empty() {
                result.push_str(separator);
            }
            use std::fmt::Write;
            write!(result, "{move_num}. ").unwrap();
        } else if !result.is_empty() && !result.ends_with('\n') {
            result.push(' ');
        }

        result.push_str(&format_mv(*mv, fmt.mv_format));
    }

    if include_result {
        if !result.is_empty() {
            result.push_str(separator);
        }
        result.push_str(&format_result(game.outcome(), include_result_type));
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_sq;

    #[test]
    fn test_empty_game() {
        let game = Game::new();
        assert!(game.is_empty());
        assert_eq!(game.len(), 0);
        assert!(game.outcome().is_none());
    }

    #[test]
    fn test_play_and_undo() {
        let mut game = Game::new();
        let mv = Mv::new(parse_sq("d4").unwrap());

        game.play(mv).unwrap();
        assert_eq!(game.len(), 1);
        assert_eq!(game.current_state().side_to_move(), Color::Black);

        let undone = game.undo();
        assert_eq!(undone.map(|m| m.sq), Some(mv.sq));
        assert!(game.is_empty());
    }

    #[test]
    fn test_parse_simple() {
        let pgn = "d4 c3 c4 e4";
        let (game, _) = parse_game(pgn, false).unwrap();
        assert_eq!(game.len(), 4);
    }

    #[test]
    fn test_parse_with_numbers() {
        let pgn = "1. d4 c3 2. c4 e4 3. d5 e3";
        let (game, _) = parse_game(pgn, false).unwrap();
        assert_eq!(game.len(), 6);
    }

    #[test]
    fn test_parse_with_yugos() {
        let pgn = "1. d4 c3 2. c4 e4 3. d5 e3 4. d3 e6 5. d6 (1 yugo) e5 (1 yugo)";
        let (game, fmt) = parse_game(pgn, true).unwrap();
        assert_eq!(game.len(), 10);
        assert_eq!(fmt, Some(MvFormat::YugosParens));
    }

    #[test]
    fn test_format_plain() {
        let mut game = Game::new();
        for sq in ["d4", "c3", "c4", "e4"] {
            game.play(Mv::new(parse_sq(sq).unwrap())).unwrap();
        }

        let fmt = PgnFormat { move_numbers: false, newlines: false, mv_format: MvFormat::Plain };
        assert_eq!(format_game(&game, &fmt, false, false), "d4 c3 c4 e4");
    }

    #[test]
    fn test_format_with_numbers() {
        let mut game = Game::new();
        for sq in ["d4", "c3", "c4", "e4"] {
            game.play(Mv::new(parse_sq(sq).unwrap())).unwrap();
        }

        let fmt = PgnFormat { move_numbers: true, newlines: false, mv_format: MvFormat::Plain };
        assert_eq!(format_game(&game, &fmt, false, false), "1. d4 c3 2. c4 e4");
    }
}
