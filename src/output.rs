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

    pub fn print_progress(&mut self, event: &ProgressEvent, verbose: bool) {
        match event {
            ProgressEvent::TestComplete(result) => {
                if verbose {
                    self.print_verbose_result(result);
                } else {
                    self.print_dot(result);
                }
            }
            ProgressEvent::Skip { suite, reason } => {
                if verbose {
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

    fn print_dot(&mut self, result: &TestResult) {
        if result.passed {
            self.set_color(Color::Green);
            write!(self.stdout, ".").unwrap();
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

    fn print_verbose_result(&mut self, result: &TestResult) {
        if result.passed {
            self.set_color(Color::Green);
            write!(self.stdout, "✓").unwrap();
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

        write!(self.stdout, " {}/{}: {} ", result.suite, file_stem, result.test.name).unwrap();

        self.set_dim();
        writeln!(self.stdout, "{:.2}s", result.elapsed.as_secs_f64()).unwrap();
        self.reset();
    }

    pub fn finish_progress(&mut self) {
        if self.dot_count > 0 {
            writeln!(self.stdout).unwrap();
        }
        writeln!(self.stdout).unwrap();
    }

    pub fn print_results(&mut self, results: &[SuiteResult], elapsed: Duration) {
        let mut total_passed = 0;
        let mut total_failed = 0;
        let mut total_skipped = 0;
        let mut failed_tests: Vec<&TestResult> = Vec::new();

        let mut sorted_results: Vec<_> = results.iter().collect();
        sorted_results.sort_by(|a, b| a.suite.name.cmp(&b.suite.name));

        for suite_result in &sorted_results {
            if suite_result.setup_error.is_some() {
                self.set_color(Color::Yellow);
                write!(self.stdout, "⊘ {}", suite_result.suite.name).unwrap();
                self.reset();
                writeln!(
                    self.stdout,
                    ": {}",
                    suite_result.setup_error.as_ref().unwrap()
                )
                .unwrap();
                total_skipped += 1;

                for file_result in &suite_result.file_results {
                    for result in &file_result.results {
                        if !result.passed {
                            failed_tests.push(result);
                        }
                    }
                }
                continue;
            }

            let suite_passed = suite_result.passed_tests();
            let suite_total = suite_result.total_tests();
            let suite_time = format!(" in {:.2}s", suite_result.elapsed.as_secs_f64());

            total_passed += suite_passed;
            total_failed += suite_total - suite_passed;

            if suite_result.passed() {
                self.set_color(Color::Green);
                write!(self.stdout, "✓ {}", suite_result.suite.name).unwrap();
                self.reset();
                writeln!(
                    self.stdout,
                    ": {}/{} tests passed{}",
                    suite_passed, suite_total, suite_time
                )
                .unwrap();
            } else {
                self.set_color(Color::Red);
                write!(self.stdout, "✗ {}", suite_result.suite.name).unwrap();
                self.reset();
                writeln!(
                    self.stdout,
                    ": {}/{} tests passed{}",
                    suite_passed, suite_total, suite_time
                )
                .unwrap();

                for file_result in &suite_result.file_results {
                    for result in &file_result.results {
                        if !result.passed {
                            failed_tests.push(result);
                        }
                    }
                }
            }
        }

        if !failed_tests.is_empty() {
            writeln!(self.stdout).unwrap();
            self.set_color(Color::Red);
            self.set_bold();
            writeln!(self.stdout, "Failures:").unwrap();
            self.reset();

            for result in failed_tests {
                writeln!(self.stdout).unwrap();
                let file_stem = result
                    .test
                    .file_path
                    .file_stem()
                    .map(|s| s.to_string_lossy())
                    .unwrap_or_default();

                self.set_color(Color::Red);
                write!(self.stdout, "✗").unwrap();
                self.reset();
                writeln!(
                    self.stdout,
                    " {}/{}: {}",
                    result.suite, file_stem, result.test.name
                )
                .unwrap();

                if let Some(error) = &result.error {
                    writeln!(self.stdout, "  Error: {}", error).unwrap();
                } else if let Some(actual) = &result.actual_output {
                    writeln!(
                        self.stdout,
                        "  {}:{}",
                        result.test.file_path.display(),
                        result.test.start_line
                    )
                    .unwrap();
                    writeln!(self.stdout, "  Command: {}", result.test.command).unwrap();
                    writeln!(self.stdout).unwrap();
                    self.print_diff(&result.test.expected_output, actual);
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
            writeln!(
                self.stdout,
                " {} passed, {} failed, {} skipped{}",
                total_passed, total_failed, total_skipped, elapsed_str
            )
            .unwrap();
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

    pub fn print_list(&mut self, results: &[(&crate::discover::Suite, Vec<crate::parse::TestCase>)]) {
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

            let mut files: std::collections::HashMap<&std::path::Path, Vec<&crate::parse::TestCase>> =
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
                let stem = path.file_stem().map(|s| s.to_string_lossy()).unwrap_or_default();
                writeln!(self.stdout, "  {}: {} test(s)", stem, tests.len()).unwrap();
                for test in tests {
                    writeln!(self.stdout, "    - {}", test.name).unwrap();
                }
            }
        }
    }
}
