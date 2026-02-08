use std::path::PathBuf;

use clap::{Parser, Subcommand};

/// CLI SPRT Elo testing tool for Migoyugo
#[derive(Parser, Debug)]
#[command(name = "myu-arena", version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[allow(clippy::large_enum_variant)]
#[derive(Subcommand, Debug)]
pub enum Commands {
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
#[command(args_override_self = true)]
pub struct TestArgs {
    /// Path to the 'dev' engine executable
    #[arg(long)]
    pub dev: PathBuf,

    /// Path to the 'base' engine executable
    #[arg(long)]
    pub base: PathBuf,

    /// JSON settings for the dev engine (string or file path)
    #[arg(long)]
    pub dev_settings: Option<String>,

    /// JSON settings for the base engine (string or file path)
    #[arg(long)]
    pub base_settings: Option<String>,

    /// JSON settings for both engines (string or file path)
    #[arg(long)]
    pub engine_settings: Option<String>,

    /// SPRT alpha (false positive rate)
    #[arg(long, default_value = "0.05")]
    pub alpha: f64,

    /// SPRT beta (false negative rate)
    #[arg(long, default_value = "0.05")]
    pub beta: f64,

    /// Elo null hypothesis (H0)
    #[arg(long, default_value = "0.0")]
    pub elo0: f64,

    /// Elo alternative hypothesis (H1)
    #[arg(long, default_value = "5.0")]
    pub elo1: f64,

    /// Maximum number of game pairs
    #[arg(long, default_value = "50000")]
    pub max_pairs: usize,

    /// Number of concurrent game pairs
    #[arg(long, default_value = "1")]
    pub concurrency: usize,

    /// Time control in milliseconds per move
    #[arg(long, default_value = "100")]
    pub time_ms: u64,

    /// Timeout leniency factor (multiplier on time control for timeout detection)
    #[arg(long, default_value = "3.0")]
    pub timeout_leniency: f64,

    /// Path to opening book file (one line per opening, space-separated moves)
    #[arg(long)]
    pub opening_book: Option<PathBuf>,

    /// Output file for games (games appended as they finish)
    #[arg(long)]
    pub games_file: Option<PathBuf>,

    /// Directory for engine error logs
    #[arg(long)]
    pub logs_dir: Option<PathBuf>,
}
