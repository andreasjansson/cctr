use cctr::cli::Cli;
use cctr::discover::discover_suites;
use cctr::output::Output;
use cctr::parse_file;
use cctr::runner::{
    is_interrupted, run_from_stdin, run_suite, set_interrupted, ProgressEvent, SuiteResult,
};
use cctr::update::update_corpus_file;
use clap::Parser;
use rayon::prelude::*;
use std::io::Read;
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

    // Set up signal handler for graceful shutdown
    // When interrupted, we set a flag that tells running suites to skip remaining tests
    // but still run their teardown
    if let Err(e) = ctrlc::set_handler(move || {
        use std::io::Write;
        if is_interrupted() {
            let _ = writeln!(std::io::stderr(), "\nForce quit");
            std::process::exit(130);
        }
        let _ = writeln!(
            std::io::stderr(),
            "\nInterrupted - cleaning up... (press Ctrl-C again to force quit)"
        );
        set_interrupted();
    }) {
        eprintln!("Warning: Could not set signal handler: {}", e);
    }

    let cli = Cli::parse();

    let use_color = !cli.no_color && atty::is(atty::Stream::Stdout);
    let mut output = Output::new(use_color);

    // Check for stdin mode
    if cli.test_root.as_os_str() == "-" {
        return run_stdin_mode(&cli, &mut output);
    }

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
    let verbose_level = cli.verbose;

    let update = cli.update;
    let progress_handle = thread::spawn(move || {
        let mut output = Output::new(use_color);
        for event in progress_rx {
            output.print_progress(&event, verbose_level, update);
        }
        output.finish_progress();
    });

    let pattern = cli.pattern.as_deref();
    let stream_output = verbose_level >= 2;
    let results: Vec<SuiteResult> = if cli.sequential || suites.len() == 1 {
        suites
            .iter()
            .map(|suite| run_suite(suite, pattern, Some(&progress_tx), stream_output))
            .collect()
    } else {
        suites
            .par_iter()
            .map(|suite| {
                let tx = progress_tx.clone();
                run_suite(suite, pattern, Some(&tx), stream_output)
            })
            .collect()
    };

    drop(progress_tx);
    progress_handle.join().unwrap();

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

    let all_passed = results.iter().all(|r| r.passed());

    std::process::exit(if all_passed { 0 } else { 1 });
}

fn run_stdin_mode(cli: &Cli, output: &mut Output) -> anyhow::Result<()> {
    let mut content = String::new();
    std::io::stdin().read_to_string(&mut content)?;

    let use_color = !cli.no_color && atty::is(atty::Stream::Stdout);
    let start_time = Instant::now();

    let (progress_tx, progress_rx) = mpsc::channel::<ProgressEvent>();
    let verbose_level = cli.verbose;
    let update = cli.update;

    let progress_handle = thread::spawn(move || {
        let mut output = Output::new(use_color);
        for event in progress_rx {
            output.print_progress(&event, verbose_level, update);
        }
        output.finish_progress();
    });

    let stream_output = verbose_level >= 2;
    let result = run_from_stdin(&content, Some(&progress_tx), stream_output);

    drop(progress_tx);
    progress_handle.join().unwrap();

    let elapsed = start_time.elapsed();
    let results = vec![result];
    output.print_results(&results, elapsed, cli.update);

    let all_passed = results.iter().all(|r| r.passed());

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
            let corpus = parse_file(&file)?;

            // Check if file name matches the pattern
            let file_matches = pattern.is_none_or(|pat| {
                file.file_stem()
                    .and_then(|s| s.to_str())
                    .is_some_and(|name| name.contains(pat))
            });

            // Keep tests where either the file matches or the test name matches
            let filtered: Vec<_> = if let Some(pat) = pattern {
                corpus
                    .tests
                    .into_iter()
                    .filter(|t| file_matches || t.name.contains(pat))
                    .collect()
            } else {
                corpus.tests
            };

            all_tests.extend(filtered);
        }

        if !all_tests.is_empty() || pattern.is_none() {
            suite_tests.push((suite, all_tests));
        }
    }

    output.print_list(&suite_tests);
    Ok(())
}
