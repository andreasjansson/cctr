//! Corpus test file parser.
//!
//! Parses `.txt` corpus test files into structured test cases using winnow.
//!
//! # File Format
//!
//! ```text
//! ===
//! test name
//! ===
//! command to run
//! ---
//! expected output
//!
//! ===
//! test with variables
//! ===
//! some_command
//! ---
//! Completed in {{ time: number }}s
//! ---
//! where
//! * time > 0
//! * time < 60
//! ```
//!
//! ## Skip Directives
//!
//! Tests can be conditionally skipped using `%skip` directives:
//!
//! ```text
//! %skip                           # unconditional skip
//! %skip(not yet implemented)      # unconditional skip with message
//! %skip if: test "$OS" = "Win"    # conditional skip
//! %skip(unix only) if: test ...   # conditional skip with message
//! ```
//!
//! File-level skips go at the top of the file before any tests.
//! Test-level skips go after the test name, before the closing `===`.

use std::path::{Path, PathBuf};
use thiserror::Error;
use winnow::combinator::{alt, opt, repeat};
use winnow::error::ContextError;
use winnow::prelude::*;
use winnow::token::{take_till, take_while};

// ============ Data Types ============

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VarType {
    Number,
    String,
    JsonString,
    JsonBool,
    JsonArray,
    JsonObject,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VariableDecl {
    pub name: String,
    pub var_type: Option<VarType>,
}

/// Skip directive - unconditional or conditional (with shell command)
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SkipDirective {
    pub message: Option<String>,
    /// Shell command condition - if exits 0, test is skipped
    pub condition: Option<String>,
}

/// Expected exit code for a test
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ExpectedExit {
    /// Command must exit with code 0 (default)
    #[default]
    Success,
    /// Command must exit with a specific code
    Code(i32),
    /// Command must exit with any non-zero code
    NonZero,
}

/// Supported platforms
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Platform {
    Windows,
    Unix,
    MacOS,
    Linux,
}

/// Shell to use for running commands.
/// Default: bash on Unix, powershell on Windows
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Shell {
    /// Bourne shell (sh)
    Sh,
    /// Bash shell (default on Unix)
    Bash,
    /// Zsh shell
    Zsh,
    /// PowerShell (default on Windows)
    PowerShell,
    /// Windows cmd.exe
    Cmd,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TestCase {
    pub name: String,
    pub command: String,
    /// Expected output - None means no output checking (exit-only test without `---` separator)
    pub expected_output: Option<String>,
    pub file_path: PathBuf,
    pub start_line: usize,
    pub end_line: usize,
    pub variables: Vec<VariableDecl>,
    pub constraints: Vec<String>,
    pub skip: Option<SkipDirective>,
    /// If true and this test fails, skip remaining tests in the file
    pub require: bool,
    /// Expected exit code (default: must be 0)
    pub expected_exit: ExpectedExit,
}

impl TestCase {
    pub fn variable_names(&self) -> Vec<&str> {
        self.variables.iter().map(|v| v.name.as_str()).collect()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CorpusFile {
    pub file_skip: Option<SkipDirective>,
    pub file_shell: Option<Shell>,
    pub file_platform: Vec<Platform>,
    pub tests: Vec<TestCase>,
}

#[derive(Error, Debug)]
pub enum ParseError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("parse error at line {line}: {message}")]
    Parse { line: usize, message: String },
}

// ============ Public API ============

pub fn parse_file(path: &Path) -> Result<CorpusFile, ParseError> {
    let content = std::fs::read_to_string(path)?;
    parse_content(&content, path)
}

pub fn parse_content(content: &str, path: &Path) -> Result<CorpusFile, ParseError> {
    let mut state = ParseState::new(content, path);
    match corpus_file(&mut state) {
        Ok(file) => {
            // Validate shell/platform compatibility
            if let Some(shell) = file.file_shell {
                if !file.file_platform.is_empty() {
                    validate_shell_platform(shell, &file.file_platform)?;
                }
            }
            Ok(file)
        }
        Err(_) => Err(ParseError::Parse {
            line: state.current_line,
            message: state
                .error_message
                .unwrap_or_else(|| "failed to parse corpus file".to_string()),
        }),
    }
}

/// Validate that the shell is compatible with the specified platforms
fn validate_shell_platform(shell: Shell, platforms: &[Platform]) -> Result<(), ParseError> {
    let is_windows_shell = matches!(shell, Shell::PowerShell | Shell::Cmd);

    let has_windows = platforms.contains(&Platform::Windows);
    let has_unix = platforms
        .iter()
        .any(|p| matches!(p, Platform::Unix | Platform::MacOS | Platform::Linux));

    // Windows-only shells can't run on Unix platforms
    if is_windows_shell && has_unix && !has_windows {
        return Err(ParseError::Parse {
            line: 1,
            message: format!(
                "shell '{:?}' is not compatible with platforms {:?}",
                shell, platforms
            ),
        });
    }

    // Unix-only shells (sh, zsh) can't run on Windows
    if matches!(shell, Shell::Sh | Shell::Zsh) && has_windows && !has_unix {
        return Err(ParseError::Parse {
            line: 1,
            message: format!(
                "shell '{:?}' is not compatible with platforms {:?}",
                shell, platforms
            ),
        });
    }

    // cmd is Windows-only
    if shell == Shell::Cmd && has_unix && !has_windows {
        return Err(ParseError::Parse {
            line: 1,
            message: format!(
                "shell 'cmd' is only available on Windows, but platforms are {:?}",
                platforms
            ),
        });
    }

    Ok(())
}

// ============ Parse State ============

struct ParseState<'a> {
    input: &'a str,
    path: &'a Path,
    current_line: usize,
    delimiter_len: usize,
    error_message: Option<String>,
}

impl<'a> ParseState<'a> {
    fn new(input: &'a str, path: &'a Path) -> Self {
        Self {
            input,
            path,
            current_line: 1,
            delimiter_len: 3,
            error_message: None,
        }
    }
}

// ============ Type Annotation Parsing ============

fn parse_type_annotation(type_str: &str) -> Option<VarType> {
    match type_str.to_lowercase().as_str() {
        "number" => Some(VarType::Number),
        "string" => Some(VarType::String),
        "json string" => Some(VarType::JsonString),
        "json bool" => Some(VarType::JsonBool),
        "json array" => Some(VarType::JsonArray),
        "json object" => Some(VarType::JsonObject),
        _ => None,
    }
}

const RESERVED_KEYWORDS: &[&str] = &[
    "true",
    "false",
    "null",
    "and",
    "or",
    "not",
    "in",
    "forall",
    "contains",
    "startswith",
    "endswith",
    "matches",
    "len",
    "type",
    "keys",
    "values",
    "sum",
    "min",
    "max",
    "abs",
    "unique",
    "lower",
    "upper",
    "number",
    "string",
    "bool",
    "array",
    "object",
    "env",
];

fn is_reserved_keyword(name: &str) -> bool {
    RESERVED_KEYWORDS.contains(&name)
}

fn parse_placeholder(content: &str) -> Result<(String, Option<VarType>), String> {
    let content = content.trim();
    let (name, var_type) = if let Some(colon_pos) = content.find(':') {
        let name = content[..colon_pos].trim().to_string();
        let type_str = content[colon_pos + 1..].trim();
        (name, parse_type_annotation(type_str))
    } else {
        (content.to_string(), None)
    };

    if is_reserved_keyword(&name) {
        return Err(format!(
            "'{}' is a reserved keyword and cannot be used as a variable name",
            name
        ));
    }

    Ok((name, var_type))
}

fn extract_variables_from_expected(expected: &str) -> Result<Vec<VariableDecl>, String> {
    let mut variables = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut remaining = expected;

    while let Some(start) = remaining.find("{{") {
        if let Some(end) = remaining[start..].find("}}") {
            let content = &remaining[start + 2..start + end];
            let (name, var_type) = parse_placeholder(content)?;
            if !name.is_empty() && seen.insert(name.clone()) {
                variables.push(VariableDecl { name, var_type });
            }
            remaining = &remaining[start + end + 2..];
        } else {
            break;
        }
    }

    Ok(variables)
}

// ============ Winnow Parsers ============

fn header_sep(input: &mut &str) -> ModalResult<usize> {
    let line: &str = take_while(1.., '=').parse_next(input)?;
    if line.len() >= 3 {
        Ok(line.len())
    } else {
        Err(winnow::error::ErrMode::Backtrack(ContextError::new()))
    }
}

fn check_header_sep_exact(line: &str, expected_len: usize) -> Option<String> {
    let trimmed = line.trim();
    if trimmed.chars().all(|c| c == '=') && trimmed.len() >= 3 && trimmed.len() != expected_len {
        Some(format!(
            "delimiter length mismatch: expected {} '=' characters but found {}",
            expected_len,
            trimmed.len()
        ))
    } else {
        None
    }
}

fn header_sep_exact(input: &mut &str, len: usize) -> ModalResult<()> {
    let line: &str = take_while(1.., '=').parse_next(input)?;
    if line.len() == len {
        Ok(())
    } else {
        Err(winnow::error::ErrMode::Backtrack(ContextError::new()))
    }
}

fn dash_sep_exact(input: &mut &str, len: usize) -> ModalResult<()> {
    let line: &str = take_while(1.., '-').parse_next(input)?;
    if line.len() == len {
        Ok(())
    } else {
        Err(winnow::error::ErrMode::Backtrack(ContextError::new()))
    }
}

fn line_content<'a>(input: &mut &'a str) -> ModalResult<&'a str> {
    take_till(0.., |c| c == '\n' || c == '\r').parse_next(input)
}

fn newline(input: &mut &str) -> ModalResult<()> {
    alt(("\r\n".value(()), "\n".value(()), "\r".value(()))).parse_next(input)
}

fn opt_newline(input: &mut &str) -> ModalResult<()> {
    opt(newline).map(|_| ()).parse_next(input)
}

fn blank_line(input: &mut &str) -> ModalResult<()> {
    (take_while(0.., ' '), newline)
        .map(|_| ())
        .parse_next(input)
}

fn skip_blank_lines(input: &mut &str) -> ModalResult<()> {
    repeat(0.., blank_line)
        .map(|_: Vec<()>| ())
        .parse_next(input)
}

fn is_any_separator_line(line: &str) -> bool {
    let trimmed = line.trim();
    (trimmed.len() >= 3 && trimmed.chars().all(|c| c == '='))
        || (trimmed.len() >= 3 && trimmed.chars().all(|c| c == '-'))
}

// ============ Skip Directive Parser ============

fn skip_message(input: &mut &str) -> ModalResult<String> {
    '('.parse_next(input)?;
    let msg: &str = take_till(0.., ')').parse_next(input)?;
    ')'.parse_next(input)?;
    Ok(msg.to_string())
}

fn skip_condition(input: &mut &str) -> ModalResult<String> {
    let _ = take_while(0.., ' ').parse_next(input)?;
    "if:".parse_next(input)?;
    let _ = take_while(0.., ' ').parse_next(input)?;
    let condition = line_content.parse_next(input)?;
    Ok(condition.trim().to_string())
}

fn platform_name(input: &mut &str) -> ModalResult<Platform> {
    let name: &str = take_while(1.., |c: char| c.is_ascii_alphanumeric()).parse_next(input)?;
    match name.to_lowercase().as_str() {
        "windows" => Ok(Platform::Windows),
        "unix" => Ok(Platform::Unix),
        "macos" => Ok(Platform::MacOS),
        "linux" => Ok(Platform::Linux),
        _ => Err(winnow::error::ErrMode::Backtrack(ContextError::new())),
    }
}

/// Parse %platform directive with comma-separated platforms
/// e.g., %platform windows or %platform unix
fn platform_directive(input: &mut &str) -> ModalResult<Vec<Platform>> {
    "%platform".parse_next(input)?;
    let _ = take_while(0.., ' ').parse_next(input)?;

    let mut platforms = Vec::new();

    // Parse first platform (required)
    let first = platform_name.parse_next(input)?;
    platforms.push(first);

    // Parse additional comma-separated platforms
    loop {
        let _ = take_while(0.., ' ').parse_next(input)?;
        if opt(',').parse_next(input)?.is_none() {
            break;
        }
        let _ = take_while(0.., ' ').parse_next(input)?;
        let platform = platform_name.parse_next(input)?;
        platforms.push(platform);
    }

    let _ = line_content.parse_next(input)?;
    opt_newline.parse_next(input)?;

    Ok(platforms)
}

fn skip_directive(input: &mut &str) -> ModalResult<SkipDirective> {
    "%skip".parse_next(input)?;
    let message = opt(skip_message).parse_next(input)?;
    let condition = opt(skip_condition).parse_next(input)?;

    if message.is_none() && condition.is_none() {
        let _ = line_content.parse_next(input)?;
    }

    opt_newline.parse_next(input)?;

    Ok(SkipDirective { message, condition })
}

// ============ Shell Directive Parser ============

fn shell_name(input: &mut &str) -> ModalResult<Shell> {
    let name: &str = take_while(1.., |c: char| c.is_ascii_alphanumeric()).parse_next(input)?;
    match name.to_lowercase().as_str() {
        "sh" => Ok(Shell::Sh),
        "bash" => Ok(Shell::Bash),
        "zsh" => Ok(Shell::Zsh),
        "powershell" => Ok(Shell::PowerShell),
        "cmd" => Ok(Shell::Cmd),
        _ => Err(winnow::error::ErrMode::Backtrack(ContextError::new())),
    }
}

fn shell_directive(input: &mut &str) -> ModalResult<Shell> {
    "%shell".parse_next(input)?;
    let _ = take_while(0.., ' ').parse_next(input)?;
    let shell = shell_name.parse_next(input)?;
    let _ = line_content.parse_next(input)?;
    opt_newline.parse_next(input)?;
    Ok(shell)
}

// ============ Exit Directive Parser ============

/// Parse %exit directive - specifies expected exit code
/// %exit 1        - expect exit code 1
/// %exit nonzero  - expect any non-zero exit code
fn exit_directive(input: &mut &str) -> ModalResult<ExpectedExit> {
    "%exit".parse_next(input)?;
    let _ = take_while(1.., ' ').parse_next(input)?;
    let value: &str = take_while(1.., |c: char| c.is_ascii_alphanumeric()).parse_next(input)?;
    let _ = line_content.parse_next(input)?;
    opt_newline.parse_next(input)?;

    match value.to_lowercase().as_str() {
        "nonzero" => Ok(ExpectedExit::NonZero),
        s => {
            if let Ok(code) = s.parse::<i32>() {
                Ok(ExpectedExit::Code(code))
            } else {
                Err(winnow::error::ErrMode::Backtrack(ContextError::new()))
            }
        }
    }
}

// ============ Test Case Parser ============

fn description_line(input: &mut &str) -> ModalResult<String> {
    let content = line_content.parse_next(input)?;
    opt_newline.parse_next(input)?;
    Ok(content.trim().to_string())
}

fn read_block_until_separator(input: &mut &str, delimiter_len: usize) -> String {
    let mut lines = Vec::new();

    loop {
        if input.is_empty() {
            break;
        }

        let peek_line = input.lines().next().unwrap_or("");
        let trimmed = peek_line.trim();

        // Only exact-length separators terminate the block
        // Any other length (shorter or longer) is treated as content
        if is_any_separator_line(peek_line) && trimmed.len() == delimiter_len {
            break;
        }

        let line = line_content.parse_next(input).unwrap_or("");
        opt_newline.parse_next(input).ok();
        lines.push(line);
    }

    while lines.last().is_some_and(|s| s.trim().is_empty()) {
        lines.pop();
    }

    lines.join("\n")
}

fn constraint_line(input: &mut &str) -> ModalResult<String> {
    let _ = take_while(0.., ' ').parse_next(input)?;
    let _ = opt('*').parse_next(input)?;
    let _ = take_while(0.., ' ').parse_next(input)?;

    let content = line_content.parse_next(input)?;
    opt_newline.parse_next(input)?;

    let trimmed = content.trim();
    if trimmed.is_empty() || trimmed == "where" {
        Err(winnow::error::ErrMode::Backtrack(ContextError::new()))
    } else {
        Ok(trimmed.to_string())
    }
}

fn where_section(input: &mut &str, delimiter_len: usize) -> ModalResult<Vec<String>> {
    dash_sep_exact(input, delimiter_len)?;
    opt_newline.parse_next(input)?;

    let _ = take_while(0.., ' ').parse_next(input)?;
    "where".parse_next(input)?;
    opt_newline.parse_next(input)?;

    let constraints: Vec<String> = repeat(0.., constraint_line).parse_next(input)?;
    Ok(constraints)
}

// ============ Main Parsers ============

fn test_case(state: &mut ParseState) -> Result<TestCase, winnow::error::ErrMode<ContextError>> {
    let input = &mut state.input;

    skip_blank_lines.parse_next(input)?;

    let start_line = state.current_line;

    let delimiter_len = header_sep.parse_next(input)?;
    state.delimiter_len = delimiter_len;
    opt_newline.parse_next(input)?;
    state.current_line += 1;

    let name = description_line.parse_next(input)?;
    state.current_line += 1;

    // Parse test-level directives (%skip, %require, %exit allowed at test level)
    let mut skip = None;
    let mut require = false;
    let mut expected_exit = ExpectedExit::Success;

    loop {
        let _ = take_while(0.., ' ').parse_next(input)?;
        if input.starts_with("%skip") && skip.is_none() {
            skip = Some(skip_directive.parse_next(input)?);
            state.current_line += 1;
        } else if input.starts_with("%require") {
            "%require".parse_next(input)?;
            let _ = take_while(0.., ' ').parse_next(input)?;
            let _ = opt('\n').parse_next(input)?;
            require = true;
            state.current_line += 1;
        } else if input.starts_with("%exit") {
            expected_exit = exit_directive.parse_next(input)?;
            state.current_line += 1;
        } else {
            break;
        }
    }

    // Check for directives that are only allowed at file level
    let _ = take_while(0.., ' ').parse_next(input)?;
    if input.starts_with("%platform") {
        state.error_message =
            Some("%platform is only allowed at file level, not inside test headers".to_string());
        return Err(winnow::error::ErrMode::Backtrack(ContextError::new()));
    }
    if input.starts_with("%shell") {
        state.error_message =
            Some("%shell is only allowed at file level, not inside test headers".to_string());
        return Err(winnow::error::ErrMode::Backtrack(ContextError::new()));
    }

    if let Some(err) = input
        .lines()
        .next()
        .and_then(|l| check_header_sep_exact(l, delimiter_len))
    {
        state.error_message = Some(err);
        return Err(winnow::error::ErrMode::Backtrack(ContextError::new()));
    }
    header_sep_exact(input, delimiter_len)?;
    opt_newline.parse_next(input)?;
    state.current_line += 1;

    let command_start = state.current_line;
    let command = read_block_until_separator(input, delimiter_len);
    state.current_line = command_start + command.lines().count().max(1);

    // Check if there's a --- separator (optional - if missing, it's exit-only mode)
    let has_output_separator = opt(|i: &mut &str| dash_sep_exact(i, delimiter_len))
        .parse_next(input)?
        .is_some();

    let (expected_output, variables, constraints) = if has_output_separator {
        opt_newline.parse_next(input)?;
        state.current_line += 1;

        let expected_start = state.current_line;
        let expected_output = read_block_until_separator(input, delimiter_len);
        let expected_lines = expected_output.lines().count();
        state.current_line =
            expected_start + expected_lines.max(if expected_output.is_empty() { 0 } else { 1 });

        let constraints = opt(|i: &mut &str| where_section(i, delimiter_len))
            .parse_next(input)?
            .unwrap_or_default();
        if !constraints.is_empty() {
            state.current_line += 2 + constraints.len();
        }

        let variables = extract_variables_from_expected(&expected_output)
            .map_err(|_| winnow::error::ErrMode::Backtrack(ContextError::new()))?;

        (Some(expected_output), variables, constraints)
    } else {
        (None, Vec::new(), Vec::new())
    };

    skip_blank_lines.parse_next(input)?;

    let end_line = state.current_line;

    Ok(TestCase {
        name,
        command,
        expected_output,
        file_path: state.path.to_path_buf(),
        start_line,
        end_line,
        variables,
        constraints,
        skip,
        require,
        expected_exit,
    })
}

fn corpus_file(state: &mut ParseState) -> Result<CorpusFile, winnow::error::ErrMode<ContextError>> {
    let input = &mut state.input;

    skip_blank_lines.parse_next(input)?;

    // Parse file-level directives (skip, shell, platform can appear in any order)
    let mut file_skip = None;
    let mut file_shell = None;
    let mut file_platform = Vec::new();

    loop {
        let _ = take_while(0.., ' ').parse_next(input)?;
        if input.starts_with("%skip") && file_skip.is_none() {
            file_skip = Some(skip_directive.parse_next(input)?);
            state.current_line += 1;
            skip_blank_lines.parse_next(input)?;
        } else if input.starts_with("%shell") && file_shell.is_none() {
            file_shell = Some(shell_directive.parse_next(input)?);
            state.current_line += 1;
            skip_blank_lines.parse_next(input)?;
        } else if input.starts_with("%platform") && file_platform.is_empty() {
            file_platform = platform_directive.parse_next(input)?;
            state.current_line += 1;
            skip_blank_lines.parse_next(input)?;
        } else {
            break;
        }
    }

    let mut tests = Vec::new();

    while !state.input.is_empty() {
        let peeked = state.input.trim_start();
        if peeked.is_empty() {
            break;
        }

        if !peeked.starts_with("===") {
            break;
        }

        let tc = test_case(state)?;
        tests.push(tc);
    }

    Ok(CorpusFile {
        file_skip,
        file_shell,
        file_platform,
        tests,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn parse_test(content: &str) -> CorpusFile {
        parse_content(content, Path::new("<test>")).unwrap()
    }

    #[test]
    fn test_parse_single_test() {
        let content = r#"===
test name
===
echo hello
---
hello
"#;
        let file = parse_test(content);
        assert!(file.file_skip.is_none());
        assert!(file.file_shell.is_none());
        assert!(file.file_platform.is_empty());
        assert_eq!(file.tests.len(), 1);
        assert_eq!(file.tests[0].name, "test name");
        assert_eq!(file.tests[0].command, "echo hello");
        assert_eq!(file.tests[0].expected_output, "hello");
        assert!(file.tests[0].variables.is_empty());
        assert!(file.tests[0].constraints.is_empty());
        assert!(file.tests[0].skip.is_none());
    }

    #[test]
    fn test_parse_multiple_tests() {
        let content = r#"===
first test
===
echo first
---
first

===
second test
===
echo second
---
second
"#;
        let file = parse_test(content);
        assert_eq!(file.tests.len(), 2);
        assert_eq!(file.tests[0].name, "first test");
        assert_eq!(file.tests[1].name, "second test");
    }

    #[test]
    fn test_parse_multiline_output() {
        let content = r#"===
multiline test
===
echo -e "line1\nline2\nline3"
---
line1
line2
line3
"#;
        let file = parse_test(content);
        assert_eq!(file.tests.len(), 1);
        assert_eq!(file.tests[0].expected_output, "line1\nline2\nline3");
    }

    #[test]
    fn test_parse_empty_expected() {
        let content = r#"===
exit only test
===
true
---
"#;
        let file = parse_test(content);
        assert_eq!(file.tests.len(), 1);
        assert_eq!(file.tests[0].expected_output, "");
    }

    #[test]
    fn test_parse_with_inline_type() {
        let content = r#"===
timing test
===
time_command
---
Completed in {{ n: number }}s
"#;
        let file = parse_test(content);
        assert_eq!(file.tests.len(), 1);
        assert_eq!(
            file.tests[0].expected_output,
            "Completed in {{ n: number }}s"
        );
        assert_eq!(file.tests[0].variables.len(), 1);
        assert_eq!(file.tests[0].variables[0].name, "n");
        assert_eq!(file.tests[0].variables[0].var_type, Some(VarType::Number));
    }

    #[test]
    fn test_parse_with_constraints() {
        let content = r#"===
timing test
===
time_command
---
Completed in {{ n: number }}s
---
where
* n > 0
* n < 60
"#;
        let file = parse_test(content);
        assert_eq!(file.tests.len(), 1);
        assert_eq!(file.tests[0].variables.len(), 1);
        assert_eq!(file.tests[0].constraints.len(), 2);
        assert_eq!(file.tests[0].constraints[0], "n > 0");
        assert_eq!(file.tests[0].constraints[1], "n < 60");
    }

    #[test]
    fn test_parse_multiple_variables() {
        let content = r#"===
multi var test
===
some_command
---
{{ count: number }} items in {{ time: number }}s: {{ msg: string }}
---
where
* count > 0
* time < 10
"#;
        let file = parse_test(content);
        assert_eq!(file.tests.len(), 1);
        assert_eq!(file.tests[0].variables.len(), 3);
        assert_eq!(file.tests[0].variables[0].name, "count");
        assert_eq!(file.tests[0].variables[1].name, "time");
        assert_eq!(file.tests[0].variables[2].name, "msg");
        assert_eq!(file.tests[0].variables[2].var_type, Some(VarType::String));
    }

    #[test]
    fn test_parse_duck_typed_variable() {
        let content = r#"===
duck typed
===
echo "val: 42"
---
val: {{ x }}
---
where
* x > 0
"#;
        let file = parse_test(content);
        assert_eq!(file.tests.len(), 1);
        assert_eq!(file.tests[0].variables.len(), 1);
        assert_eq!(file.tests[0].variables[0].name, "x");
        assert_eq!(file.tests[0].variables[0].var_type, None);
    }

    #[test]
    fn test_parse_empty_string_var() {
        let content = r#"===
empty string
===
echo "val: "
---
val: {{ s: string }}
---
where
* len(s) == 0
"#;
        let file = parse_test(content);
        assert_eq!(file.tests.len(), 1);
        assert_eq!(file.tests[0].name, "empty string");
        assert_eq!(file.tests[0].expected_output, "val: {{ s: string }}");
        assert_eq!(file.tests[0].variables.len(), 1);
        assert_eq!(file.tests[0].variables[0].name, "s");
        assert_eq!(file.tests[0].variables[0].var_type, Some(VarType::String));
        assert_eq!(file.tests[0].constraints.len(), 1);
        assert_eq!(file.tests[0].constraints[0], "len(s) == 0");
    }

    #[test]
    fn test_skip_unconditional() {
        let content = r#"===
skipped test
%skip
===
echo hello
---
hello
"#;
        let file = parse_test(content);
        assert_eq!(file.tests.len(), 1);
        let skip = file.tests[0].skip.as_ref().unwrap();
        assert!(skip.message.is_none());
        assert!(skip.condition.is_none());
    }

    #[test]
    fn test_skip_with_message() {
        let content = r#"===
skipped test
%skip(not yet implemented)
===
echo hello
---
hello
"#;
        let file = parse_test(content);
        assert_eq!(file.tests.len(), 1);
        let skip = file.tests[0].skip.as_ref().unwrap();
        assert_eq!(skip.message.as_deref(), Some("not yet implemented"));
        assert!(skip.condition.is_none());
    }

    #[test]
    fn test_skip_with_condition() {
        let content = r#"===
unix only test
%skip if: test "$OS" = "Windows_NT"
===
echo hello
---
hello
"#;
        let file = parse_test(content);
        assert_eq!(file.tests.len(), 1);
        let skip = file.tests[0].skip.as_ref().unwrap();
        assert!(skip.message.is_none());
        assert_eq!(
            skip.condition.as_deref(),
            Some(r#"test "$OS" = "Windows_NT""#)
        );
    }

    #[test]
    fn test_skip_with_message_and_condition() {
        let content = r#"===
unix only test
%skip(requires bash) if: test "$OS" = "Windows_NT"
===
echo hello
---
hello
"#;
        let file = parse_test(content);
        assert_eq!(file.tests.len(), 1);
        let skip = file.tests[0].skip.as_ref().unwrap();
        assert_eq!(skip.message.as_deref(), Some("requires bash"));
        assert_eq!(
            skip.condition.as_deref(),
            Some(r#"test "$OS" = "Windows_NT""#)
        );
    }

    #[test]
    fn test_file_level_skip() {
        let content = r#"%skip(windows tests) if: test "$OS" != "Windows_NT"

===
test 1
===
echo hello
---
hello
"#;
        let file = parse_test(content);
        let file_skip = file.file_skip.as_ref().unwrap();
        assert_eq!(file_skip.message.as_deref(), Some("windows tests"));
        assert_eq!(
            file_skip.condition.as_deref(),
            Some(r#"test "$OS" != "Windows_NT""#)
        );
        assert_eq!(file.tests.len(), 1);
    }

    #[test]
    fn test_file_level_skip_unconditional() {
        let content = r#"%skip(all tests disabled)

===
test 1
===
echo hello
---
hello
"#;
        let file = parse_test(content);
        let file_skip = file.file_skip.as_ref().unwrap();
        assert_eq!(file_skip.message.as_deref(), Some("all tests disabled"));
        assert!(file_skip.condition.is_none());
    }

    #[test]
    fn test_parse_file() {
        let mut f = NamedTempFile::new().unwrap();
        write!(f, "===\ntest\n===\necho hi\n---\nhi\n").unwrap();

        let file = parse_file(f.path()).unwrap();
        assert_eq!(file.tests.len(), 1);
        assert_eq!(file.tests[0].name, "test");
        assert_eq!(file.tests[0].file_path, f.path());
    }

    #[test]
    fn test_multiline_command() {
        let content = r#"===
multiline command
===
echo "line 1"
echo "line 2"
echo "line 3"
---
line 1
line 2
line 3
"#;
        let file = parse_test(content);
        assert_eq!(file.tests.len(), 1);
        assert_eq!(
            file.tests[0].command,
            "echo \"line 1\"\necho \"line 2\"\necho \"line 3\""
        );
    }

    #[test]
    fn test_line_numbers() {
        let content = r#"===
first test
===
echo hello
---
hello

===
second test
===
echo world
---
world
"#;
        let file = parse_test(content);
        assert_eq!(file.tests.len(), 2);
        assert_eq!(file.tests[0].start_line, 1);
        // Just verify we have reasonable line tracking
        assert!(file.tests[0].start_line < file.tests[0].end_line);
        assert!(file.tests[1].start_line < file.tests[1].end_line);
    }

    #[test]
    fn test_longer_delimiters() {
        let content = r#"=====
test with longer delimiters
=====
echo hello
-----
hello
"#;
        let file = parse_test(content);
        assert_eq!(file.tests.len(), 1);
        assert_eq!(file.tests[0].name, "test with longer delimiters");
        assert_eq!(file.tests[0].command, "echo hello");
        assert_eq!(file.tests[0].expected_output, "hello");
    }

    #[test]
    fn test_dash_separator_in_output() {
        let content = r#"====
test with --- in output
====
echo "---"
----
---
"#;
        let file = parse_test(content);
        assert_eq!(file.tests.len(), 1);
        assert_eq!(file.tests[0].expected_output, "---");
    }

    #[test]
    fn test_dash_separators_in_output() {
        // With 5-char delimiters, shorter --- and ---- can appear in output
        // But ----- is the closing delimiter so it terminates the block
        let content = r#"=====
test with various dash separators in output
=====
printf "---\n----\n"
-----
---
----
"#;
        let file = parse_test(content);
        assert_eq!(file.tests.len(), 1);
        assert_eq!(file.tests[0].expected_output, "---\n----");
    }

    #[test]
    fn test_shorter_equals_in_output_is_content() {
        // Shorter === in expected output is treated as content when using longer delimiters
        // Only === of same or longer length signals a new test
        let content = r#"=====
test with === and ==== in output
=====
echo "==="
-----
===

=====
second test
=====
echo "===="
-----
====
"#;
        let file = parse_test(content);
        assert_eq!(file.tests.len(), 2);
        assert_eq!(file.tests[0].expected_output, "===");
        assert_eq!(file.tests[1].expected_output, "====");
    }

    #[test]
    fn test_same_length_equals_ends_block() {
        // === of same length or longer signals new test
        let content = r#"====
first test
====
echo "hello"
----
hello

====
second test same length
====
echo "world"
----
world
"#;
        let file = parse_test(content);
        assert_eq!(file.tests.len(), 2);
        assert_eq!(file.tests[0].expected_output, "hello");
        assert_eq!(file.tests[1].expected_output, "world");
    }

    #[test]
    fn test_longer_delimiters_with_constraints() {
        let content = r#"====
test with constraints
====
echo "count: 42"
----
count: {{ n: number }}
----
where
* n > 0
"#;
        let file = parse_test(content);
        assert_eq!(file.tests.len(), 1);
        assert_eq!(file.tests[0].constraints.len(), 1);
        assert_eq!(file.tests[0].constraints[0], "n > 0");
    }

    #[test]
    fn test_mismatched_header_delimiter_error() {
        let content = r#"====
test name
===
echo hello
---
hello
"#;
        let result = parse_content(content, Path::new("<test>"));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("delimiter length mismatch"),
            "Error should mention delimiter mismatch: {}",
            err
        );
        assert!(
            err.to_string().contains("expected 4") && err.to_string().contains("found 3"),
            "Error should mention expected 4 and found 3: {}",
            err
        );
    }

    #[test]
    fn test_wrong_dash_length_treated_as_content() {
        // With simplified logic, wrong-length delimiters are treated as content
        // This test uses 4-char delimiters but has --- in content
        let content = r#"====
test name
====
echo hello
----
---
hello
"#;
        let file = parse_test(content);
        assert_eq!(file.tests.len(), 1);
        assert_eq!(file.tests[0].expected_output, "---\nhello");
    }

    #[test]
    fn test_multiple_tests_same_delimiter_length() {
        // Multiple tests must use the same delimiter length
        // (or longer delimiter tests can follow shorter ones, but not vice versa)
        let content = r#"===
first test
===
echo "short"
---
short

===
second test
===
echo "world"
---
world
"#;
        let file = parse_test(content);
        assert_eq!(file.tests.len(), 2);
        assert_eq!(file.tests[0].expected_output, "short");
        assert_eq!(file.tests[1].expected_output, "world");
    }

    #[test]
    fn test_all_tests_must_use_same_delimiter_length() {
        // With exact-match logic, all tests in a file must use the same delimiter length
        // A longer delimiter after shorter is treated as content of the first test
        let content = r#"===
first test
===
echo "short"
---
short

=====
this looks like a test but is content
=====
"#;
        let file = parse_test(content);
        assert_eq!(file.tests.len(), 1);
        // The ===== block is included as content
        assert!(file.tests[0].expected_output.contains("====="));
    }

    #[test]
    fn test_shell_directive_file_level_bash() {
        let content = r#"%shell bash

===
test 1
===
echo hello
---
hello
"#;
        let file = parse_test(content);
        assert_eq!(file.file_shell, Some(Shell::Bash));
        assert_eq!(file.tests.len(), 1);
    }

    #[test]
    fn test_shell_directive_file_level_powershell() {
        let content = r#"%shell powershell

===
test 1
===
echo hello
---
hello
"#;
        let file = parse_test(content);
        assert_eq!(file.file_shell, Some(Shell::PowerShell));
    }

    #[test]
    fn test_shell_directive_file_level_sh() {
        let content = r#"%shell sh

===
test 1
===
echo hello
---
hello
"#;
        let file = parse_test(content);
        assert_eq!(file.file_shell, Some(Shell::Sh));
    }

    #[test]
    fn test_shell_directive_file_level_zsh() {
        let content = r#"%shell zsh

===
test 1
===
echo hello
---
hello
"#;
        let file = parse_test(content);
        assert_eq!(file.file_shell, Some(Shell::Zsh));
    }

    #[test]
    fn test_shell_directive_file_level_cmd() {
        let content = r#"%shell cmd

===
test 1
===
echo hello
---
hello
"#;
        let file = parse_test(content);
        assert_eq!(file.file_shell, Some(Shell::Cmd));
    }

    #[test]
    fn test_platform_directive_single() {
        let content = r#"%platform windows

===
test 1
===
echo hello
---
hello
"#;
        let file = parse_test(content);
        assert_eq!(file.file_platform, vec![Platform::Windows]);
    }

    #[test]
    fn test_platform_directive_multiple() {
        let content = r#"%platform linux, macos

===
test 1
===
echo hello
---
hello
"#;
        let file = parse_test(content);
        assert_eq!(file.file_platform, vec![Platform::Linux, Platform::MacOS]);
    }

    #[test]
    fn test_platform_directive_macos() {
        let content = r#"%platform macos

===
test 1
===
echo hello
---
hello
"#;
        let file = parse_test(content);
        assert_eq!(file.file_platform, vec![Platform::MacOS]);
    }

    #[test]
    fn test_all_directives_file_level() {
        let content = r#"%shell bash
%platform unix
%skip(not ready yet)

===
test 1
===
echo hello
---
hello
"#;
        let file = parse_test(content);
        assert_eq!(file.file_shell, Some(Shell::Bash));
        assert_eq!(file.file_platform, vec![Platform::Unix]);
        assert!(file.file_skip.is_some());
        assert_eq!(
            file.file_skip.as_ref().unwrap().message.as_deref(),
            Some("not ready yet")
        );
    }

    #[test]
    fn test_directives_any_order() {
        let content = r#"%platform windows
%skip(windows only)
%shell powershell

===
test 1
===
echo hello
---
hello
"#;
        let file = parse_test(content);
        assert_eq!(file.file_shell, Some(Shell::PowerShell));
        assert_eq!(file.file_platform, vec![Platform::Windows]);
        assert!(file.file_skip.is_some());
    }

    #[test]
    fn test_skip_with_condition_file_level() {
        let content = r#"%skip(needs feature) if: test -f /nonexistent

===
test 1
===
echo hello
---
hello
"#;
        let file = parse_test(content);
        assert!(file.file_skip.is_some());
        let skip = file.file_skip.unwrap();
        assert_eq!(skip.message.as_deref(), Some("needs feature"));
        assert_eq!(skip.condition.as_deref(), Some("test -f /nonexistent"));
    }

    #[test]
    fn test_skip_test_level_with_condition() {
        let content = r#"===
test with skip
%skip(not ready) if: false
===
echo hello
---
hello
"#;
        let file = parse_test(content);
        assert_eq!(file.tests.len(), 1);
        assert!(file.tests[0].skip.is_some());
        let skip = file.tests[0].skip.as_ref().unwrap();
        assert_eq!(skip.message.as_deref(), Some("not ready"));
        assert_eq!(skip.condition.as_deref(), Some("false"));
    }

    #[test]
    fn test_shell_platform_valid_bash_unix() {
        let content = r#"%shell bash
%platform unix

===
test
===
echo hello
---
hello
"#;
        let file = parse_test(content);
        assert_eq!(file.file_shell, Some(Shell::Bash));
        assert_eq!(file.file_platform, vec![Platform::Unix]);
    }

    #[test]
    fn test_shell_platform_valid_powershell_windows() {
        let content = r#"%shell powershell
%platform windows

===
test
===
echo hello
---
hello
"#;
        let file = parse_test(content);
        assert_eq!(file.file_shell, Some(Shell::PowerShell));
        assert_eq!(file.file_platform, vec![Platform::Windows]);
    }

    #[test]
    fn test_shell_platform_invalid_cmd_unix() {
        let content = r#"%shell cmd
%platform unix

===
test
===
echo hello
---
hello
"#;
        let result = parse_content(content, Path::new("<test>"));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("not compatible"));
    }

    #[test]
    fn test_shell_platform_invalid_zsh_windows() {
        let content = r#"%shell zsh
%platform windows

===
test
===
echo hello
---
hello
"#;
        let result = parse_content(content, Path::new("<test>"));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("not compatible"));
    }

    #[test]
    fn test_require_directive() {
        let content = r#"===
required test
%require
===
echo hello
---
hello
"#;
        let file = parse_test(content);
        assert_eq!(file.tests.len(), 1);
        assert!(file.tests[0].require);
    }

    #[test]
    fn test_require_with_skip() {
        let content = r#"===
required and skipped
%require
%skip
===
echo hello
---
hello
"#;
        let file = parse_test(content);
        assert_eq!(file.tests.len(), 1);
        assert!(file.tests[0].require);
        assert!(file.tests[0].skip.is_some());
    }

    #[test]
    fn test_skip_then_require() {
        let content = r#"===
skip then require
%skip
%require
===
echo hello
---
hello
"#;
        let file = parse_test(content);
        assert_eq!(file.tests.len(), 1);
        assert!(file.tests[0].require);
        assert!(file.tests[0].skip.is_some());
    }

    #[test]
    fn test_no_require_by_default() {
        let content = r#"===
normal test
===
echo hello
---
hello
"#;
        let file = parse_test(content);
        assert_eq!(file.tests.len(), 1);
        assert!(!file.tests[0].require);
    }
}
