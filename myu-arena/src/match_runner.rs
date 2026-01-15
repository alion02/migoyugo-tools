//! Match runner for paired games with pentanomial scoring.

use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
    },
    thread::{self, JoinHandle},
};

use myu_core::{Color, Game, Mv, Outcome};
use myu_protocol::Sq;

use crate::{
    engine::{Engine, MoveResult},
    opening_book::OpeningBook,
};

/// Pentanomial outcome for a game pair
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PentanomialOutcome {
    LL,   // Both games lost by dev (0 points)
    LD,   // One loss, one draw (0.5 points)
    DDWL, // Two draws, or one win one loss (1 point)
    WD,   // One win, one draw (1.5 points)
    WW,   // Both games won by dev (2 points)
}

impl PentanomialOutcome {
    pub fn from_pair(game1_dev_score: f64, game2_dev_score: f64) -> Self {
        let total = game1_dev_score + game2_dev_score;
        if total < 0.25 {
            PentanomialOutcome::LL
        } else if total < 0.75 {
            PentanomialOutcome::LD
        } else if total < 1.25 {
            PentanomialOutcome::DDWL
        } else if total < 1.75 {
            PentanomialOutcome::WD
        } else {
            PentanomialOutcome::WW
        }
    }

    pub fn index(self) -> usize {
        match self {
            PentanomialOutcome::LL => 0,
            PentanomialOutcome::LD => 1,
            PentanomialOutcome::DDWL => 2,
            PentanomialOutcome::WD => 3,
            PentanomialOutcome::WW => 4,
        }
    }
}

/// Result of a single game
#[derive(Debug, Clone)]
pub struct GameResult {
    pub game: Game,
    pub outcome: Option<Outcome>,
    /// Score from dev's perspective (1.0 = win, 0.5 = draw, 0.0 = loss)
    pub dev_score: f64,
    /// Termination reason if abnormal
    pub termination_reason: Option<String>,
    /// Which engine caused abnormal termination, if any
    pub faulty_engine: Option<FaultyEngine>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaultyEngine {
    Dev,
    Base,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    Crash,
    Timeout,
    IllegalMove,
    InfiniteLoop,
}

/// Result of a game pair
#[derive(Debug)]
pub struct GamePairResult {
    pub game1: GameResult, // dev plays White
    pub game2: GameResult, // dev plays Black
    pub outcome: PentanomialOutcome,
    pub opening: Vec<Mv>,
}

/// Match runner that manages concurrent game pairs
pub struct MatchRunner {
    dev_path: PathBuf,
    base_path: PathBuf,
    time_ms: u64,
    timeout_leniency: f64,
    logs_dir: Option<PathBuf>,
    opening_book: OpeningBook,
    concurrency: usize,
    stop_flag: Arc<AtomicBool>,
}

impl MatchRunner {
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
        Self { dev_path, base_path, time_ms, timeout_leniency, logs_dir, opening_book, concurrency, stop_flag }
    }

    /// Run game pairs, yielding results as they complete
    pub fn run_pairs(self, max_pairs: usize) -> impl Iterator<Item = GamePairResult> {
        let (result_tx, result_rx): (Sender<GamePairResult>, Receiver<GamePairResult>) = mpsc::channel();
        let (task_tx, task_rx): (Sender<Vec<Mv>>, Receiver<Vec<Mv>>) = mpsc::channel();
        let task_rx = Arc::new(std::sync::Mutex::new(task_rx));

        // Spawn worker threads
        let mut handles: Vec<JoinHandle<()>> = Vec::new();
        for worker_id in 0..self.concurrency {
            let dev_path = self.dev_path.clone();
            let base_path = self.base_path.clone();
            let time_ms = self.time_ms;
            let timeout_leniency = self.timeout_leniency;
            let logs_dir = self.logs_dir.clone();
            let result_tx = result_tx.clone();
            let task_rx = task_rx.clone();
            let stop_flag = self.stop_flag.clone();

            let handle = thread::spawn(move || {
                loop {
                    // Check stop flag
                    if stop_flag.load(Ordering::SeqCst) {
                        break;
                    }

                    // Get next opening
                    let opening = {
                        let rx = task_rx.lock().unwrap();
                        match rx.recv() {
                            Ok(opening) => opening,
                            Err(_) => break, // Channel closed
                        }
                    };

                    // Play the game pair
                    let result = play_game_pair(
                        &dev_path,
                        &base_path,
                        time_ms,
                        timeout_leniency,
                        logs_dir.as_ref(),
                        &opening,
                        &stop_flag,
                    );

                    if result_tx.send(result).is_err() {
                        break;
                    }
                }
            });
            handles.push(handle);
        }

        // Send tasks
        let opening_book = self.opening_book;
        let stop_flag = self.stop_flag.clone();
        thread::spawn(move || {
            for pair_idx in 0..max_pairs {
                if stop_flag.load(Ordering::SeqCst) {
                    break;
                }
                let opening = opening_book.get_opening(pair_idx);
                if task_tx.send(opening).is_err() {
                    break;
                }
            }
            // Drop task_tx to signal workers to stop
        });

        // Return iterator over results
        GamePairIterator { result_rx, received: 0, max_pairs, stop_flag: self.stop_flag }
    }
}

struct GamePairIterator {
    result_rx: Receiver<GamePairResult>,
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
        match self.result_rx.recv() {
            Ok(result) => {
                self.received += 1;
                Some(result)
            }
            Err(_) => None,
        }
    }
}

/// Play a single game pair (dev as White, then dev as Black)
fn play_game_pair(
    dev_path: &PathBuf,
    base_path: &PathBuf,
    time_ms: u64,
    timeout_leniency: f64,
    logs_dir: Option<&PathBuf>,
    opening: &[Mv],
    stop_flag: &Arc<AtomicBool>,
) -> GamePairResult {
    // Game 1: dev plays White
    let game1 = play_single_game(
        dev_path,
        base_path,
        time_ms,
        timeout_leniency,
        logs_dir,
        opening,
        true, // dev is White
        stop_flag,
    );

    // Game 2: dev plays Black
    let game2 = play_single_game(
        dev_path,
        base_path,
        time_ms,
        timeout_leniency,
        logs_dir,
        opening,
        false, // dev is Black
        stop_flag,
    );

    let outcome = PentanomialOutcome::from_pair(game1.dev_score, game2.dev_score);

    GamePairResult { game1, game2, outcome, opening: opening.to_vec() }
}

/// Play a single game
fn play_single_game(
    dev_path: &PathBuf,
    base_path: &PathBuf,
    time_ms: u64,
    timeout_leniency: f64,
    logs_dir: Option<&PathBuf>,
    opening: &[Mv],
    dev_is_white: bool,
    stop_flag: &Arc<AtomicBool>,
) -> GameResult {
    // Spawn engines
    let (white_path, black_path) = if dev_is_white { (dev_path, base_path) } else { (base_path, dev_path) };

    let mut white_engine = match Engine::spawn("white", white_path, time_ms, timeout_leniency, logs_dir.cloned()) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("Failed to spawn white engine: {e}");
            return GameResult {
                game: Game::new(),
                outcome: None,
                dev_score: if dev_is_white { 0.0 } else { 1.0 },
                termination_reason: Some(format!("Engine spawn failed: {e}")),
                faulty_engine: Some(if dev_is_white { FaultyEngine::Dev } else { FaultyEngine::Base }),
            };
        }
    };

    let mut black_engine = match Engine::spawn("black", black_path, time_ms, timeout_leniency, logs_dir.cloned()) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("Failed to spawn black engine: {e}");
            return GameResult {
                game: Game::new(),
                outcome: None,
                dev_score: if dev_is_white { 1.0 } else { 0.0 },
                termination_reason: Some(format!("Engine spawn failed: {e}")),
                faulty_engine: Some(if dev_is_white { FaultyEngine::Base } else { FaultyEngine::Dev }),
            };
        }
    };

    // Initialize game
    let mut game = Game::new();

    // Sync engines
    if let Err(e) = white_engine.sync() {
        eprintln!("White engine sync failed: {e}");
        return make_error_result(game, dev_is_white, true, "sync_failed", &mut white_engine);
    }
    if let Err(e) = black_engine.sync() {
        eprintln!("Black engine sync failed: {e}");
        return make_error_result(game, dev_is_white, false, "sync_failed", &mut black_engine);
    }

    // Play opening moves
    let opening_sqs: Vec<Sq> = opening.iter().filter_map(|mv| Sq::from_raw(mv.sq.raw())).collect();

    if !opening_sqs.is_empty() {
        if let Err(e) = white_engine.play(opening_sqs.clone()) {
            eprintln!("Failed to send opening to white: {e}");
        }
        if let Err(e) = black_engine.play(opening_sqs.clone()) {
            eprintln!("Failed to send opening to black: {e}");
        }

        // Update our game state with opening
        for mv in opening {
            if game.play(*mv).is_err() {
                break;
            }
        }
    }

    // Main game loop
    let mut move_count = 0;
    let max_moves = 500; // Prevent infinite games

    loop {
        if stop_flag.load(Ordering::SeqCst) {
            return GameResult {
                game,
                outcome: None,
                dev_score: 0.5,
                termination_reason: Some("Match stopped".into()),
                faulty_engine: None,
            };
        }

        // Check for game over
        if let Some(outcome) = game.outcome() {
            let dev_score = compute_dev_score(&outcome, dev_is_white);
            return GameResult {
                game,
                outcome: Some(outcome),
                dev_score,
                termination_reason: None,
                faulty_engine: None,
            };
        }

        // Detect infinite loop
        move_count += 1;
        if move_count > max_moves {
            let is_white_turn = game.current_state().side_to_move() == Color::White;
            let is_dev_turn = (is_white_turn && dev_is_white) || (!is_white_turn && !dev_is_white);
            eprintln!("Game exceeded {} moves, adjudicating as infinite loop", max_moves);
            let engine = if is_white_turn { &mut white_engine } else { &mut black_engine };
            engine.write_log("infinite_loop");
            return GameResult {
                game,
                outcome: None,
                dev_score: if is_dev_turn { 0.0 } else { 1.0 },
                termination_reason: Some("Infinite loop detected".into()),
                faulty_engine: Some(if is_dev_turn { FaultyEngine::Dev } else { FaultyEngine::Base }),
            };
        }

        // Get current player
        let is_white_turn = game.current_state().side_to_move() == Color::White;
        let (current_engine, opponent_engine) =
            if is_white_turn { (&mut white_engine, &mut black_engine) } else { (&mut black_engine, &mut white_engine) };

        let is_dev_turn = (is_white_turn && dev_is_white) || (!is_white_turn && !dev_is_white);

        // Request move
        match current_engine.go(time_ms) {
            MoveResult::Move(sq) => {
                // Convert to core type
                let core_sq = myu_core::Sq::from_raw(sq.raw()).unwrap();
                let mv = Mv::new(core_sq);

                // Validate and play move
                match game.play(mv) {
                    Ok(()) => {
                        // Send move to the engine that made it (to update its state)
                        if let Err(e) = current_engine.play(vec![sq]) {
                            eprintln!("Failed to send move to current engine: {e}");
                        }
                        // Send move to opponent
                        if let Err(e) = opponent_engine.play(vec![sq]) {
                            eprintln!("Failed to send move to opponent: {e}");
                        }
                    }
                    Err(e) => {
                        eprintln!("Illegal move from {}: {e}", current_engine.name());
                        current_engine.write_log("illegal_move");
                        return GameResult {
                            game,
                            outcome: None,
                            dev_score: if is_dev_turn { 0.0 } else { 1.0 },
                            termination_reason: Some(format!("Illegal move: {e}")),
                            faulty_engine: Some(if is_dev_turn { FaultyEngine::Dev } else { FaultyEngine::Base }),
                        };
                    }
                }
            }
            MoveResult::NoMove => {
                eprintln!("{} returned no move", current_engine.name());
                current_engine.write_log("no_move");
                return GameResult {
                    game,
                    outcome: None,
                    dev_score: if is_dev_turn { 0.0 } else { 1.0 },
                    termination_reason: Some("Engine returned no move".into()),
                    faulty_engine: Some(if is_dev_turn { FaultyEngine::Dev } else { FaultyEngine::Base }),
                };
            }
            MoveResult::Timeout => {
                eprintln!("{} timed out", current_engine.name());
                return GameResult {
                    game,
                    outcome: None,
                    dev_score: if is_dev_turn { 0.0 } else { 1.0 },
                    termination_reason: Some("Timeout".into()),
                    faulty_engine: Some(if is_dev_turn { FaultyEngine::Dev } else { FaultyEngine::Base }),
                };
            }
            MoveResult::Crash => {
                eprintln!("{} crashed", current_engine.name());
                return GameResult {
                    game,
                    outcome: None,
                    dev_score: if is_dev_turn { 0.0 } else { 1.0 },
                    termination_reason: Some("Engine crashed".into()),
                    faulty_engine: Some(if is_dev_turn { FaultyEngine::Dev } else { FaultyEngine::Base }),
                };
            }
            MoveResult::IllegalProtocol(msg) => {
                eprintln!("{} protocol error: {msg}", current_engine.name());
                current_engine.write_log("protocol_error");
                return GameResult {
                    game,
                    outcome: None,
                    dev_score: if is_dev_turn { 0.0 } else { 1.0 },
                    termination_reason: Some(format!("Protocol error: {msg}")),
                    faulty_engine: Some(if is_dev_turn { FaultyEngine::Dev } else { FaultyEngine::Base }),
                };
            }
            MoveResult::EngineError(msg) => {
                eprintln!("{} error: {msg}", current_engine.name());
                // Engine errors don't cause a loss, just report them
                continue;
            }
        }
    }
}

fn make_error_result(
    game: Game,
    dev_is_white: bool,
    white_failed: bool,
    reason: &str,
    engine: &mut Engine,
) -> GameResult {
    engine.write_log(reason);
    let dev_failed = (dev_is_white && white_failed) || (!dev_is_white && !white_failed);
    GameResult {
        game,
        outcome: None,
        dev_score: if dev_failed { 0.0 } else { 1.0 },
        termination_reason: Some(reason.to_string()),
        faulty_engine: Some(if dev_failed { FaultyEngine::Dev } else { FaultyEngine::Base }),
    }
}

fn compute_dev_score(outcome: &Outcome, dev_is_white: bool) -> f64 {
    match outcome.winner() {
        Some(Color::White) => {
            if dev_is_white {
                1.0
            } else {
                0.0
            }
        }
        Some(Color::Black) => {
            if dev_is_white {
                0.0
            } else {
                1.0
            }
        }
        None => 0.5,
    }
}
