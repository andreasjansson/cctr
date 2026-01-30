use crate::runner::{ProgressEvent, SuiteResult, TestResult};
use similar::{ChangeTag, TextDiff};
use std::io::Write;
use std::time::Duration;
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

pub struct Output {
    stdout: StandardStream,
    dot_count: usize,
}

impl Output {
    pub fn new(color: bool) -> Self {
        let color_choice = if color {
            ColorChoice::Auto
        } else {
            ColorChoice::Never
        };
        Self {
            stdout: StandardStream::stdout(color_choice),
            dot_count: 0,
        }
    }

    fn set_color(&mut self, color: Color) {
        let _ = self.stdout.set_color(ColorSpec::new().set_fg(Some(color)));
    }

    fn set_bold(&mut self) {
        let _ = self.stdout.set_color(ColorSpec::new().set_bold(true));
    }

    fn set_dim(&mut self) {
        let _ = self.stdout.set_color(ColorSpec::new().set_dimmed(true));
    }

    fn reset(&mut self) {
        let _ = self.stdout.reset();
    }

    pub fn print_progress(&mut self, event: &ProgressEvent, verbose_level: u8, update_mode: bool) {
        match event {
            ProgressEvent::TestStart { suite, file, name } => {
                if verbose_level >= 1 {
                    self.set_dim();
                    writeln!(self.stdout, "starting {}/{}: {}", suite, file, name).unwrap();
                    self.reset();
                    let _ = self.stdout.flush();
                }
            }
            ProgressEvent::TestComplete(result) => {
                if verbose_level >= 1 {
                    self.print_verbose_result(result, update_mode);
                } else {
                    self.print_dot(result, update_mode);
                }
            }
            ProgressEvent::TestOutput { suite, file, name, line } => {
                if verbose_level >= 2 {
                    self.set_dim();
                    write!(self.stdout, "[{}/{}:{}] ", suite, file, name).unwrap();
                    self.reset();
                    writeln!(self.stdout, "{}", line).unwrap();
                    let _ = self.stdout.flush();
                }
            }
            ProgressEvent::Skip { suite, reason } => {
                if verbose_level >= 1 {
                    self.set_color(Color::Yellow);
                    write!(self.stdout, "S").unwrap();
                    self.reset();
                    writeln!(self.stdout, " {}: {}", suite, reason).unwrap();
                } else {
                    self.set_color(Color::Yellow);
                    write!(self.stdout, "S").unwrap();
                    self.reset();
                    let _ = self.stdout.flush();
                    self.dot_count += 1;
                    self.maybe_newline();
                }
            }
        }
    }

    fn print_dot(&mut self, result: &TestResult, update_mode: bool) {
        if result.skipped {
            self.set_color(Color::Yellow);
            write!(self.stdout, "s").unwrap();
        } else if result.passed {
            self.set_color(Color::Green);
            write!(self.stdout, ".").unwrap();
        } else if update_mode {
            self.set_color(Color::Cyan);
            write!(self.stdout, "U").unwrap();
        } else {
            self.set_color(Color::Red);
            write!(self.stdout, "F").unwrap();
        }
        self.reset();
        let _ = self.stdout.flush();

        self.dot_count += 1;
        self.maybe_newline();
    }

    fn maybe_newline(&mut self) {
        if self.dot_count >= 80 {
            writeln!(self.stdout).unwrap();
            self.dot_count = 0;
        }
    }

    fn print_verbose_result(&mut self, result: &TestResult, update_mode: bool) {
        if result.skipped {
            self.set_color(Color::Yellow);
            write!(self.stdout, "⊘").unwrap();
        } else if result.passed {
            self.set_color(Color::Green);
            write!(self.stdout, "✓").unwrap();
        } else if update_mode {
            self.set_color(Color::Cyan);
            write!(self.stdout, "↺").unwrap();
        } else {
            self.set_color(Color::Red);
            write!(self.stdout, "✗").unwrap();
        }
        self.reset();

        let file_stem = result
            .test
            .file_path
            .file_stem()
            .map(|s| s.to_string_lossy())
            .unwrap_or_default();

        write!(
            self.stdout,
            " {}/{}: {}",
            result.suite, file_stem, result.test.name
        )
        .unwrap();

        if result.skipped {
            self.set_color(Color::Yellow);
            if let Some(reason) = &result.skip_reason {
                writeln!(self.stdout, " ({})", reason).unwrap();
            } else {
                writeln!(self.stdout, " (skipped)").unwrap();
            }
            self.reset();
        } else {
            self.set_dim();
            writeln!(self.stdout, " {:.2}s", result.elapsed.as_secs_f64()).unwrap();
            self.reset();
        }

        // Print warning if present
        if let Some(warning) = &result.warning {
            self.set_color(Color::Yellow);
            writeln!(self.stdout, "  ⚠ Warning: {}", warning).unwrap();
            self.reset();
        }
    }

    pub fn finish_progress(&mut self) {
        if self.dot_count > 0 {
            writeln!(self.stdout).unwrap();
        }
        writeln!(self.stdout).unwrap();
    }

    pub fn print_results(&mut self, results: &[SuiteResult], elapsed: Duration, update_mode: bool) {
        let mut total_passed = 0;
        let mut total_failed = 0;
        let mut total_skipped = 0;
        let mut failed_tests: Vec<&TestResult> = Vec::new();
        let mut parse_errors: Vec<(&std::path::Path, &str)> = Vec::new();

        let mut sorted_results: Vec<_> = results.iter().collect();
        sorted_results.sort_by(|a, b| a.suite.name.cmp(&b.suite.name));

        for suite_result in &sorted_results {
            if let Some(setup_error) = &suite_result.setup_error {
                let skipped_count = suite_result.suite.test_count();
                self.set_color(Color::Yellow);
                write!(self.stdout, "⊘ {}", suite_result.suite.name).unwrap();
                self.reset();
                writeln!(
                    self.stdout,
                    ": {} ({} tests skipped)",
                    setup_error, skipped_count
                )
                .unwrap();
                total_skipped += skipped_count;

                for file_result in &suite_result.file_results {
                    for result in &file_result.results {
                        if !result.passed {
                            failed_tests.push(result);
                        }
                    }
                }
                continue;
            }

            // Collect parse errors
            for file_result in &suite_result.file_results {
                if let Some(err) = &file_result.parse_error {
                    parse_errors.push((file_result.file_path.as_path(), err.as_str()));
                }
            }

            let suite_skipped: usize = suite_result
                .file_results
                .iter()
                .flat_map(|f| &f.results)
                .filter(|r| r.skipped)
                .count();
            let suite_passed = suite_result.passed_tests() - suite_skipped;
            let suite_total = suite_result.total_tests();
            let suite_failed = suite_total - suite_passed - suite_skipped;
            let has_parse_errors = suite_result
                .file_results
                .iter()
                .any(|f| f.parse_error.is_some());
            let suite_time = format!(" in {:.2}s", suite_result.elapsed.as_secs_f64());

            total_passed += suite_passed;
            total_failed += suite_failed;
            total_skipped += suite_skipped;
            if has_parse_errors {
                total_failed += 1; // Count parse error as a failure
            }

            let skip_info = if suite_skipped > 0 {
                format!(", {} skipped", suite_skipped)
            } else {
                String::new()
            };

            if suite_result.passed() && !has_parse_errors {
                self.set_color(Color::Green);
                write!(self.stdout, "✓ {}", suite_result.suite.name).unwrap();
                self.reset();
                writeln!(
                    self.stdout,
                    ": {}/{} tests passed{}{}",
                    suite_passed,
                    suite_total - suite_skipped,
                    suite_time,
                    skip_info
                )
                .unwrap();
            } else {
                if update_mode {
                    self.set_color(Color::Cyan);
                    write!(self.stdout, "↺ {}", suite_result.suite.name).unwrap();
                } else {
                    self.set_color(Color::Red);
                    write!(self.stdout, "✗ {}", suite_result.suite.name).unwrap();
                }
                self.reset();
                writeln!(
                    self.stdout,
                    ": {}/{} tests passed{}{}",
                    suite_passed,
                    suite_total - suite_skipped,
                    suite_time,
                    skip_info
                )
                .unwrap();

                for file_result in &suite_result.file_results {
                    for result in &file_result.results {
                        if !result.passed && !result.skipped {
                            failed_tests.push(result);
                        }
                    }
                }
            }
        }

        // Print parse errors first
        if !parse_errors.is_empty() {
            writeln!(self.stdout).unwrap();
            self.set_color(Color::Red);
            self.set_bold();
            writeln!(self.stdout, "Parse Errors:").unwrap();
            self.reset();

            for (path, error) in &parse_errors {
                writeln!(self.stdout).unwrap();
                self.set_color(Color::Red);
                write!(self.stdout, "✗").unwrap();
                self.reset();
                writeln!(self.stdout, " {}", path.display()).unwrap();
                writeln!(self.stdout, "  {}", error).unwrap();
            }
        }

        if !failed_tests.is_empty() {
            writeln!(self.stdout).unwrap();
            if update_mode {
                self.set_color(Color::Cyan);
                self.set_bold();
                writeln!(self.stdout, "Updated:").unwrap();
            } else {
                self.set_color(Color::Red);
                self.set_bold();
                writeln!(self.stdout, "Failures:").unwrap();
            }
            self.reset();

            for result in failed_tests {
                writeln!(self.stdout).unwrap();
                let file_stem = result
                    .test
                    .file_path
                    .file_stem()
                    .map(|s| s.to_string_lossy())
                    .unwrap_or_default();

                if update_mode {
                    self.set_color(Color::Cyan);
                    write!(self.stdout, "↺").unwrap();
                } else {
                    self.set_color(Color::Red);
                    write!(self.stdout, "✗").unwrap();
                }
                self.reset();
                writeln!(
                    self.stdout,
                    " {}/{}: {}",
                    result.suite, file_stem, result.test.name
                )
                .unwrap();

                // Print warning if present
                if let Some(warning) = &result.warning {
                    self.set_color(Color::Yellow);
                    writeln!(self.stdout, "  ⚠ Warning: {}", warning).unwrap();
                    self.reset();
                }

                if let Some(error) = &result.error {
                    writeln!(self.stdout, "  Error: {}", error).unwrap();
                } else if let Some(actual) = &result.actual_output {
                    let display_path = std::env::current_dir()
                        .ok()
                        .and_then(|cwd| result.test.file_path.strip_prefix(&cwd).ok())
                        .map(|p| p.to_path_buf())
                        .unwrap_or_else(|| result.test.file_path.clone());
                    writeln!(
                        self.stdout,
                        "  {}:{}",
                        display_path.display(),
                        result.test.start_line
                    )
                    .unwrap();
                    writeln!(self.stdout, "  Command: {}", result.test.command).unwrap();
                    writeln!(self.stdout).unwrap();
                    self.print_diff(&result.expected_output, actual);
                }
            }
        }

        writeln!(self.stdout).unwrap();
        let elapsed_str = format!(" in {:.2}s", elapsed.as_secs_f64());

        if total_failed == 0 && total_skipped == 0 {
            self.set_color(Color::Green);
            self.set_bold();
            write!(self.stdout, "All {} tests passed", total_passed).unwrap();
            self.reset();
            writeln!(self.stdout, "{}", elapsed_str).unwrap();
        } else {
            self.set_bold();
            write!(self.stdout, "Summary:").unwrap();
            self.reset();
            if update_mode {
                writeln!(
                    self.stdout,
                    " {} left unchanged, {} updated, {} skipped{}",
                    total_passed, total_failed, total_skipped, elapsed_str
                )
                .unwrap();
            } else {
                writeln!(
                    self.stdout,
                    " {} passed, {} failed, {} skipped{}",
                    total_passed, total_failed, total_skipped, elapsed_str
                )
                .unwrap();
            }
        }
    }

    pub fn print_diff(&mut self, expected: &str, actual: &str) {
        let diff = TextDiff::from_lines(expected, actual);

        for (idx, group) in diff.grouped_ops(3).iter().enumerate() {
            if idx > 0 {
                writeln!(self.stdout, "...").unwrap();
            }

            for op in group {
                for change in diff.iter_changes(op) {
                    let (sign, color) = match change.tag() {
                        ChangeTag::Delete => ("-", Color::Red),
                        ChangeTag::Insert => ("+", Color::Green),
                        ChangeTag::Equal => (" ", Color::White),
                    };

                    self.set_color(color);
                    write!(self.stdout, "{}{}", sign, change.value()).unwrap();
                    self.reset();
                    if change.missing_newline() {
                        writeln!(self.stdout).unwrap();
                    }
                }
            }
        }
    }

    pub fn print_list(&mut self, results: &[(&crate::discover::Suite, Vec<crate::TestCase>)]) {
        for (suite, tests_by_file) in results {
            let mut markers = Vec::new();
            if suite.has_fixture {
                markers.push("fixture");
            }
            if suite.has_setup {
                markers.push("setup");
            }
            if suite.has_teardown {
                markers.push("teardown");
            }
            let marker_str = if markers.is_empty() {
                String::new()
            } else {
                format!(" [{}]", markers.join(", "))
            };

            writeln!(self.stdout).unwrap();
            self.set_bold();
            write!(self.stdout, "{}", suite.name).unwrap();
            self.reset();
            writeln!(self.stdout, "{}", marker_str).unwrap();

            let mut files: std::collections::HashMap<&std::path::Path, Vec<&crate::TestCase>> =
                std::collections::HashMap::new();
            for test in tests_by_file {
                files
                    .entry(test.file_path.as_path())
                    .or_default()
                    .push(test);
            }

            let mut file_list: Vec<_> = files.into_iter().collect();
            file_list.sort_by_key(|(path, _)| *path);

            for (path, tests) in file_list {
                let stem = path
                    .file_stem()
                    .map(|s| s.to_string_lossy())
                    .unwrap_or_default();
                writeln!(self.stdout, "  {}: {} test(s)", stem, tests.len()).unwrap();
                for test in tests {
                    writeln!(self.stdout, "    - {}", test.name).unwrap();
                }
            }
        }
    }
}
