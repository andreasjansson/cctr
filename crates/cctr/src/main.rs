use cctr::cli::Cli;
use cctr::discover::discover_suites;
use cctr::output::Output;
use cctr::parse::parse_corpus_file;
use cctr::runner::{run_suite, ProgressEvent, SuiteResult};
use cctr::update::update_corpus_file;
use clap::Parser;
use rayon::prelude::*;
use std::sync::mpsc;
use std::thread;
use std::time::Instant;

fn main() -> anyhow::Result<()> {
    // Reset SIGPIPE handler to default (terminate) so piping to head/tail works correctly
    #[cfg(unix)]
    {
        unsafe {
            libc::signal(libc::SIGPIPE, libc::SIG_DFL);
        }
    }
    let cli = Cli::parse();

    let use_color = !cli.no_color && atty::is(atty::Stream::Stdout);
    let mut output = Output::new(use_color);

    let root = cli
        .test_root
        .canonicalize()
        .unwrap_or(cli.test_root.clone());

    if cli.list {
        list_tests(&root, cli.pattern.as_deref(), &mut output)?;
        return Ok(());
    }

    let all_suites = discover_suites(&root)?;
    let suites: Vec<_> = all_suites.into_iter().collect();

    if suites.is_empty() {
        eprintln!("No test suites found");
        std::process::exit(1);
    }

    let start_time = Instant::now();

    let (progress_tx, progress_rx) = mpsc::channel::<ProgressEvent>();
    let verbose = cli.verbose;

    let update = cli.update;
    let progress_handle = thread::spawn(move || {
        let mut output = Output::new(use_color);
        for event in progress_rx {
            output.print_progress(&event, verbose, update);
        }
        output.finish_progress();
    });

    let pattern = cli.pattern.as_deref();
    eprintln!(
        "[CCTR MAIN] Starting {} suites (parallel={})",
        suites.len(),
        !cli.sequential && suites.len() > 1
    );
    let results: Vec<SuiteResult> = if cli.sequential || suites.len() == 1 {
        suites
            .iter()
            .map(|suite| run_suite(suite, pattern, Some(&progress_tx)))
            .collect()
    } else {
        suites
            .par_iter()
            .map(|suite| {
                let tx = progress_tx.clone();
                run_suite(suite, pattern, Some(&tx))
            })
            .collect()
    };
    eprintln!(
        "[CCTR MAIN] All suites complete, got {} results",
        results.len()
    );

    eprintln!("[CCTR MAIN] Dropping progress_tx");
    drop(progress_tx);
    eprintln!("[CCTR MAIN] Joining progress_handle");
    progress_handle.join().unwrap();
    eprintln!("[CCTR MAIN] progress_handle joined");

    if cli.update {
        for suite_result in &results {
            for file_result in &suite_result.file_results {
                let failed: Vec<_> = file_result
                    .results
                    .iter()
                    .filter(|r| !r.passed && r.actual_output.is_some())
                    .collect();

                if !failed.is_empty() {
                    update_corpus_file(&file_result.file_path, &failed)?;
                    eprintln!("Updated: {}", file_result.file_path.display());
                }
            }
        }
    }

    let elapsed = start_time.elapsed();
    output.print_results(&results, elapsed, cli.update);

    let all_passed = results
        .iter()
        .all(|r| r.passed() || r.setup_error.is_some());

    std::process::exit(if all_passed { 0 } else { 1 });
}

fn list_tests(
    root: &std::path::Path,
    pattern: Option<&str>,
    output: &mut Output,
) -> anyhow::Result<()> {
    let suites = discover_suites(root)?;

    let mut suite_tests = Vec::new();
    for suite in &suites {
        let mut all_tests = Vec::new();
        for file in suite.corpus_files() {
            let tests = parse_corpus_file(&file)?;
            all_tests.extend(tests);
        }

        if let Some(pat) = pattern {
            all_tests.retain(|t| t.name.contains(pat));
        }

        if !all_tests.is_empty() || pattern.is_none() {
            suite_tests.push((suite, all_tests));
        }
    }

    output.print_list(&suite_tests);
    Ok(())
}
