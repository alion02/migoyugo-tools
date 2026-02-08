//! Match runner for paired games with pentanomial scoring.

use std::{
    fmt,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
};

use myu_core::{Color, Game, Mv, Outcome};

use crate::{
    engine::{Engine, LogReason, MoveResult},
    opening_book::OpeningBook,
};

// =============================================================================
// Public Types
// =============================================================================

/// Game score from one player's perspective
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Score {
    Win,
    Draw,
    Loss,
}

impl Score {
    pub fn flipped(self) -> Self {
        match self {
            Self::Win => Self::Loss,
            Self::Draw => Self::Draw,
            Self::Loss => Self::Win,
        }
    }
}

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
    /// Create from pair of dev scores
    pub fn from_scores(score1: Score, score2: Score) -> Self {
        use Score::*;
        match (score1, score2) {
            (Loss, Loss) => Self::Ll,
            (Loss, Draw) | (Draw, Loss) => Self::Ld,
            (Draw, Draw) | (Win, Loss) | (Loss, Win) => Self::DdWl,
            (Win, Draw) | (Draw, Win) => Self::Wd,
            (Win, Win) => Self::Ww,
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

/// Kind of abnormal termination
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminationKind {
    Crash,
    Timeout,
    IllegalMove,
    ProtocolError,
    NoMove,
    SyncFailed,
    SpawnFailed,
    Stopped,
}

/// Abnormal termination details
#[derive(Debug, Clone)]
pub struct Termination {
    pub kind: TerminationKind,
    pub details: String,
    pub faulty_engine: Option<FaultyEngine>,
}

/// Result of a single game
#[derive(Debug, Clone)]
pub struct GameResult {
    pub game: Game,
    /// Score from dev's perspective
    pub dev_score: Score,
    /// Termination details if abnormal
    pub termination: Option<Termination>,
}

/// Result of a game pair
#[derive(Debug)]
pub struct GamePairResult {
    pub game1: GameResult, // dev plays White
    pub game2: GameResult, // dev plays Black
    pub outcome: Pentanomial,
}

/// Configuration for running games
pub struct GameConfig {
    pub dev_path: PathBuf,
    pub base_path: PathBuf,
    pub time_ms: u64,
    pub timeout_leniency: f64,
    pub logs_dir: Option<PathBuf>,
}

// =============================================================================
// Engine Role & Managed Engine
// =============================================================================

/// Identifies which engine role (for spawning and error reporting)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EngineRole {
    Dev,
    Base,
}

impl EngineRole {
    fn path(self, config: &GameConfig) -> &Path {
        match self {
            EngineRole::Dev => &config.dev_path,
            EngineRole::Base => &config.base_path,
        }
    }

    fn name(self) -> &'static str {
        match self {
            EngineRole::Dev => "dev",
            EngineRole::Base => "base",
        }
    }

    fn as_faulty(self) -> FaultyEngine {
        match self {
            EngineRole::Dev => FaultyEngine::Dev,
            EngineRole::Base => FaultyEngine::Base,
        }
    }
}

/// A managed engine that tracks health and handles spawn/reset
struct ManagedEngine {
    engine: Option<Engine>,
    role: EngineRole,
    stop_flag: Arc<AtomicBool>,
}

impl ManagedEngine {
    fn new(role: EngineRole, stop_flag: Arc<AtomicBool>) -> Self {
        Self { engine: None, role, stop_flag }
    }

    /// Ensure the engine is ready to play. Spawns if needed.
    fn ensure_ready(&mut self, config: &GameConfig) -> Result<&mut Engine, String> {
        if self.engine.is_none() {
            let mut engine = Engine::spawn(
                self.role.name(),
                self.role.path(config),
                config.time_ms,
                config.timeout_leniency,
                config.logs_dir.clone(),
                self.stop_flag.clone(),
            )?;
            engine.sync()?;
            self.engine = Some(engine);
        }
        Ok(self.engine.as_mut().unwrap())
    }

    /// Reset the engine for a new game. Marks as needing respawn on failure.
    fn reset(&mut self) {
        if let Some(ref mut engine) = self.engine
            && engine.reset().is_err()
        {
            self.engine = None;
        }
    }

    /// Mark the engine as needing respawn
    fn mark_failed(&mut self) {
        self.engine = None;
    }

    /// Check if the engine needs respawn
    fn needs_respawn(&self) -> bool {
        self.engine.is_none()
    }
}

// =============================================================================
// Raw Game Result (color-neutral)
// =============================================================================

/// Color-neutral termination (which color's engine failed)
struct RawTermination {
    kind: TerminationKind,
    details: String,
    faulty_color: Color,
}

/// Color-neutral game result (score is from white's perspective)
struct RawGameResult {
    game: Game,
    white_score: Score,
    termination: Option<RawTermination>,
}

impl RawGameResult {
    fn normal(game: Game, outcome: &Outcome) -> Self {
        let white_score = match outcome.winner() {
            Some(Color::White) => Score::Win,
            Some(Color::Black) => Score::Loss,
            None => Score::Draw,
        };
        Self { game, white_score, termination: None }
    }

    fn error(game: Game, faulty_color: Color, kind: TerminationKind, details: String) -> Self {
        // The faulty engine loses
        let white_score = if faulty_color == Color::White { Score::Loss } else { Score::Win };
        Self { game, white_score, termination: Some(RawTermination { kind, details, faulty_color }) }
    }

    fn stopped(game: Game) -> Self {
        Self {
            game,
            white_score: Score::Draw,
            termination: Some(RawTermination {
                kind: TerminationKind::Stopped,
                details: "Match stopped".into(),
                faulty_color: Color::White, // Arbitrary, won't be used
            }),
        }
    }
}

// =============================================================================
// Match Runner
// =============================================================================

/// Match runner that manages concurrent game pairs
pub struct MatchRunner {
    config: Arc<GameConfig>,
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
            config: Arc::new(GameConfig { dev_path, base_path, time_ms, timeout_leniency, logs_dir }),
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
            let config = self.config.clone();
            let result_tx = result_tx.clone();
            let task_rx = task_rx.clone();
            let stop_flag = self.stop_flag.clone();

            thread::spawn(move || {
                let mut dev = ManagedEngine::new(EngineRole::Dev, stop_flag.clone());
                let mut base = ManagedEngine::new(EngineRole::Base, stop_flag.clone());

                while !stop_flag.load(Ordering::SeqCst) {
                    let opening = match task_rx.lock().unwrap().recv() {
                        Ok(o) => o,
                        Err(_) => break,
                    };

                    let result = play_game_pair(&config, &mut dev, &mut base, &opening, &stop_flag);

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
        GamePairIterator { result_rx, received: 0, max_pairs, stop_flag: self.stop_flag, concurrency: self.concurrency }
    }
}

struct GamePairIterator {
    result_rx: mpsc::Receiver<GamePairResult>,
    received: usize,
    max_pairs: usize,
    stop_flag: Arc<AtomicBool>,
    concurrency: usize,
}

impl Iterator for GamePairIterator {
    type Item = GamePairResult;

    fn next(&mut self) -> Option<Self::Item> {
        if self.received >= self.max_pairs {
            return None;
        }

        // If stop requested, don't iterate further
        if self.stop_flag.load(Ordering::SeqCst) {
            return None;
        }

        self.result_rx.recv().ok().inspect(|_| self.received += 1)
    }
}

impl Drop for GamePairIterator {
    fn drop(&mut self) {
        // Signal stop so workers finish after their current task
        self.stop_flag.store(true, Ordering::SeqCst);

        // Wait for any in-flight game pairs to complete (up to concurrency workers may be active)
        // Using blocking recv since we want to wait for workers to finish cleanly
        for _ in 0..self.concurrency {
            if self.result_rx.recv().is_err() {
                break; // Channel closed, all workers have exited
            }
        }
    }
}

// =============================================================================
// Game Pair Execution
// =============================================================================

fn play_game_pair(
    config: &GameConfig,
    dev: &mut ManagedEngine,
    base: &mut ManagedEngine,
    opening: &[Mv],
    stop_flag: &Arc<AtomicBool>,
) -> GamePairResult {
    // Game 1: dev = white, base = black
    let game1 = play_single_game(config, dev, base, opening, stop_flag);

    // Reset for game 2
    dev.reset();
    base.reset();

    // Game 2: dev = black, base = white
    let game2 = if dev.needs_respawn() || base.needs_respawn() {
        // Reset failed, create error result
        spawn_error_result(config, dev, base)
    } else {
        play_single_game(config, base, dev, opening, stop_flag)
    };

    // Reset for next pair
    dev.reset();
    base.reset();

    let outcome = Pentanomial::from_scores(game1.dev_score, game2.dev_score);
    GamePairResult { game1, game2, outcome }
}

fn play_single_game(
    config: &GameConfig,
    white: &mut ManagedEngine,
    black: &mut ManagedEngine,
    opening: &[Mv],
    stop_flag: &Arc<AtomicBool>,
) -> GameResult {
    // Ensure engines are ready
    let (white_engine, black_engine) = match (white.ensure_ready(config), black.ensure_ready(config)) {
        (Ok(w), Ok(b)) => (w, b),
        (Err(e), _) => {
            white.mark_failed();
            return spawn_error(white.role, e);
        }
        (_, Err(e)) => {
            black.mark_failed();
            return spawn_error(black.role, e);
        }
    };

    // Send opening moves to engines
    let mut game = Game::new();
    let opening_sqs: Vec<_> = opening.iter().map(|mv| mv.sq.into()).collect();

    if !opening_sqs.is_empty() {
        _ = white_engine.play(opening_sqs.clone());
        _ = black_engine.play(opening_sqs);
        for mv in opening {
            if game.play(*mv).is_err() {
                break;
            }
        }
    }

    // Play the game
    let raw = run_game_loop(game, white_engine, black_engine, config.time_ms, stop_flag);

    // Interpret result
    interpret_result(raw, white, black)
}

fn spawn_error(failed_role: EngineRole, error: String) -> GameResult {
    let dev_failed = failed_role == EngineRole::Dev;
    GameResult {
        game: Game::new(),
        dev_score: if dev_failed { Score::Loss } else { Score::Win },
        termination: Some(Termination {
            kind: TerminationKind::SpawnFailed,
            details: error,
            faulty_engine: Some(failed_role.as_faulty()),
        }),
    }
}

fn spawn_error_result(config: &GameConfig, dev: &mut ManagedEngine, base: &mut ManagedEngine) -> GameResult {
    // Try to figure out which one failed
    let dev_failed = dev.needs_respawn();
    let base_failed = base.needs_respawn();

    // Try to respawn for subsequent games
    _ = dev.ensure_ready(config);
    _ = base.ensure_ready(config);

    let (faulty, dev_score) = match (dev_failed, base_failed) {
        (true, false) => (Some(FaultyEngine::Dev), Score::Loss),
        (false, true) => (Some(FaultyEngine::Base), Score::Win),
        _ => (None, Score::Draw), // Both failed or neither (shouldn't happen)
    };

    GameResult {
        game: Game::new(),
        dev_score,
        termination: Some(Termination {
            kind: TerminationKind::SyncFailed,
            details: "Reset failed between games".into(),
            faulty_engine: faulty,
        }),
    }
}

// =============================================================================
// Game Loop (color-only, no dev/base knowledge)
// =============================================================================

fn run_game_loop(
    mut game: Game,
    white: &mut Engine,
    black: &mut Engine,
    time_ms: u64,
    stop_flag: &Arc<AtomicBool>,
) -> RawGameResult {
    loop {
        if stop_flag.load(Ordering::SeqCst) {
            return RawGameResult::stopped(game);
        }

        if let Some(outcome) = game.outcome() {
            return RawGameResult::normal(game, &outcome);
        }

        let side_to_move = game.current_state().side_to_move();
        let (current, opponent) =
            if side_to_move == Color::White { (&mut *white, &mut *black) } else { (&mut *black, &mut *white) };

        match current.go(time_ms) {
            MoveResult::Move(sq) => {
                let mv = Mv::new(sq.into());
                match game.play(mv) {
                    Ok(()) => {
                        _ = current.play(vec![sq]);
                        _ = opponent.play(vec![sq]);
                    }
                    Err(e) => {
                        current.write_log(LogReason::IllegalMove);
                        return RawGameResult::error(
                            game,
                            side_to_move,
                            TerminationKind::IllegalMove,
                            format!("Illegal move: {e}"),
                        );
                    }
                }
            }
            MoveResult::NoMove => {
                current.write_log(LogReason::NoMove);
                return RawGameResult::error(
                    game,
                    side_to_move,
                    TerminationKind::NoMove,
                    "Engine returned no move".into(),
                );
            }
            MoveResult::Timeout => {
                return RawGameResult::error(game, side_to_move, TerminationKind::Timeout, "Timeout".into());
            }
            MoveResult::Crash => {
                return RawGameResult::error(game, side_to_move, TerminationKind::Crash, "Engine crashed".into());
            }
            MoveResult::ProtocolError(msg) => {
                current.write_log(LogReason::ProtocolError);
                return RawGameResult::error(
                    game,
                    side_to_move,
                    TerminationKind::ProtocolError,
                    format!("Protocol error: {msg}"),
                );
            }
            MoveResult::EngineError(_) => continue,
            MoveResult::Stopped => return RawGameResult::stopped(game),
        }
    }
}

// =============================================================================
// Result Interpretation
// =============================================================================

fn interpret_result(
    raw: RawGameResult,
    white: &mut ManagedEngine,
    black: &mut ManagedEngine,
) -> GameResult {
    // Convert white_score to dev_score
    let dev_score = if white.role == EngineRole::Dev {
        raw.white_score
    } else {
        raw.white_score.flipped()
    };

    // Convert termination
    let termination = raw.termination.map(|t| {
        let faulty_engine = if t.faulty_color == Color::White { &mut *white } else { &mut *black };

        // Mark the faulty engine for respawn if it's a severe error
        if matches!(t.kind, TerminationKind::Crash | TerminationKind::ProtocolError | TerminationKind::SyncFailed) {
            faulty_engine.mark_failed();
        }

        Termination {
            kind: t.kind,
            details: t.details,
            faulty_engine: if t.kind == TerminationKind::Stopped {
                None
            } else {
                Some(faulty_engine.role.as_faulty())
            },
        }
    });

    GameResult { game: raw.game, dev_score, termination }
}
