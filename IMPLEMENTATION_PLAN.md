# CCTR - CLI Corpus Test Runner

A general-purpose command-line testing tool for corpus-style tests.

> Named after the Corpus Christi Terminal Railroad (CCTR), and because it runs CLI Corpus Tests.

## Overview

CCTR is a standalone test runner that discovers and executes corpus-style tests defined in `.txt` files. Tests within a directory (suite) run sequentially, but suites run in parallel. It supports fixture directories, setup/teardown, and template variables.

### Key Features

- **Corpus test format**: Simple `===`/`---` delimited test cases in `.txt` files
- **Suite-based organization**: Tests in the same directory run sequentially
- **Parallel execution**: Different suites run in parallel
- **Fixture support**: Copy fixture directories to temp dirs with `{{ FIXTURE_DIR }}` templating
- **Setup/teardown**: `_setup.txt` runs first, `_teardown.txt` always runs last
- **Exit-only mode**: Tests with empty expected output only check exit code
- **Update mode**: Automatically update expected outputs from actual results
- **Rich output**: Progress dots, colored diffs, timing information

## Architecture

### Crate Structure

Following ripgrep/fd's approach of a focused single-crate structure for simpler tools:

```
cctr/
├── Cargo.toml
├── Cargo.lock
├── src/
│   ├── main.rs           # Entry point, CLI setup
│   ├── lib.rs            # Library root, public API
│   ├── cli.rs            # CLI argument parsing (clap derive)
│   ├── config.rs         # Configuration handling
│   ├── discover.rs       # Test suite discovery
│   ├── parse.rs          # Corpus file parsing
│   ├── runner.rs         # Test execution
│   ├── template.rs       # Template variable substitution
│   ├── output.rs         # Output formatting (progress, results, diffs)
│   ├── update.rs         # Expected output updating
│   └── error.rs          # Error types
├── tests/
│   └── integration.rs    # Integration tests
└── corpus/               # Self-hosted corpus tests for cctr itself
    └── basic/
        ├── _setup.txt
        ├── simple.txt
        └── fixture/
```

### Module Responsibilities

#### `cli.rs`
CLI argument parsing using clap with derive macros (like fd, just).

```rust
use clap::Parser;

#[derive(Parser)]
#[command(name = "cctr", about = "CLI Corpus Test Runner")]
pub struct Cli {
    /// Filter by suite or suite/file pattern
    pub filter: Option<String>,
    
    /// Update expected outputs from actual results
    #[arg(short, long)]
    pub update: bool,
    
    /// List all available tests
    #[arg(short, long)]
    pub list: bool,
    
    /// Verbose output (show each test as it completes)
    #[arg(short, long)]
    pub verbose: bool,
    
    /// Run suites sequentially instead of in parallel
    #[arg(short, long)]
    pub sequential: bool,
    
    /// Disable colored output
    #[arg(long)]
    pub no_color: bool,
    
    /// Root directory for test discovery (default: current directory)
    #[arg(short = 'C', long, default_value = ".")]
    pub root: PathBuf,
}
```

#### `discover.rs`
Recursive test suite discovery. A suite is any directory containing `.txt` files (excluding `fixture/` subdirectories).

```rust
/// Discover all test suites under a root directory
pub fn discover_suites(root: &Path) -> Result<Vec<Suite>>

/// A test suite (directory containing .txt test files)
#[refined_by(path: Path)]
pub struct Suite {
    pub path: PathBuf,
    pub has_fixture: bool,
    pub has_setup: bool,
    pub has_teardown: bool,
}
```

#### `parse.rs`
Parse corpus test files into structured test cases.

```rust
/// A single test case parsed from a corpus file
#[refined_by(name: str, start_line: int, end_line: int)]
#[invariant(start_line > 0 && start_line <= end_line)]
pub struct TestCase {
    pub name: String,
    pub command: String,
    pub expected_output: String,
    pub file_path: PathBuf,
    #[field(usize{v: v > 0})]
    pub start_line: usize,
    #[field(usize{v: v >= start_line})]
    pub end_line: usize,
}

/// Parse a corpus file into test cases
pub fn parse_corpus_file(path: &Path) -> Result<Vec<TestCase>>
```

#### `template.rs`
Template variable substitution for `{{ VAR }}` patterns.

```rust
/// Template variables available during test execution
pub struct TemplateVars {
    vars: HashMap<String, String>,
}

impl TemplateVars {
    pub fn new() -> Self;
    pub fn set(&mut self, key: &str, value: &str);
    pub fn apply(&self, text: &str) -> String;
}
```

#### `runner.rs`
Test execution engine. Handles fixture copying, environment setup, command execution.

```rust
/// Result of running a single test
pub struct TestResult {
    pub test: TestCase,
    pub passed: bool,
    pub actual_output: Option<String>,
    pub error: Option<String>,
    pub elapsed: Duration,
}

/// Result of running a suite
pub struct SuiteResult {
    pub suite: Suite,
    pub results: Vec<TestResult>,
    pub setup_failed: bool,
    pub elapsed: Duration,
}

/// Run all tests in a suite
pub fn run_suite(suite: &Suite, filter: Option<&str>) -> Result<SuiteResult>

/// Run a single test case
pub fn run_test(test: &TestCase, work_dir: &Path, vars: &TemplateVars) -> TestResult
```

#### `output.rs`
Output formatting with colors, progress indicators, diffs.

```rust
/// Print progress dot/F/S for a test result
pub fn print_progress(result: &TestResult, verbose: bool);

/// Print verbose test result line
pub fn print_verbose_result(result: &TestResult);

/// Print final summary with per-suite breakdown
pub fn print_summary(results: &[SuiteResult], elapsed: Duration);

/// Print colored unified diff
pub fn print_diff(expected: &str, actual: &str);
```

#### `update.rs`
Update corpus files with actual outputs.

```rust
/// Update expected outputs in a corpus file
pub fn update_corpus_file(path: &Path, results: &[TestResult]) -> Result<()>
```

#### `error.rs`
Error types using `thiserror` (like fd, uv).

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Failed to read corpus file: {path}")]
    ReadCorpus { path: PathBuf, source: io::Error },
    
    #[error("Failed to parse corpus file: {path}: {message}")]
    ParseCorpus { path: PathBuf, message: String },
    
    #[error("Command execution failed: {0}")]
    CommandFailed(String),
    
    #[error("Fixture copy failed: {0}")]
    FixtureCopy(#[from] io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
```

## Flux Refinement Types

CCTR will use Flux for compile-time verification of key invariants:

### Line Number Invariants

```rust
#[refined_by(start: int, end: int)]
#[invariant(start > 0 && start <= end)]
pub struct LineRange {
    #[field(usize[start])]
    start: usize,
    #[field(usize[end])]  
    end: usize,
}
```

### Non-Empty Collections

```rust
#[spec(fn(tests: Vec<TestCase>{v: v.len() > 0}) -> ...)]
pub fn run_tests(tests: Vec<TestCase>) -> ...
```

### Exit Code Validation

```rust
#[refined_by(code: int)]
#[invariant(code >= 0 && code <= 255)]
pub struct ExitCode {
    #[field(i32{v: 0 <= v && v <= 255})]
    code: i32,
}
```

### Path Safety

```rust
// Ensure paths are within expected directories
#[spec(fn(base: &Path, relative: &Path{v: !v.is_absolute()}) -> PathBuf)]
pub fn safe_join(base: &Path, relative: &Path) -> PathBuf {
    base.join(relative)
}
```

## Dependencies

Following the dependency choices of ripgrep/fd/just:

```toml
[dependencies]
# CLI
clap = { version = "4", features = ["derive", "wrap_help", "color"] }

# Error handling
thiserror = "1"
anyhow = "1"

# Output
termcolor = "1"          # Cross-platform colored output (like ripgrep)
similar = "2"            # Diff generation (like just)

# Parallelism
rayon = "1"              # Parallel iterators for suite execution

# File system
walkdir = "2"            # Directory traversal
tempfile = "3"           # Temp directories for fixtures

# Regex (for corpus parsing)
regex = "1"

# Flux refinement types
flux-rs = { git = "https://github.com/flux-rs/flux.git" }
```

## Corpus Test Format

The test format matches the Python implementation:

```
===
test name
===
command to run
---
expected output (can be multi-line)

===
another test
===
another command
---
another expected output
```

### Special Files

- `_setup.txt`: Runs first in a suite. If any test fails, remaining tests are skipped.
- `_teardown.txt`: Always runs last, regardless of success/failure.
- `fixture/`: If present, copied to temp directory. `{{ FIXTURE_DIR }}` available in tests.

### Template Variables

- `{{ FIXTURE_DIR }}`: Path to the copied fixture directory (only if fixture/ exists)
- `{{ WORK_DIR }}`: Path to the working directory (always a temp dir)

### Exit-Only Mode

Tests with empty expected output only check for exit code 0:

```
===
setup workspace
===
some-command --init
---
```

## CLI Interface

```
cctr - CLI Corpus Test Runner

USAGE:
    cctr [OPTIONS] [FILTER]

ARGS:
    <FILTER>    Filter by suite or suite/file (e.g., "languages/python" or "languages/python/grep")

OPTIONS:
    -u, --update        Update expected outputs from actual results
    -l, --list          List all available tests
    -v, --verbose       Show each test as it completes with timing
    -s, --sequential    Run suites sequentially instead of in parallel
    -C, --root <DIR>    Root directory for test discovery [default: .]
        --no-color      Disable colored output
    -h, --help          Print help
    -V, --version       Print version
```

### Output Examples

**Normal mode (dots):**
```
..F....S...............................

✓ languages/python: 45/45 tests passed in 5.83s
✓ languages/go: 42/42 tests passed in 8.91s
✗ languages/rust: 44/45 tests passed in 14.32s

Failures:

✗ languages/rust/rename: rename across modules
  /path/to/rename.txt:15
  Command: cctr rename OldName NewName
  
--- expected
+++ actual
@@ -1 +1 @@
-Renamed 3 occurrences
+Renamed 2 occurrences

Summary: 131 passed, 1 failed, 1 skipped in 14.32s
```

**Verbose mode:**
```
✓ languages/python/_setup: setup workspace 0.45s
✓ languages/python/grep_all: grep all symbols 0.12s
✓ languages/python/grep_kind: grep with kind filter 0.08s
✗ languages/python/rename: rename symbol 0.23s
...
```

## Implementation Phases

### Phase 1: Core Infrastructure
1. Set up Cargo workspace with Flux integration
2. Implement `cli.rs` with clap
3. Implement `error.rs` with thiserror
4. Implement `parse.rs` for corpus file parsing

### Phase 2: Test Discovery and Execution
1. Implement `discover.rs` for suite discovery
2. Implement `template.rs` for variable substitution
3. Implement `runner.rs` for test execution
4. Add fixture directory handling

### Phase 3: Output and Reporting
1. Implement `output.rs` with colored output
2. Add progress dots and verbose mode
3. Implement diff display with `similar`
4. Add timing information

### Phase 4: Advanced Features
1. Implement `update.rs` for expected output updating
2. Add parallel execution with rayon
3. Implement setup/teardown handling
4. Add filtering support

### Phase 5: Polish and Testing
1. Write integration tests using cctr's own corpus format
2. Add comprehensive error messages
3. Performance optimization
4. Documentation and examples

## Testing Strategy

CCTR will be tested using its own corpus test format (dogfooding):

```
corpus/
├── basic/
│   ├── _setup.txt
│   ├── simple_pass.txt
│   ├── simple_fail.txt
│   └── fixture/
│       └── sample.txt
├── templates/
│   ├── fixture_dir.txt
│   └── work_dir.txt
├── setup_teardown/
│   ├── _setup.txt
│   ├── _teardown.txt
│   └── middle.txt
└── exit_only/
    └── no_expected.txt
```

## Inspirations and Patterns

### From ripgrep
- Modular crate structure (though simpler for cctr)
- Comprehensive CLI with good defaults
- Performance-focused design
- Cross-platform support

### From fd
- Clean, focused single-binary design
- Excellent default behavior
- Simple but complete CLI interface
- Good use of clap derive

### From bat
- Beautiful colored output
- Progress indicators
- User-friendly error messages

### From just
- Task runner paradigm (tests as tasks)
- Simple configuration
- Good test organization

### From uv
- Modern Rust practices
- Excellent error handling with thiserror
- Fast parallel execution
- Good CLI UX

## Future Enhancements

1. **Watch mode**: Re-run tests on file changes
2. **Test filtering by name**: `cctr -k "grep"` to run tests matching pattern
3. **Timeout configuration**: Per-test or global timeouts
4. **Custom template variables**: Define in config file
5. **JUnit XML output**: For CI integration
6. **TAP output**: Test Anything Protocol support
7. **Snapshot testing**: First-class snapshot support beyond update mode
