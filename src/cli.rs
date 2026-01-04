use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "cctr",
    about = "CLI Corpus Test Runner - Named after the Corpus Christi Terminal Railroad",
    version
)]
pub struct Cli {
    /// Filter by suite or suite/file pattern (e.g., "languages/python" or "languages/python/grep")
    pub filter: Option<String>,

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

    /// Root directory for test discovery
    #[arg(short = 'C', long, default_value = ".")]
    pub root: PathBuf,
}
