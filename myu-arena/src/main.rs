mod engine;
mod gsprt;
mod match_runner;
mod opening_book;
mod stats;

use std::{
    fs::{self, File, OpenOptions},
    io::{BufWriter, Write},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};

use clap::{Parser, Subcommand};
use gsprt::{GsprtResult, SprtState};
use match_runner::MatchRunner;
use myu_core::{MvFormat, PgnFormat, format_game};
use opening_book::OpeningBook;
use stats::MatchStats;

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

    /// Maximum number of game pairs
    #[arg(long, default_value = "5000")]
    max_pairs: usize,

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
    println!("Generating {count} openings of depth {depth}...");
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
        Some(path) => OpeningBook::from_file(path).unwrap_or_else(|e| {
            eprintln!("Error loading opening book: {e}");
            std::process::exit(1);
        }),
        None => OpeningBook::empty(),
    };

    // Set up games output file
    let games_writer = args.games_file.as_ref().map(|path| {
        let file = OpenOptions::new().create(true).append(true).open(path).unwrap_or_else(|e| {
            eprintln!("Error opening games file: {e}");
            std::process::exit(1);
        });
        Arc::new(Mutex::new(BufWriter::new(file)))
    });

    // Set up CTRL+C handler
    let stop_flag = Arc::new(AtomicBool::new(false));
    {
        let stop_flag = stop_flag.clone();
        ctrlc::set_handler(move || {
            eprintln!("\nReceived CTRL+C, stopping match gracefully...");
            stop_flag.store(true, Ordering::SeqCst);
        })
        .expect("Error setting Ctrl-C handler");
    }

    // Initialize match state
    let mut stats = MatchStats::default();
    let mut sprt = SprtState::new(args.alpha, args.beta, args.elo0, args.elo1);

    // Print header
    print_header(&args, &sprt, &opening_book);
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
    let mut completed_pairs = 0;
    let pgn_format = PgnFormat { move_numbers: true, newlines: true, mv_format: MvFormat::Plain };

    for pair_result in runner.run_pairs(args.max_pairs) {
        completed_pairs += 1;
        stats.record_pair(&pair_result);
        sprt.update(&stats.pentanomial);

        // Write games to file
        if let Some(ref writer) = games_writer {
            write_games_to_file(writer, &pair_result, completed_pairs, &pgn_format);
        }

        // Print progress
        let elapsed = start_time.elapsed().as_secs_f64();
        let games_per_sec = if elapsed > 0.0 { (completed_pairs * 2) as f64 / elapsed } else { 0.0 };

        println!(
            "Pair {:>4}: {:5} | Dev +{}-{}={} ({:.1}%) | Elo: {:+.1} | LLR={:.3} [{:.3},{:.3}] | {:.1} g/s",
            completed_pairs,
            pair_result.outcome,
            stats.dev_wins,
            stats.dev_losses,
            stats.draws,
            stats.dev_score_pct(),
            sprt.elo_estimate(&stats.pentanomial),
            sprt.llr(),
            sprt.lower_bound(),
            sprt.upper_bound(),
            games_per_sec,
        );

        // Check if test concluded
        match sprt.test_result() {
            GsprtResult::Accept => {
                println!("\nSPRT: H1 accepted (elo >= {:.1})", args.elo1);
                break;
            }
            GsprtResult::Reject => {
                println!("\nSPRT: H0 accepted (elo <= {:.1})", args.elo0);
                break;
            }
            GsprtResult::Continue => {}
        }

        // Check if stopped
        if stop_flag.load(Ordering::SeqCst) {
            println!("\nMatch stopped by user.");
            break;
        }
    }

    // Print final stats
    print_final_results(&stats, &sprt, completed_pairs, start_time.elapsed());
}

fn print_header(args: &TestArgs, sprt: &SprtState, opening_book: &OpeningBook) {
    println!("myu-arena SPRT Test");
    println!("===================");
    println!("Dev:  {}", args.dev.display());
    println!("Base: {}", args.base.display());
    println!();
    println!("SPRT: elo0={:.1}, elo1={:.1}, alpha={:.2}, beta={:.2}", args.elo0, args.elo1, args.alpha, args.beta);
    println!("Bounds: lower={:.3}, upper={:.3}", sprt.lower_bound(), sprt.upper_bound());
    println!("Time control: {} ms/move (timeout leniency: {:.1}x)", args.time_ms, args.timeout_leniency);
    println!("Concurrency: {}", args.concurrency);
    println!("Max pairs: {}", args.max_pairs);
    if let Some(ref path) = args.opening_book {
        println!("Opening book: {} ({} openings)", path.display(), opening_book.len());
    }
    println!();
}

fn write_games_to_file(
    writer: &Mutex<BufWriter<File>>,
    pair: &match_runner::GamePairResult,
    round: usize,
    pgn_format: &PgnFormat,
) {
    let mut w = writer.lock().unwrap();
    for (i, game_result) in [&pair.game1, &pair.game2].iter().enumerate() {
        let dev_color = if i == 0 { "White" } else { "Black" };
        writeln!(w, "[Event \"SPRT Test\"]").ok();
        writeln!(w, "[Round \"{round}\"]").ok();
        writeln!(w, "[Dev \"{dev_color}\"]").ok();
        if let Some(ref reason) = game_result.termination_reason {
            writeln!(w, "[Termination \"{reason}\"]").ok();
        }
        let game_str = format_game(&game_result.game, pgn_format, true, true);
        writeln!(w, "{game_str}").ok();
        writeln!(w).ok();
    }
    w.flush().ok();
}

fn print_final_results(stats: &MatchStats, sprt: &SprtState, pairs: usize, elapsed: std::time::Duration) {
    println!();
    println!("=== Final Results ===");
    println!();
    println!("Games played: {} ({pairs} pairs)", stats.total_games());
    println!("Time elapsed: {:.1}s", elapsed.as_secs_f64());
    println!();
    println!("Score: Dev +{}-{}={}", stats.dev_wins, stats.dev_losses, stats.draws);
    println!("Score percentage: {:.2}%", stats.dev_score_pct());
    println!();
    println!("Pentanomial: {}", stats.pentanomial);
    println!();
    println!("Elo: {:+.2}", sprt.elo_estimate(&stats.pentanomial));
    println!("LLR: {:.3} [{:.3}, {:.3}]", sprt.llr(), sprt.lower_bound(), sprt.upper_bound());
    println!();
    println!("=== Error Summary ===");
    println!(
        "Dev:  {} crashes, {} timeouts, {} illegal, {} loops",
        stats.dev_crashes, stats.dev_timeouts, stats.dev_illegal_moves, stats.dev_infinite_loops
    );
    println!(
        "Base: {} crashes, {} timeouts, {} illegal, {} loops",
        stats.base_crashes, stats.base_timeouts, stats.base_illegal_moves, stats.base_infinite_loops
    );
}
