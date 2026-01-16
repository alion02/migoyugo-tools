//! Match statistics tracking.

use std::fmt;

use crate::match_runner::{FaultyEngine, GamePairResult, Pentanomial, Score, TerminationKind};

/// Pentanomial counts: [LL, LD, DD/WL, WD, WW]
#[derive(Debug, Default, Clone, Copy)]
pub struct PentanomialCounts(pub [u64; 5]);

impl PentanomialCounts {
    pub fn record(&mut self, outcome: Pentanomial) {
        self.0[outcome.index()] += 1;
    }
}

impl fmt::Display for PentanomialCounts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "LL={} LD={} DD/WL={} WD={} WW={}", self.0[0], self.0[1], self.0[2], self.0[3], self.0[4])
    }
}

impl std::ops::Deref for PentanomialCounts {
    type Target = [u64; 5];
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Aggregate statistics for the match
#[derive(Debug, Default)]
pub struct MatchStats {
    pub pentanomial: PentanomialCounts,
    pub dev_wins: u64,
    pub dev_losses: u64,
    pub draws: u64,
    pub dev_crashes: u64,
    pub dev_timeouts: u64,
    pub dev_illegal_moves: u64,
    pub dev_infinite_loops: u64,
    pub base_crashes: u64,
    pub base_timeouts: u64,
    pub base_illegal_moves: u64,
    pub base_infinite_loops: u64,
}

impl MatchStats {
    /// Record a game pair result
    pub fn record_pair(&mut self, pair: &GamePairResult) {
        self.pentanomial.record(pair.outcome);

        for result in [&pair.game1, &pair.game2] {
            // Record win/loss/draw
            match result.dev_score {
                Score::Loss => self.dev_losses += 1,
                Score::Draw => self.draws += 1,
                Score::Win => self.dev_wins += 1,
            }

            // Record errors
            if let Some(ref termination) = result.termination {
                let is_dev = termination.faulty_engine == Some(FaultyEngine::Dev);

                let inc = |dev: &mut u64, base: &mut u64| {
                    *(if is_dev { dev } else { base }) += 1;
                };

                match termination.kind {
                    TerminationKind::Crash | TerminationKind::SpawnFailed | TerminationKind::SyncFailed => {
                        inc(&mut self.dev_crashes, &mut self.base_crashes);
                    }
                    TerminationKind::Timeout => {
                        inc(&mut self.dev_timeouts, &mut self.base_timeouts);
                    }
                    TerminationKind::IllegalMove | TerminationKind::ProtocolError | TerminationKind::NoMove => {
                        inc(&mut self.dev_illegal_moves, &mut self.base_illegal_moves);
                    }
                    TerminationKind::InfiniteLoop => {
                        inc(&mut self.dev_infinite_loops, &mut self.base_infinite_loops);
                    }
                    TerminationKind::Stopped => {
                        // Not a fault, don't record
                    }
                }
            }
        }
    }

    pub fn total_games(&self) -> u64 {
        self.dev_wins + self.dev_losses + self.draws
    }

    pub fn dev_score_pct(&self) -> f64 {
        let total = self.total_games();
        if total == 0 { 50.0 } else { 100.0 * (self.dev_wins as f64 + 0.5 * self.draws as f64) / total as f64 }
    }
}
