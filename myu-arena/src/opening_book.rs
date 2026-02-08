//! Opening book handling for the match runner.

use std::{
    fs::File,
    io::{BufRead, BufReader},
    path::Path,
};

use anyhow::{Context, Result};
use myu_core::{Mv, State, parse_sq};

/// Opening book: a collection of opening move sequences
pub struct OpeningBook {
    openings: Vec<Vec<Mv>>,
}

impl OpeningBook {
    /// Create an empty opening book (games start from initial position)
    pub fn empty() -> Self {
        Self { openings: vec![vec![]] }
    }

    /// Load opening book from file
    /// Format: one opening per line, space-separated move sequence (e.g., "d4 d5 c4 e6")
    pub fn from_file(path: &Path) -> Result<Self> {
        let file = File::open(path).with_context(|| format!("Failed to open {}", path.display()))?;
        let reader = BufReader::new(file);
        let mut openings = Vec::new();

        for (line_num, line) in reader.lines().enumerate() {
            let line = line.with_context(|| format!("Read error at line {}", line_num + 1))?;
            let line = line.trim();

            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let mut moves = Vec::new();
            for (move_idx, move_str) in line.split_whitespace().enumerate() {
                let sq = parse_sq(move_str).map_err(|e| {
                    anyhow::anyhow!(
                        "Invalid move '{}' at line {} position {}: {e}",
                        move_str,
                        line_num + 1,
                        move_idx + 1
                    )
                })?;
                moves.push(Mv::new(sq));
            }
            openings.push(moves);
        }

        if openings.is_empty() {
            openings.push(vec![]); // At least one empty opening
        }

        let mut book = Self { openings };
        book.validate()?;
        Ok(book)
    }

    /// Generate a random opening book with n random openings of the given depth
    pub fn generate_random(count: usize, depth: usize) -> Self {
        use rand::prelude::*;

        let mut rng = rand::rng();
        let mut openings = Vec::with_capacity(count);

        for _ in 0..count {
            let mut state = State::new();
            let mut moves = Vec::new();

            for _ in 0..depth {
                let legal: Vec<Mv> = state.legal_moves().collect();
                if legal.is_empty() {
                    break;
                }
                let mv = *legal.choose(&mut rng).unwrap();
                moves.push(mv);
                (state, _) = state.play_unchecked(mv);
            }

            openings.push(moves);
        }

        if openings.is_empty() {
            openings.push(vec![]);
        }

        Self { openings }
    }

    /// Validate all openings by replaying them from the initial position
    fn validate(&mut self) -> Result<()> {
        let mut invalid_indices = Vec::new();

        for (idx, opening) in self.openings.iter().enumerate() {
            if let Err(e) = Self::validate_opening(opening) {
                eprintln!("Warning: opening {} is invalid: {}", idx + 1, e);
                invalid_indices.push(idx);
            }
        }

        // Remove invalid openings in reverse order to preserve indices
        for idx in invalid_indices.into_iter().rev() {
            self.openings.remove(idx);
        }

        // Ensure we still have at least one opening
        if self.openings.is_empty() {
            return Err(anyhow::anyhow!("No valid openings in book after validation"));
        }

        Ok(())
    }

    /// Validate a single opening by replaying it from the initial position
    fn validate_opening(opening: &[Mv]) -> Result<()> {
        let mut state = State::new();

        for (move_idx, mv) in opening.iter().enumerate() {
            match state.play(*mv) {
                Ok((new_state, _)) => state = new_state,
                Err(e) => {
                    return Err(anyhow::anyhow!(
                        "move {} ({}): {}",
                        move_idx + 1,
                        myu_core::format_sq(mv.sq),
                        e
                    ));
                }
            }
        }

        // Check if the position is still playable (not a terminal position)
        if state.legal_moves().next().is_none() {
            return Err(anyhow::anyhow!("opening leads to terminal position"));
        }

        Ok(())
    }

    /// Save opening book to file
    pub fn save_to_file(&self, path: &Path) -> Result<()> {
        use std::io::Write;

        let mut file = File::create(path).with_context(|| format!("Failed to create {}", path.display()))?;

        for opening in &self.openings {
            let line: String = opening.iter().map(|mv| myu_core::format_sq(mv.sq)).collect::<Vec<_>>().join(" ");
            writeln!(file, "{}", line).context("Write error")?;
        }

        Ok(())
    }

    /// Number of openings in the book
    pub fn len(&self) -> usize {
        self.openings.len()
    }

    /// Get an opening for a given pair index (cycles through the book)
    pub fn get_opening(&self, pair_index: usize) -> Vec<Mv> {
        self.openings[pair_index % self.openings.len()].clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_book() {
        let book = OpeningBook::empty();
        assert_eq!(book.len(), 1);
        assert!(book.get_opening(0).is_empty());
    }

    #[test]
    fn test_generate_random() {
        let book = OpeningBook::generate_random(10, 6);
        assert_eq!(book.len(), 10);
        for i in 0..10 {
            let opening = book.get_opening(i);
            assert!(opening.len() <= 6);
        }
    }

    #[test]
    fn test_cycling() {
        let book = OpeningBook::generate_random(3, 4);
        assert_eq!(book.get_opening(0), book.get_opening(3));
        assert_eq!(book.get_opening(1), book.get_opening(4));
        assert_eq!(book.get_opening(2), book.get_opening(5));
    }

    #[test]
    fn test_validation() {
        // Generated openings should always be valid
        let book = OpeningBook::generate_random(10, 6);
        for i in 0..10 {
            let opening = book.get_opening(i);
            assert!(OpeningBook::validate_opening(&opening).is_ok());
        }
    }
}
