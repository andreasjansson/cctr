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
    let cli = Cli::parse();

    let use_color = !cli.no_color && atty::is(atty::Stream::Stdout);
    let mut output = Output::new(use_color);

    let root = cli.root.canonicalize().unwrap_or(cli.root.clone());

    if cli.list {
        list_tests(&root, &mut output)?;
        return Ok(());
    }

    let (suite_filter, file_filter) = parse_filter(&cli.filter, &root);

    let all_suites = discover_suites(&root)?;
    let suites: Vec<_> = all_suites
        .into_iter()
        .filter(|s| {
            suite_filter
                .as_ref()
                .map_or(true, |f| s.name.starts_with(f))
        })
        .collect();

    if suites.is_empty() {
        eprintln!("No test suites found");
        std::process::exit(1);
    }

    let start_time = Instant::now();

    let (progress_tx, progress_rx) = mpsc::channel::<ProgressEvent>();
    let verbose = cli.verbose;

    let progress_handle = thread::spawn(move || {
        let mut output = Output::new(use_color);
        for event in progress_rx {
            output.print_progress(&event, verbose);
        }
        output.finish_progress();
    });

    let results: Vec<SuiteResult> = if cli.sequential || suites.len() == 1 {
        suites
            .iter()
            .map(|suite| run_suite(suite, file_filter.as_deref(), Some(&progress_tx)))
            .collect()
    } else {
        suites
            .par_iter()
            .map(|suite| {
                let tx = progress_tx.clone();
                run_suite(suite, file_filter.as_deref(), Some(&tx))
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
    output.print_results(&results, elapsed);

    let all_passed = results
        .iter()
        .all(|r| r.passed() || r.setup_error.is_some());

    std::process::exit(if all_passed { 0 } else { 1 });
}

fn parse_filter(filter: &Option<String>, root: &std::path::Path) -> (Option<String>, Option<String>) {
    match filter {
        None => (None, None),
        Some(f) => {
            if !f.contains('/') {
                return (Some(f.clone()), None);
            }

            let potential_suite = root.join(f);
            if potential_suite.is_dir() {
                return (Some(f.clone()), None);
            }

            if let Some(pos) = f.rfind('/') {
                let suite_part = &f[..pos];
                let file_part = &f[pos + 1..];
                (Some(suite_part.to_string()), Some(file_part.to_string()))
            } else {
                (Some(f.clone()), None)
            }
        }
    }
}

fn list_tests(
    root: &std::path::Path,
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
        suite_tests.push((suite, all_tests));
    }

    output.print_list(&suite_tests);
    Ok(())
}
