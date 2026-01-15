mod engine;
mod gsprt;
mod match_runner;
mod opening_book;
mod stats;

use std::{
    fs::{self, File, OpenOptions},
    io::BufWriter,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};

use clap::{Parser, Subcommand};
use match_runner::{MatchRunner, PentanomialOutcome};
use myu_core::{MvFormat, PgnFormat, format_game};
use opening_book::OpeningBook;
use stats::MatchStats;

use crate::gsprt::{GsprtResult, SprtState};

/// CLI SPRT Elo testing tool for Migoyugo
#[derive(Parser, Debug)]
#[command(name = "myu-arena", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Run SPRT test between two engines
    Test(TestArgs),
    /// Generate a random opening book
    GenBook {
        /// Number of openings to generate
        #[arg(long, default_value = "50")]
        count: usize,
        /// Depth of each opening (number of moves)
        #[arg(long, default_value = "6")]
        depth: usize,
        /// Output file path
        #[arg(long)]
        output: PathBuf,
    },
}

#[derive(Parser, Debug)]
struct TestArgs {
    /// Path to the 'dev' engine executable
    #[arg(long)]
    dev: PathBuf,

    /// Path to the 'base' engine executable
    #[arg(long)]
    base: PathBuf,

    /// SPRT alpha (false positive rate)
    #[arg(long, default_value = "0.05")]
    alpha: f64,

    /// SPRT beta (false negative rate)
    #[arg(long, default_value = "0.05")]
    beta: f64,

    /// Elo null hypothesis (H0)
    #[arg(long, default_value = "0.0")]
    elo0: f64,

    /// Elo alternative hypothesis (H1)
    #[arg(long, default_value = "5.0")]
    elo1: f64,

    /// Maximum number of games (must be even)
    #[arg(long, default_value = "10000")]
    max_games: usize,

    /// Number of concurrent game pairs
    #[arg(long, default_value = "1")]
    concurrency: usize,

    /// Time control in milliseconds per move
    #[arg(long, default_value = "1000")]
    time_ms: u64,

    /// Timeout leniency factor (multiplier on time control for timeout detection)
    #[arg(long, default_value = "10.0")]
    timeout_leniency: f64,

    /// Path to opening book file (one line per opening, space-separated moves)
    #[arg(long)]
    opening_book: Option<PathBuf>,

    /// Output file for games (games appended as they finish)
    #[arg(long)]
    games_file: Option<PathBuf>,

    /// Directory for engine error logs
    #[arg(long)]
    logs_dir: Option<PathBuf>,
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Test(args) => run_test(args),
        Commands::GenBook { count, depth, output } => gen_book(count, depth, output),
    }
}

fn gen_book(count: usize, depth: usize, output: PathBuf) {
    println!("Generating {} openings of depth {}...", count, depth);
    let book = OpeningBook::generate_random(count, depth);
    match book.save_to_file(&output) {
        Ok(()) => println!("Opening book saved to {}", output.display()),
        Err(e) => {
            eprintln!("Error saving opening book: {e}");
            std::process::exit(1);
        }
    }
}

fn run_test(args: TestArgs) {
    // Validate arguments
    if !args.max_games.is_multiple_of(2) {
        eprintln!("Error: max_games must be even (paired games)");
        std::process::exit(1);
    }

    if args.elo0 >= args.elo1 {
        eprintln!("Error: elo0 must be less than elo1");
        std::process::exit(1);
    }

    // Set up logs directory
    if let Some(ref logs_dir) = args.logs_dir
        && let Err(e) = fs::create_dir_all(logs_dir)
    {
        eprintln!("Error creating logs directory: {e}");
        std::process::exit(1);
    }

    // Load opening book
    let opening_book = match &args.opening_book {
        Some(path) => match OpeningBook::from_file(path) {
            Ok(book) => book,
            Err(e) => {
                eprintln!("Error loading opening book: {e}");
                std::process::exit(1);
            }
        },
        None => OpeningBook::empty(),
    };

    // Set up games output file
    let games_writer: Option<Arc<Mutex<BufWriter<File>>>> = args.games_file.as_ref().map(|path| {
        let file = OpenOptions::new().create(true).append(true).open(path).unwrap_or_else(|e| {
            eprintln!("Error opening games file: {e}");
            std::process::exit(1);
        });
        Arc::new(Mutex::new(BufWriter::new(file)))
    });

    // Set up CTRL+C handler
    let stop_flag = Arc::new(AtomicBool::new(false));
    let stop_flag_ctrlc = stop_flag.clone();
    ctrlc::set_handler(move || {
        eprintln!("\nReceived CTRL+C, stopping match gracefully...");
        stop_flag_ctrlc.store(true, Ordering::SeqCst);
    })
    .expect("Error setting Ctrl-C handler");

    // Initialize match state
    let stats = Arc::new(Mutex::new(MatchStats::new()));
    let sprt_state = Arc::new(Mutex::new(SprtState::new(args.alpha, args.beta, args.elo0, args.elo1)));

    // Print header
    println!("myu-arena SPRT Test");
    println!("===================");
    println!("Dev:  {}", args.dev.display());
    println!("Base: {}", args.base.display());
    println!();
    println!("SPRT: elo0={:.1}, elo1={:.1}, alpha={:.2}, beta={:.2}", args.elo0, args.elo1, args.alpha, args.beta);
    let (lower, upper) = {
        let sprt = sprt_state.lock().unwrap();
        (sprt.lower_bound(), sprt.upper_bound())
    };
    println!("Bounds: lower={:.3}, upper={:.3}", lower, upper);
    println!("Time control: {} ms/move (timeout leniency: {:.1}x)", args.time_ms, args.timeout_leniency);
    println!("Concurrency: {}", args.concurrency);
    println!("Max games: {}", args.max_games);
    if let Some(ref path) = args.opening_book {
        println!("Opening book: {} ({} openings)", path.display(), opening_book.len());
    }
    println!();

    // Flush stdout to ensure header is visible
    use std::io::Write;
    std::io::stdout().flush().ok();

    let start_time = Instant::now();

    // Create match runner
    let runner = MatchRunner::new(
        args.dev.clone(),
        args.base.clone(),
        args.time_ms,
        args.timeout_leniency,
        args.logs_dir.clone(),
        opening_book,
        args.concurrency,
        stop_flag.clone(),
    );

    // Run the match
    let max_pairs = args.max_games / 2;
    let mut completed_pairs = 0;

    let pgn_format = PgnFormat { move_numbers: true, newlines: false, mv_format: MvFormat::Plain };

    for pair_result in runner.run_pairs(max_pairs) {
        completed_pairs += 1;

        // Update stats
        {
            let mut s = stats.lock().unwrap();
            s.record_pair(&pair_result);
        }

        // Update SPRT state
        let sprt_result = {
            let mut sprt = sprt_state.lock().unwrap();
            let s = stats.lock().unwrap();
            sprt.update(&s.pentanomial);
            sprt.test_result()
        };

        // Write games to file
        if let Some(ref writer) = games_writer {
            let mut w = writer.lock().unwrap();
            for (game_num, game_result) in [&pair_result.game1, &pair_result.game2].iter().enumerate() {
                let dev_color = if game_num == 0 { "White" } else { "Black" };
                writeln!(w, "[Event \"SPRT Test\"]").ok();
                writeln!(w, "[Round \"{}\"]", completed_pairs).ok();
                writeln!(w, "[Dev \"{}\"]", dev_color).ok();
                if let Some(reason) = &game_result.termination_reason {
                    writeln!(w, "[Termination \"{}\"]", reason).ok();
                }
                let game_str = format_game(&game_result.game, &pgn_format, true, true);
                writeln!(w, "{}", game_str).ok();
                writeln!(w).ok();
            }
            w.flush().ok();
        }

        // Print progress
        let s = stats.lock().unwrap();
        let sprt = sprt_state.lock().unwrap();
        let elapsed = start_time.elapsed().as_secs_f64();
        let games_per_sec = if elapsed > 0.0 { (completed_pairs * 2) as f64 / elapsed } else { 0.0 };

        println!(
            "Pair {:>4}: {} | Dev +{}-{}={} ({:.1}%) | LLR={:.3} [{:.3},{:.3}] | {:.1} g/s",
            completed_pairs,
            format_pentanomial_outcome(&pair_result.outcome),
            s.dev_wins,
            s.dev_losses,
            s.draws,
            if s.total_games() > 0 {
                100.0 * (s.dev_wins as f64 + 0.5 * s.draws as f64) / s.total_games() as f64
            } else {
                50.0
            },
            sprt.llr(),
            sprt.lower_bound(),
            sprt.upper_bound(),
            games_per_sec
        );

        // Check if test concluded
        match sprt_result {
            GsprtResult::Accept => {
                println!();
                println!("SPRT: H1 accepted (elo >= {:.1})", args.elo1);
                break;
            }
            GsprtResult::Reject => {
                println!();
                println!("SPRT: H0 accepted (elo <= {:.1})", args.elo0);
                break;
            }
            GsprtResult::Continue => {}
        }

        // Check if stopped
        if stop_flag.load(Ordering::SeqCst) {
            println!();
            println!("Match stopped by user.");
            break;
        }
    }

    // Print final stats
    let elapsed = start_time.elapsed();
    let s = stats.lock().unwrap();
    let sprt = sprt_state.lock().unwrap();

    println!();
    println!("=== Final Results ===");
    println!();
    println!("Games played: {} ({} pairs)", s.total_games(), completed_pairs);
    println!("Time elapsed: {:.1}s", elapsed.as_secs_f64());
    println!();
    println!("Score: Dev +{}-{}={}", s.dev_wins, s.dev_losses, s.draws);
    println!(
        "Score percentage: {:.2}%",
        if s.total_games() > 0 {
            100.0 * (s.dev_wins as f64 + 0.5 * s.draws as f64) / s.total_games() as f64
        } else {
            50.0
        }
    );
    println!();
    println!(
        "Pentanomial: LL={} LD={} DD/WL={} WD={} WW={}",
        s.pentanomial[0], s.pentanomial[1], s.pentanomial[2], s.pentanomial[3], s.pentanomial[4]
    );
    println!();
    println!("LLR: {:.3} [{:.3}, {:.3}]", sprt.llr(), sprt.lower_bound(), sprt.upper_bound());

    if let Some((elo_est, elo_low, elo_high)) = sprt.elo_estimate() {
        println!("Elo estimate: {:.2} [{:.2}, {:.2}]", elo_est, elo_low, elo_high);
    }

    println!();
    println!("=== Error Summary ===");
    println!("Dev crashes: {}", s.dev_crashes);
    println!("Dev timeouts: {}", s.dev_timeouts);
    println!("Dev illegal moves: {}", s.dev_illegal_moves);
    println!("Dev infinite loops: {}", s.dev_infinite_loops);
    println!("Base crashes: {}", s.base_crashes);
    println!("Base timeouts: {}", s.base_timeouts);
    println!("Base illegal moves: {}", s.base_illegal_moves);
    println!("Base infinite loops: {}", s.base_infinite_loops);
}

fn format_pentanomial_outcome(outcome: &PentanomialOutcome) -> &'static str {
    match outcome {
        PentanomialOutcome::LL => "LL  ",
        PentanomialOutcome::LD => "LD  ",
        PentanomialOutcome::DDWL => "DD/WL",
        PentanomialOutcome::WD => "WD  ",
        PentanomialOutcome::WW => "WW  ",
    }
}
