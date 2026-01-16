//! Match runner for paired games with pentanomial scoring.

use std::{
    fmt,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
};

use myu_core::{Color, Game, Mv, Outcome};
use myu_protocol::Sq;

use crate::{
    engine::{Engine, MoveResult},
    opening_book::OpeningBook,
};

/// Pentanomial outcome for a game pair
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pentanomial {
    Ll,   // Both games lost by dev (0 points)
    Ld,   // One loss, one draw (0.5 points)
    DdWl, // Two draws, or one win one loss (1 point)
    Wd,   // One win, one draw (1.5 points)
    Ww,   // Both games won by dev (2 points)
}

impl Pentanomial {
    /// Create from pair of dev scores (each 0.0, 0.5, or 1.0)
    pub fn from_scores(score1: f64, score2: f64) -> Self {
        match score1 + score2 {
            0.0 => Self::Ll,
            0.5 => Self::Ld,
            1.0 => Self::DdWl,
            1.5 => Self::Wd,
            2.0 => Self::Ww,
            _ => unreachable!(),
        }
    }

    pub const fn index(self) -> usize {
        match self {
            Self::Ll => 0,
            Self::Ld => 1,
            Self::DdWl => 2,
            Self::Wd => 3,
            Self::Ww => 4,
        }
    }
}

impl fmt::Display for Pentanomial {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad(match self {
            Self::Ll => "LL",
            Self::Ld => "LD",
            Self::DdWl => "DD/WL",
            Self::Wd => "WD",
            Self::Ww => "WW",
        })
    }
}

/// Which engine is at fault
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaultyEngine {
    Dev,
    Base,
}

/// Result of a single game
#[derive(Debug, Clone)]
pub struct GameResult {
    pub game: Game,
    /// Score from dev's perspective (1.0 = win, 0.5 = draw, 0.0 = loss)
    pub dev_score: f64,
    /// Termination reason if abnormal
    pub termination_reason: Option<String>,
    /// Which engine caused abnormal termination, if any
    pub faulty_engine: Option<FaultyEngine>,
}

/// Result of a game pair
#[derive(Debug)]
pub struct GamePairResult {
    pub game1: GameResult, // dev plays White
    pub game2: GameResult, // dev plays Black
    pub outcome: Pentanomial,
}

/// Configuration for running games
#[derive(Clone)]
pub struct GameConfig {
    pub dev_path: PathBuf,
    pub base_path: PathBuf,
    pub time_ms: u64,
    pub timeout_leniency: f64,
    pub logs_dir: Option<PathBuf>,
}

/// Match runner that manages concurrent game pairs
pub struct MatchRunner {
    config: GameConfig,
    opening_book: OpeningBook,
    concurrency: usize,
    stop_flag: Arc<AtomicBool>,
}

impl MatchRunner {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        dev_path: PathBuf,
        base_path: PathBuf,
        time_ms: u64,
        timeout_leniency: f64,
        logs_dir: Option<PathBuf>,
        opening_book: OpeningBook,
        concurrency: usize,
        stop_flag: Arc<AtomicBool>,
    ) -> Self {
        Self {
            config: GameConfig { dev_path, base_path, time_ms, timeout_leniency, logs_dir },
            opening_book,
            concurrency,
            stop_flag,
        }
    }

    /// Run game pairs, yielding results as they complete
    pub fn run_pairs(self, max_pairs: usize) -> impl Iterator<Item = GamePairResult> {
        let (result_tx, result_rx) = mpsc::channel();
        let (task_tx, task_rx) = mpsc::channel::<Vec<Mv>>();
        let task_rx = Arc::new(Mutex::new(task_rx));

        // Spawn worker threads
        for _ in 0..self.concurrency {
            let config = GameConfig {
                dev_path: self.config.dev_path.clone(),
                base_path: self.config.base_path.clone(),
                time_ms: self.config.time_ms,
                timeout_leniency: self.config.timeout_leniency,
                logs_dir: self.config.logs_dir.clone(),
            };
            let result_tx = result_tx.clone();
            let task_rx = task_rx.clone();
            let stop_flag = self.stop_flag.clone();

            thread::spawn(move || {
                while !stop_flag.load(Ordering::SeqCst) {
                    let opening = match task_rx.lock().unwrap().recv() {
                        Ok(o) => o,
                        Err(_) => break,
                    };
                    let result = play_game_pair(&config, &opening, &stop_flag);
                    if result_tx.send(result).is_err() {
                        break;
                    }
                }
            });
        }

        // Send tasks in separate thread
        let opening_book = self.opening_book;
        let stop_flag = self.stop_flag.clone();
        thread::spawn(move || {
            for pair_idx in 0..max_pairs {
                if stop_flag.load(Ordering::SeqCst) {
                    break;
                }
                if task_tx.send(opening_book.get_opening(pair_idx)).is_err() {
                    break;
                }
            }
        });

        // Return iterator
        GamePairIterator { result_rx, received: 0, max_pairs, stop_flag: self.stop_flag }
    }
}

struct GamePairIterator {
    result_rx: mpsc::Receiver<GamePairResult>,
    received: usize,
    max_pairs: usize,
    stop_flag: Arc<AtomicBool>,
}

impl Iterator for GamePairIterator {
    type Item = GamePairResult;

    fn next(&mut self) -> Option<Self::Item> {
        if self.received >= self.max_pairs || self.stop_flag.load(Ordering::SeqCst) {
            return None;
        }
        self.result_rx.recv().ok().inspect(|_| self.received += 1)
    }
}

/// Play a single game pair (dev as White, then dev as Black)
fn play_game_pair(config: &GameConfig, opening: &[Mv], stop_flag: &Arc<AtomicBool>) -> GamePairResult {
    let game1 = play_single_game(config, opening, true, stop_flag);
    let game2 = play_single_game(config, opening, false, stop_flag);
    let outcome = Pentanomial::from_scores(game1.dev_score, game2.dev_score);
    GamePairResult { game1, game2, outcome }
}

/// Play a single game
fn play_single_game(
    config: &GameConfig,
    opening: &[Mv],
    dev_is_white: bool,
    stop_flag: &Arc<AtomicBool>,
) -> GameResult {
    let (dev_path, base_path) = (&config.dev_path, &config.base_path);
    let (white_path, black_path) = if dev_is_white { (dev_path, base_path) } else { (base_path, dev_path) };

    // Spawn engines
    let spawn = |name, path: &PathBuf, is_white| {
        Engine::spawn(name, path, config.time_ms, config.timeout_leniency, config.logs_dir.clone())
            .map_err(|e| error_result(Game::new(), dev_is_white, is_white, format!("Engine spawn failed: {e}")))
    };

    let mut white = match spawn("white", white_path, true) {
        Ok(e) => e,
        Err(r) => return r,
    };
    let mut black = match spawn("black", black_path, false) {
        Ok(e) => e,
        Err(r) => return r,
    };

    // Sync engines
    let sync = |engine: &mut Engine, is_white| {
        engine.sync().map_err(|e| {
            engine.write_log("sync_failed");
            error_result(Game::new(), dev_is_white, is_white, format!("Sync failed: {e}"))
        })
    };

    if let Err(r) = sync(&mut white, true) {
        return r;
    }
    if let Err(r) = sync(&mut black, false) {
        return r;
    }

    // Initialize game and play opening
    let mut game = Game::new();
    let opening_sqs: Vec<Sq> = opening.iter().filter_map(|mv| Sq::from_raw(mv.sq.raw())).collect();

    if !opening_sqs.is_empty() {
        let _ = white.play(opening_sqs.clone());
        let _ = black.play(opening_sqs);
        for mv in opening {
            if game.play(*mv).is_err() {
                break;
            }
        }
    }

    // Main game loop
    run_game_loop(game, white, black, config.time_ms, dev_is_white, stop_flag)
}

const MAX_MOVES: usize = 500;

fn run_game_loop(
    mut game: Game,
    mut white: Engine,
    mut black: Engine,
    time_ms: u64,
    dev_is_white: bool,
    stop_flag: &Arc<AtomicBool>,
) -> GameResult {
    for _move_count in 0..MAX_MOVES {
        if stop_flag.load(Ordering::SeqCst) {
            return GameResult {
                game,
                dev_score: 0.5,
                termination_reason: Some("Match stopped".into()),
                faulty_engine: None,
            };
        }

        if let Some(outcome) = game.outcome() {
            return GameResult {
                game,
                dev_score: dev_score(&outcome, dev_is_white),
                termination_reason: None,
                faulty_engine: None,
            };
        }

        let is_white_turn = game.current_state().side_to_move() == Color::White;
        let (current, opponent) = if is_white_turn { (&mut white, &mut black) } else { (&mut black, &mut white) };

        match current.go(time_ms) {
            MoveResult::Move(sq) => {
                let core_sq = myu_core::Sq::from_raw(sq.raw()).unwrap();
                let mv = Mv::new(core_sq);

                match game.play(mv) {
                    Ok(()) => {
                        let _ = current.play(vec![sq]);
                        let _ = opponent.play(vec![sq]);
                    }
                    Err(e) => {
                        current.write_log("illegal_move");
                        return error_result(game, dev_is_white, is_white_turn, format!("Illegal move: {e}"));
                    }
                }
            }
            MoveResult::NoMove => {
                current.write_log("no_move");
                return error_result(game, dev_is_white, is_white_turn, "Engine returned no move".into());
            }
            MoveResult::Timeout => {
                return error_result(game, dev_is_white, is_white_turn, "Timeout".into());
            }
            MoveResult::Crash => {
                return error_result(game, dev_is_white, is_white_turn, "Engine crashed".into());
            }
            MoveResult::IllegalProtocol(msg) => {
                current.write_log("protocol_error");
                return error_result(game, dev_is_white, is_white_turn, format!("Protocol error: {msg}"));
            }
            MoveResult::EngineError(_) => continue,
        }
    }

    // Exceeded max moves
    let is_white_turn = game.current_state().side_to_move() == Color::White;
    let engine = if is_white_turn { &mut white } else { &mut black };
    engine.write_log("infinite_loop");
    error_result(game, dev_is_white, is_white_turn, "Infinite loop detected".into())
}

fn error_result(game: Game, dev_is_white: bool, white_failed: bool, reason: String) -> GameResult {
    let dev_failed = dev_is_white == white_failed;
    GameResult {
        game,
        dev_score: if dev_failed { 0.0 } else { 1.0 },
        termination_reason: Some(reason),
        faulty_engine: Some(if dev_failed { FaultyEngine::Dev } else { FaultyEngine::Base }),
    }
}

fn dev_score(outcome: &Outcome, dev_is_white: bool) -> f64 {
    match outcome.winner() {
        Some(winner) => {
            if (winner == Color::White) == dev_is_white {
                1.0
            } else {
                0.0
            }
        }
        None => 0.5,
    }
}
