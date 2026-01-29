use crate::discover::Suite;
use crate::matcher::Matcher;
use crate::{parse_content, parse_file, TestCase};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};
use tempfile::TempDir;

#[derive(Debug, Clone)]
pub struct TestResult {
    pub test: TestCase,
    pub passed: bool,
    pub skipped: bool,
    pub skip_reason: Option<String>,
    pub actual_output: Option<String>,
    pub expected_output: String,
    pub error: Option<String>,
    pub elapsed: Duration,
    pub suite: String,
}

#[derive(Debug, Clone)]
pub struct FileResult {
    pub file_path: PathBuf,
    pub results: Vec<TestResult>,
    pub parse_error: Option<String>,
}

impl FileResult {
    pub fn passed(&self) -> bool {
        self.parse_error.is_none() && self.results.iter().all(|r| r.passed)
    }
}

#[derive(Debug)]
pub struct SuiteResult {
    pub suite: Suite,
    pub file_results: Vec<FileResult>,
    pub setup_error: Option<String>,
    pub elapsed: Duration,
}

impl SuiteResult {
    pub fn passed(&self) -> bool {
        self.setup_error.is_none() && self.file_results.iter().all(|f| f.passed())
    }

    pub fn total_tests(&self) -> usize {
        self.file_results.iter().map(|f| f.results.len()).sum()
    }

    pub fn passed_tests(&self) -> usize {
        self.file_results
            .iter()
            .flat_map(|f| &f.results)
            .filter(|r| r.passed)
            .count()
    }
}

#[derive(Debug, Clone)]
pub enum ProgressEvent {
    TestStart {
        suite: String,
        file: String,
        name: String,
    },
    TestComplete(Box<TestResult>),
    Skip {
        suite: String,
        reason: String,
    },
}

fn run_command(command: &str, work_dir: &Path, env_vars: &[(String, String)]) -> (String, i32) {
    let mut cmd = if cfg!(windows) {
        let mut c = Command::new("cmd");
        c.arg("/C").arg(command);
        c
    } else {
        let mut c = Command::new("bash");
        c.arg("-c").arg(command);
        c
    };
    cmd.current_dir(work_dir);

    for (key, value) in env_vars {
        cmd.env(key, value);
    }

    match cmd.output() {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let combined = format!("{}{}", stdout, stderr);
            let exit_code = output.status.code().unwrap_or(-1);
            // Normalize line endings (Windows uses \r\n) and trim trailing newlines
            let normalized = combined.replace("\r\n", "\n");
            (normalized.trim_end_matches('\n').to_string(), exit_code)
        }
        Err(e) => (format!("Failed to execute command: {}", e), -1),
    }
}

use crate::SkipDirective;
use cctr_corpus::{Platform, PlatformCondition};

fn is_current_platform(platform: Platform) -> bool {
    match platform {
        Platform::Windows => cfg!(windows),
        Platform::Unix => cfg!(unix),
        Platform::MacOS => cfg!(target_os = "macos"),
        Platform::Linux => cfg!(target_os = "linux"),
    }
}

fn should_skip_for_platform(condition: &PlatformCondition) -> bool {
    match condition {
        PlatformCondition::Is(p) => is_current_platform(*p),
        PlatformCondition::Not(p) => !is_current_platform(*p),
        PlatformCondition::Or(platforms) => platforms.iter().any(|p| is_current_platform(*p)),
    }
}

fn should_skip(
    skip: &SkipDirective,
    work_dir: &Path,
    env_vars: &[(String, String)],
) -> Option<String> {
    let debug = std::env::var("CCTR_DEBUG_SKIP").is_ok();

    // Check platform condition first
    if let Some(platform) = &skip.platform {
        let should_skip = should_skip_for_platform(platform);
        if debug {
            eprintln!(
                "[DEBUG SKIP] platform: {:?}, should_skip: {}, is_windows: {}",
                platform, should_skip, cfg!(windows)
            );
        }
        if should_skip {
            return Some(skip.message.clone().unwrap_or_else(|| "skipped".to_string()));
        } else {
            return None;
        }
    }

    // Check shell condition
    match &skip.condition {
        Some(condition) => {
            let (output, exit_code) = run_command(condition, work_dir, env_vars);
            if debug {
                eprintln!(
                    "[DEBUG SKIP] condition: {:?}, exit_code: {}, output: {:?}, is_windows: {}",
                    condition, exit_code, output, cfg!(windows)
                );
            }
            if exit_code == 0 {
                Some(
                    skip.message
                        .clone()
                        .unwrap_or_else(|| "skipped".to_string()),
                )
            } else {
                None
            }
        }
        None => Some(
            skip.message
                .clone()
                .unwrap_or_else(|| "skipped".to_string()),
        ),
    }
}

fn run_test(
    test: &TestCase,
    work_dir: &Path,
    suite_name: &str,
    env_vars: &[(String, String)],
) -> TestResult {
    let start = Instant::now();

    if let Some(skip) = &test.skip {
        if let Some(reason) = should_skip(skip, work_dir, env_vars) {
            return TestResult {
                test: test.clone(),
                passed: true,
                skipped: true,
                skip_reason: Some(reason),
                actual_output: None,
                expected_output: test.expected_output.clone(),
                error: None,
                elapsed: start.elapsed(),
                suite: suite_name.to_string(),
            };
        }
    }

    let (actual_output, exit_code) = run_command(&test.command, work_dir, env_vars);
    let elapsed = start.elapsed();

    let (passed, error, expected_output) = if test.variables.is_empty() {
        let expected = &test.expected_output;
        if expected.is_empty() {
            (exit_code == 0, None, expected.clone())
        } else {
            (actual_output == *expected, None, expected.clone())
        }
    } else {
        let matcher = Matcher::new(&test.variables, &test.constraints, env_vars);
        let result = match matcher.matches(&test.expected_output, &actual_output) {
            Ok(true) => (true, None),
            Ok(false) => (false, None),
            Err(e) => (false, Some(e.to_string())),
        };
        (result.0, result.1, test.expected_output.clone())
    };

    TestResult {
        test: test.clone(),
        passed,
        skipped: false,
        skip_reason: None,
        actual_output: Some(actual_output),
        expected_output,
        error,
        elapsed,
        suite: suite_name.to_string(),
    }
}

fn run_corpus_file(
    file_path: &Path,
    work_dir: &Path,
    suite_name: &str,
    env_vars: &[(String, String)],
    pattern: Option<&str>,
    progress_tx: Option<&Sender<ProgressEvent>>,
) -> FileResult {
    let corpus = match parse_file(file_path) {
        Ok(corpus) => corpus,
        Err(e) => {
            return FileResult {
                file_path: file_path.to_path_buf(),
                results: vec![],
                parse_error: Some(e.to_string()),
            };
        }
    };

    // Handle file-level skip directive
    if let Some(skip) = &corpus.file_skip {
        if let Some(reason) = should_skip(skip, work_dir, env_vars) {
            let file_stem = file_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            let mut results = Vec::new();
            for test in corpus.tests {
                if let Some(tx) = progress_tx {
                    let _ = tx.send(ProgressEvent::TestStart {
                        suite: suite_name.to_string(),
                        file: file_stem.clone(),
                        name: test.name.clone(),
                    });
                }
                let result = TestResult {
                    test: test.clone(),
                    passed: true,
                    skipped: true,
                    skip_reason: Some(reason.clone()),
                    actual_output: None,
                    expected_output: test.expected_output.clone(),
                    error: None,
                    elapsed: Duration::ZERO,
                    suite: suite_name.to_string(),
                };
                if let Some(tx) = progress_tx {
                    let _ = tx.send(ProgressEvent::TestComplete(Box::new(result.clone())));
                }
                results.push(result);
            }
            return FileResult {
                file_path: file_path.to_path_buf(),
                results,
                parse_error: None,
            };
        }
    }

    let mut results = Vec::new();

    // Check if file name matches the pattern (excluding .txt extension)
    let file_matches = pattern.is_none_or(|pat| {
        file_path
            .file_stem()
            .and_then(|s| s.to_str())
            .is_some_and(|name| name.contains(pat))
    });

    let file_stem = file_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string();

    for test in corpus.tests {
        if let Some(pat) = pattern {
            // Match if either the file name OR the test name contains the pattern
            if !file_matches && !test.name.contains(pat) {
                continue;
            }
        }

        if let Some(tx) = progress_tx {
            let _ = tx.send(ProgressEvent::TestStart {
                suite: suite_name.to_string(),
                file: file_stem.clone(),
                name: test.name.clone(),
            });
        }

        let result = run_test(&test, work_dir, suite_name, env_vars);
        if let Some(tx) = progress_tx {
            let _ = tx.send(ProgressEvent::TestComplete(Box::new(result.clone())));
        }
        results.push(result);
    }

    FileResult {
        file_path: file_path.to_path_buf(),
        results,
        parse_error: None,
    }
}

pub fn run_suite(
    suite: &Suite,
    pattern: Option<&str>,
    progress_tx: Option<&Sender<ProgressEvent>>,
) -> SuiteResult {
    let start = Instant::now();
    let mut file_results = Vec::new();
    let mut setup_error = None;

    let temp_dir = match TempDir::with_prefix(format!("cctr_{}_", suite.name.replace('/', "_"))) {
        Ok(d) => d,
        Err(e) => {
            return SuiteResult {
                suite: suite.clone(),
                file_results,
                setup_error: Some(format!("Failed to create temp dir: {}", e)),
                elapsed: start.elapsed(),
            };
        }
    };

    let work_dir = temp_dir
        .path()
        .canonicalize()
        .unwrap_or_else(|_| temp_dir.path().to_path_buf());
    let work_dir = work_dir.as_path();

    // Build environment variables to inject
    let test_path = suite
        .path
        .canonicalize()
        .unwrap_or_else(|_| suite.path.clone());
    let mut env_vars = vec![
        (
            "CCTR_WORK_DIR".to_string(),
            work_dir.to_string_lossy().to_string(),
        ),
        (
            "CCTR_TEST_PATH".to_string(),
            test_path.to_string_lossy().to_string(),
        ),
    ];

    if suite.has_fixture {
        let fixture_src = suite.path.join("fixture");
        if let Err(e) = copy_dir_recursive(&fixture_src, work_dir) {
            return SuiteResult {
                suite: suite.clone(),
                file_results,
                setup_error: Some(format!("Failed to copy fixture: {}", e)),
                elapsed: start.elapsed(),
            };
        }
        env_vars.push((
            "CCTR_FIXTURE_DIR".to_string(),
            work_dir.to_string_lossy().to_string(),
        ));
    }

    if suite.has_setup {
        let setup_file = suite.path.join("_setup.txt");
        let file_result = run_corpus_file(
            &setup_file,
            work_dir,
            &suite.name,
            &env_vars,
            None, // Setup always runs all tests regardless of pattern
            progress_tx,
        );
        let setup_passed = file_result.passed();
        file_results.push(file_result);

        if !setup_passed {
            setup_error = Some("Setup failed".to_string());
            return SuiteResult {
                suite: suite.clone(),
                file_results,
                setup_error,
                elapsed: start.elapsed(),
            };
        }
    }

    for corpus_file in suite.corpus_files() {
        let file_result = run_corpus_file(
            &corpus_file,
            work_dir,
            &suite.name,
            &env_vars,
            pattern,
            progress_tx,
        );
        file_results.push(file_result);
    }

    if suite.has_teardown {
        let teardown_file = suite.path.join("_teardown.txt");
        let file_result = run_corpus_file(
            &teardown_file,
            work_dir,
            &suite.name,
            &env_vars,
            None, // Teardown always runs all tests regardless of pattern
            progress_tx,
        );
        file_results.push(file_result);
    }

    SuiteResult {
        suite: suite.clone(),
        file_results,
        setup_error,
        elapsed: start.elapsed(),
    }
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    if !dst.exists() {
        std::fs::create_dir_all(dst)?;
    }

    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }

    Ok(())
}

pub fn run_from_stdin(content: &str, progress_tx: Option<&Sender<ProgressEvent>>) -> SuiteResult {
    let start = Instant::now();

    let stdin_path = PathBuf::from("<stdin>");
    let corpus = match parse_content(content, &stdin_path) {
        Ok(c) => c,
        Err(e) => {
            let suite = Suite {
                name: "stdin".to_string(),
                path: PathBuf::from("."),
                has_fixture: false,
                has_setup: false,
                has_teardown: false,
                single_file: None,
            };
            return SuiteResult {
                suite,
                file_results: vec![],
                setup_error: Some(format!("Failed to parse: {}", e)),
                elapsed: start.elapsed(),
            };
        }
    };

    let temp_dir = match TempDir::with_prefix("cctr_stdin_") {
        Ok(d) => d,
        Err(e) => {
            let suite = Suite {
                name: "stdin".to_string(),
                path: PathBuf::from("."),
                has_fixture: false,
                has_setup: false,
                has_teardown: false,
                single_file: None,
            };
            return SuiteResult {
                suite,
                file_results: vec![],
                setup_error: Some(format!("Failed to create temp dir: {}", e)),
                elapsed: start.elapsed(),
            };
        }
    };

    let work_dir = temp_dir
        .path()
        .canonicalize()
        .unwrap_or_else(|_| temp_dir.path().to_path_buf());

    let test_path = std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from("."));
    let env_vars = vec![
        (
            "CCTR_WORK_DIR".to_string(),
            work_dir.to_string_lossy().to_string(),
        ),
        (
            "CCTR_TEST_PATH".to_string(),
            test_path.to_string_lossy().to_string(),
        ),
    ];

    let mut results = Vec::new();
    for test in corpus.tests {
        if let Some(tx) = progress_tx {
            let _ = tx.send(ProgressEvent::TestStart {
                suite: "stdin".to_string(),
                file: "stdin".to_string(),
                name: test.name.clone(),
            });
        }

        let result = run_test(&test, &work_dir, "stdin", &env_vars);
        if let Some(tx) = progress_tx {
            let _ = tx.send(ProgressEvent::TestComplete(Box::new(result.clone())));
        }
        results.push(result);
    }

    let suite = Suite {
        name: "stdin".to_string(),
        path: PathBuf::from("."),
        has_fixture: false,
        has_setup: false,
        has_teardown: false,
        single_file: None,
    };

    SuiteResult {
        suite,
        file_results: vec![FileResult {
            file_path: stdin_path,
            results,
            parse_error: None,
        }],
        setup_error: None,
        elapsed: start.elapsed(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_suite(dir: &Path, name: &str) -> Suite {
        let suite_dir = dir.join(name);
        fs::create_dir_all(&suite_dir).unwrap();
        Suite::new(suite_dir, dir)
    }

    fn create_test_file(dir: &Path, content: &str) {
        fs::write(dir, content).unwrap();
    }

    #[test]
    fn test_run_simple_test() {
        let tmp = TempDir::new().unwrap();
        let suite = create_suite(tmp.path(), "simple");
        create_test_file(
            &suite.path.join("test.txt"),
            "===\necho test\n===\necho hello\n---\nhello\n",
        );

        let result = run_suite(&suite, None, None);
        assert!(result.passed());
        assert_eq!(result.total_tests(), 1);
        assert_eq!(result.passed_tests(), 1);
    }

    #[test]
    fn test_run_failing_test() {
        let tmp = TempDir::new().unwrap();
        let suite = create_suite(tmp.path(), "failing");
        create_test_file(
            &suite.path.join("test.txt"),
            "===\nfailing test\n===\necho wrong\n---\nexpected\n",
        );

        let result = run_suite(&suite, None, None);
        assert!(!result.passed());
        assert_eq!(result.passed_tests(), 0);
    }

    #[test]
    fn test_exit_only_mode() {
        let tmp = TempDir::new().unwrap();
        let suite = create_suite(tmp.path(), "exit_only");
        create_test_file(
            &suite.path.join("test.txt"),
            "===\nexit only\n===\ntrue\n---\n",
        );

        let result = run_suite(&suite, None, None);
        assert!(result.passed());
    }

    #[test]
    fn test_exit_only_failure() {
        let tmp = TempDir::new().unwrap();
        let suite = create_suite(tmp.path(), "exit_fail");
        create_test_file(
            &suite.path.join("test.txt"),
            "===\nexit only fail\n===\nfalse\n---\n",
        );

        let result = run_suite(&suite, None, None);
        assert!(!result.passed());
    }

    #[test]
    fn test_env_vars() {
        let tmp = TempDir::new().unwrap();
        let suite = create_suite(tmp.path(), "envvars");
        create_test_file(
            &suite.path.join("test.txt"),
            "===\nenv var test\n===\necho $CCTR_WORK_DIR\n---\n",
        );

        let result = run_suite(&suite, None, None);
        // Just checks exit code 0 since expected is empty
        assert!(result.passed());
    }

    #[test]
    fn test_fixture_copy() {
        let tmp = TempDir::new().unwrap();
        let suite_dir = tmp.path().join("with_fixture");
        let fixture_dir = suite_dir.join("fixture");
        fs::create_dir_all(&fixture_dir).unwrap();
        fs::write(fixture_dir.join("data.txt"), "fixture content").unwrap();
        create_test_file(
            &suite_dir.join("test.txt"),
            "===\nread fixture\n===\ncat data.txt\n---\nfixture content\n",
        );

        let suite = Suite::new(suite_dir, tmp.path());
        let result = run_suite(&suite, None, None);
        assert!(result.passed());
    }
}
