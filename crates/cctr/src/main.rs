use cctr::cli::Cli;
use cctr::discover::{discover_suites, Suite};
use cctr::output::Output;
use cctr::parse_file;
use cctr::runner::{
    is_in_teardown, is_interrupted, kill_all_child_groups, run_from_stdin, run_suite,
    set_interrupted, ProgressEvent, SuiteResult,
};
use cctr::update::update_corpus_file;
use clap::Parser;
use rayon::prelude::*;
use regex::Regex;
use std::io::Read;
use std::sync::mpsc;
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

/// A second signal only forces a quit if it arrives at least this long after
/// the first.
///
/// A single shutdown event can deliver the same signal more than once: GNU
/// `timeout` signals the process group *and* the monitored child directly, so
/// cctr sees two SIGTERMs microseconds apart. Treating that as "the user asked
/// twice" skips teardown, which is exactly when cleanup matters most. Nobody
/// can react to the first message and signal again this quickly, so anything
/// inside the window is a duplicate of the same request.
const FORCE_QUIT_GRACE: Duration = Duration::from_millis(500);

/// When the first shutdown signal was handled.
static FIRST_SIGNAL_AT: Mutex<Option<Instant>> = Mutex::new(None);

/// Decide what a newly arrived shutdown signal means, and record the first one.
fn classify_signal(now: Instant) -> SignalAction {
    let mut first = FIRST_SIGNAL_AT.lock().unwrap_or_else(|e| e.into_inner());
    let action = decide_signal_action(*first, now);
    if action == SignalAction::BeginShutdown {
        *first = Some(now);
    }
    action
}

/// The duplicate-collapsing rule, kept pure so it can be tested directly.
fn decide_signal_action(first_seen: Option<Instant>, now: Instant) -> SignalAction {
    match first_seen {
        None => SignalAction::BeginShutdown,
        Some(at) if now.duration_since(at) < FORCE_QUIT_GRACE => SignalAction::Duplicate,
        Some(_) => SignalAction::ForceQuit,
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum SignalAction {
    BeginShutdown,
    Duplicate,
    ForceQuit,
}

fn main() -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        unsafe {
            libc::signal(libc::SIGPIPE, libc::SIG_DFL);
        }
    }

    if let Err(e) = ctrlc::set_handler(move || {
        use std::io::Write;
        match classify_signal(Instant::now()) {
            SignalAction::Duplicate => {}
            SignalAction::ForceQuit => {
                let _ = writeln!(std::io::stderr(), "\nForce quit");
                // Test commands run in their own process groups, so they do not
                // die with us. Take their whole subtree down before leaving.
                kill_all_child_groups();
                std::process::exit(130);
            }
            SignalAction::BeginShutdown => {
                if is_in_teardown() {
                    let _ = writeln!(
                        std::io::stderr(),
                        "\nAttempted to interrupt during teardown (press Ctrl-C again to force quit)"
                    );
                } else {
                    let _ = writeln!(
                        std::io::stderr(),
                        "\nInterrupted - running teardown... (press Ctrl-C again to force quit)"
                    );
                }
                set_interrupted();
            }
        }
    }) {
        eprintln!("Warning: Could not set signal handler: {}", e);
    }

    let cli = Cli::parse();

    let use_color = !cli.no_color && atty::is(atty::Stream::Stdout);
    let mut output = Output::new(use_color);

    if cli.paths.len() == 1 && cli.paths[0].as_os_str() == "-" {
        return run_stdin_mode(&cli, &mut output);
    }

    let pattern = cli.pattern.as_deref().map(|p| {
        Regex::new(p).unwrap_or_else(|e| {
            eprintln!("Invalid pattern '{}': {}", p, e);
            std::process::exit(1);
        })
    });

    let suites = discover_all(&cli.paths)?;

    if cli.list {
        list_tests(&suites, pattern.as_ref(), &mut output)?;
        return Ok(());
    }

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

    let pattern_ref = pattern.as_ref();
    let stream_output = verbose_level >= 2;
    let results: Vec<SuiteResult> = if cli.sequential || suites.len() == 1 {
        suites
            .iter()
            .map(|suite| run_suite(suite, pattern_ref, Some(&progress_tx), stream_output))
            .collect()
    } else {
        suites
            .par_iter()
            .map(|suite| {
                let tx = progress_tx.clone();
                run_suite(suite, pattern_ref, Some(&tx), stream_output)
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

                if failed.is_empty() {
                    continue;
                }

                let updated = update_corpus_file(&file_result.file_path, &failed)?;
                if updated > 0 {
                    eprintln!(
                        "Updated {} test(s) in: {}",
                        updated,
                        file_result.file_path.display()
                    );
                }

                let skipped = failed.len() - updated;
                if skipped > 0 {
                    eprintln!(
                        "Skipped {} test(s) with variables or constraints in: {} (update these manually)",
                        skipped,
                        file_result.file_path.display()
                    );
                }
            }
        }
    }

    let elapsed = start_time.elapsed();
    output.print_results(&results, elapsed, cli.update, cli.verbose >= 1);

    let total_tests: usize = results.iter().map(|r| r.total_tests()).sum();
    let had_parse_error = results
        .iter()
        .flat_map(|r| &r.file_results)
        .any(|f| f.parse_error.is_some());

    // An interrupt can stop suites from starting at all; that is not the
    // same as there being no tests.
    if total_tests == 0 && !had_parse_error && !is_interrupted() {
        match cli.pattern.as_deref() {
            Some(p) => eprintln!("No tests matched pattern '{}'", p),
            None => eprintln!("No tests found"),
        }
        std::process::exit(1);
    }

    let all_passed = results.iter().all(|r| r.passed());

    std::process::exit(if all_passed { 0 } else { 1 });
}

fn discover_all(paths: &[std::path::PathBuf]) -> anyhow::Result<Vec<Suite>> {
    let mut all_suites = Vec::new();
    for path in paths {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());
        let suites = discover_suites(&canonical)?;
        all_suites.extend(suites);
    }
    // Deduplicate suites by path (in case overlapping dirs are given)
    all_suites.sort_by(|a, b| a.name.cmp(&b.name));
    all_suites.dedup_by(|a, b| a.path == b.path && a.single_file == b.single_file);
    Ok(all_suites)
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
    output.print_results(&results, elapsed, cli.update, cli.verbose >= 1);

    let all_passed = results.iter().all(|r| r.passed());

    std::process::exit(if all_passed { 0 } else { 1 });
}

fn list_tests(
    suites: &[Suite],
    pattern: Option<&Regex>,
    output: &mut Output,
) -> anyhow::Result<()> {
    let mut suite_tests = Vec::new();
    for suite in suites {
        let mut all_tests = Vec::new();
        for file in suite.corpus_files() {
            let corpus = parse_file(&file)?;

            let file_matches = pattern.is_none_or(|pat| {
                file.file_stem()
                    .and_then(|s| s.to_str())
                    .is_some_and(|name| pat.is_match(name))
            });

            let filtered: Vec<_> = if let Some(pat) = pattern {
                corpus
                    .tests
                    .into_iter()
                    .filter(|t| file_matches || pat.is_match(&t.name))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_signal_begins_shutdown() {
        let now = Instant::now();
        assert_eq!(decide_signal_action(None, now), SignalAction::BeginShutdown);
    }

    #[test]
    fn duplicate_delivery_of_the_same_event_is_collapsed() {
        // GNU timeout signals the process group and the child directly, so the
        // second delivery arrives essentially instantly.
        let first = Instant::now();
        let same_event = first + Duration::from_micros(50);
        assert_eq!(
            decide_signal_action(Some(first), same_event),
            SignalAction::Duplicate
        );
    }

    #[test]
    fn repeat_within_grace_window_is_collapsed() {
        let first = Instant::now();
        let inside = first + FORCE_QUIT_GRACE - Duration::from_millis(1);
        assert_eq!(
            decide_signal_action(Some(first), inside),
            SignalAction::Duplicate
        );
    }

    #[test]
    fn separate_signal_after_grace_window_forces_quit() {
        let first = Instant::now();
        let later = first + FORCE_QUIT_GRACE + Duration::from_millis(1);
        assert_eq!(
            decide_signal_action(Some(first), later),
            SignalAction::ForceQuit
        );
    }

    #[test]
    fn deliberate_second_press_forces_quit() {
        // A person reacting to the "press Ctrl-C again" message takes far
        // longer than the grace window.
        let first = Instant::now();
        let later = first + Duration::from_secs(2);
        assert_eq!(
            decide_signal_action(Some(first), later),
            SignalAction::ForceQuit
        );
    }
}
