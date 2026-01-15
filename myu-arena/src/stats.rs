//! Match statistics tracking.

use crate::match_runner::{FaultyEngine, GamePairResult};

/// Aggregate statistics for the match
#[derive(Debug, Default)]
pub struct MatchStats {
    /// Pentanomial counts: [LL, LD, DD/WL, WD, WW]
    pub pentanomial: [u64; 5],

    /// Dev wins (individual games)
    pub dev_wins: u64,
    /// Dev losses (individual games)
    pub dev_losses: u64,
    /// Draws (individual games)
    pub draws: u64,

    /// Error counts for dev
    pub dev_crashes: u64,
    pub dev_timeouts: u64,
    pub dev_illegal_moves: u64,
    pub dev_infinite_loops: u64,

    /// Error counts for base
    pub base_crashes: u64,
    pub base_timeouts: u64,
    pub base_illegal_moves: u64,
    pub base_infinite_loops: u64,
}

impl MatchStats {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a game pair result
    pub fn record_pair(&mut self, pair: &GamePairResult) {
        // Update pentanomial
        self.pentanomial[pair.outcome.index()] += 1;

        // Update individual game stats
        for game_result in [&pair.game1, &pair.game2] {
            if game_result.dev_score > 0.75 {
                self.dev_wins += 1;
            } else if game_result.dev_score < 0.25 {
                self.dev_losses += 1;
            } else {
                self.draws += 1;
            }

            // Record errors
            if let Some(ref reason) = game_result.termination_reason {
                let is_dev = game_result.faulty_engine == Some(FaultyEngine::Dev);
                let reason_lower = reason.to_lowercase();

                if reason_lower.contains("crash") || reason_lower.contains("spawn") {
                    if is_dev {
                        self.dev_crashes += 1;
                    } else {
                        self.base_crashes += 1;
                    }
                } else if reason_lower.contains("timeout") {
                    if is_dev {
                        self.dev_timeouts += 1;
                    } else {
                        self.base_timeouts += 1;
                    }
                } else if reason_lower.contains("illegal") {
                    if is_dev {
                        self.dev_illegal_moves += 1;
                    } else {
                        self.base_illegal_moves += 1;
                    }
                } else if reason_lower.contains("infinite") || reason_lower.contains("loop") {
                    if is_dev {
                        self.dev_infinite_loops += 1;
                    } else {
                        self.base_infinite_loops += 1;
                    }
                }
            }
        }
    }

    /// Total games played
    pub fn total_games(&self) -> u64 {
        self.dev_wins + self.dev_losses + self.draws
    }

    /// Total game pairs played
    pub fn total_pairs(&self) -> u64 {
        self.pentanomial.iter().sum()
    }

    /// Dev score percentage
    pub fn dev_score_pct(&self) -> f64 {
        let total = self.total_games();
        if total == 0 {
            return 50.0;
        }
        100.0 * (self.dev_wins as f64 + 0.5 * self.draws as f64) / total as f64
    }

    /// Total errors for dev
    pub fn dev_total_errors(&self) -> u64 {
        self.dev_crashes + self.dev_timeouts + self.dev_illegal_moves + self.dev_infinite_loops
    }

    /// Total errors for base
    pub fn base_total_errors(&self) -> u64 {
        self.base_crashes + self.base_timeouts + self.base_illegal_moves + self.base_infinite_loops
    }
}
