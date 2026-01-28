use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "cctr", about = "CLI Corpus Test Runner", version)]
pub struct Cli {
    /// Root directory for test discovery, or "-" to read from stdin
    #[arg(default_value = ".")]
    pub test_root: PathBuf,

    /// Filter tests by name pattern
    #[arg(short, long)]
    pub pattern: Option<String>,

    /// Update expected outputs from actual results
    #[arg(short, long)]
    pub update: bool,

    /// List all available tests
    #[arg(short, long)]
    pub list: bool,

    /// Show each test as it completes with timing
    #[arg(short, long)]
    pub verbose: bool,

    /// Run suites sequentially instead of in parallel
    #[arg(short, long)]
    pub sequential: bool,

    /// Disable colored output
    #[arg(long)]
    pub no_color: bool,

    /// Debug skip condition evaluation
    #[arg(long, hide = true)]
    pub debug_skip: bool,
}
