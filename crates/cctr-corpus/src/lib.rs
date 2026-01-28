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

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SkipDirective {
    pub message: Option<String>,
    pub condition: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TestCase {
    pub name: String,
    pub command: String,
    pub expected_output: String,
    pub file_path: PathBuf,
    pub start_line: usize,
    pub end_line: usize,
    pub variables: Vec<VariableDecl>,
    pub constraints: Vec<String>,
    pub skip: Option<SkipDirective>,
}

impl TestCase {
    pub fn variable_names(&self) -> Vec<&str> {
        self.variables.iter().map(|v| v.name.as_str()).collect()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CorpusFile {
    pub file_skip: Option<SkipDirective>,
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
        Ok(file) => Ok(file),
        Err(_) => Err(ParseError::Parse {
            line: state.current_line,
            message: "failed to parse corpus file".to_string(),
        }),
    }
}

// ============ Parse State ============

struct ParseState<'a> {
    input: &'a str,
    path: &'a Path,
    current_line: usize,
    delimiter_len: usize,
}

impl<'a> ParseState<'a> {
    fn new(input: &'a str, path: &'a Path) -> Self {
        Self {
            input,
            path,
            current_line: 1,
            delimiter_len: 3,
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

fn header_sep(input: &mut &str) -> ModalResult<()> {
    let line: &str = take_while(1.., '=').parse_next(input)?;
    if line.len() >= 3 {
        Ok(())
    } else {
        Err(winnow::error::ErrMode::Backtrack(ContextError::new()))
    }
}

fn dash_sep(input: &mut &str) -> ModalResult<()> {
    let line: &str = take_while(1.., '-').parse_next(input)?;
    if line.len() >= 3 {
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

fn is_separator_line(line: &str) -> bool {
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

fn try_skip_directive(input: &mut &str) -> ModalResult<Option<SkipDirective>> {
    let _ = take_while(0.., ' ').parse_next(input)?;
    if input.starts_with("%skip") {
        Ok(Some(skip_directive.parse_next(input)?))
    } else {
        Ok(None)
    }
}

// ============ Test Case Parser ============

fn description_line(input: &mut &str) -> ModalResult<String> {
    let content = line_content.parse_next(input)?;
    opt_newline.parse_next(input)?;
    Ok(content.trim().to_string())
}

fn command_lines(input: &mut &str) -> ModalResult<String> {
    let mut lines = Vec::new();

    loop {
        if input.is_empty() {
            break;
        }

        let peek_line = input.lines().next().unwrap_or("");
        if is_separator_line(peek_line) {
            break;
        }

        let line = line_content.parse_next(input)?;
        opt_newline.parse_next(input)?;
        lines.push(line);
    }

    while lines.last().is_some_and(|s| s.trim().is_empty()) {
        lines.pop();
    }

    Ok(lines.join("\n"))
}

fn expected_block(input: &mut &str) -> ModalResult<String> {
    let mut lines = Vec::new();

    loop {
        if input.is_empty() {
            break;
        }

        let peek_line = input.lines().next().unwrap_or("");
        if is_separator_line(peek_line) {
            break;
        }

        let line = line_content.parse_next(input)?;
        opt_newline.parse_next(input)?;
        lines.push(line);
    }

    while lines.last() == Some(&"") {
        lines.pop();
    }

    Ok(lines.join("\n"))
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

fn where_section(input: &mut &str) -> ModalResult<Vec<String>> {
    dash_sep.parse_next(input)?;
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

    header_sep.parse_next(input)?;
    opt_newline.parse_next(input)?;
    state.current_line += 1;

    let name = description_line.parse_next(input)?;
    state.current_line += 1;

    let skip = try_skip_directive.parse_next(input)?;
    if skip.is_some() {
        state.current_line += 1;
    }

    header_sep.parse_next(input)?;
    opt_newline.parse_next(input)?;
    state.current_line += 1;

    let command_start = state.current_line;
    let command = command_lines.parse_next(input)?;
    state.current_line = command_start + command.lines().count().max(1);

    dash_sep.parse_next(input)?;
    opt_newline.parse_next(input)?;
    state.current_line += 1;

    let expected_start = state.current_line;
    let expected_output = expected_block.parse_next(input)?;
    let expected_lines = expected_output.lines().count();
    state.current_line =
        expected_start + expected_lines.max(if expected_output.is_empty() { 0 } else { 1 });

    let constraints = opt(where_section).parse_next(input)?.unwrap_or_default();
    if !constraints.is_empty() {
        state.current_line += 2 + constraints.len();
    }

    skip_blank_lines.parse_next(input)?;

    let end_line = state.current_line;

    let variables = extract_variables_from_expected(&expected_output)
        .map_err(|_| winnow::error::ErrMode::Backtrack(ContextError::new()))?;

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
    })
}

fn corpus_file(state: &mut ParseState) -> Result<CorpusFile, winnow::error::ErrMode<ContextError>> {
    let input = &mut state.input;

    skip_blank_lines.parse_next(input)?;

    let file_skip = try_skip_directive.parse_next(input)?;
    if file_skip.is_some() {
        state.current_line += 1;
    }

    skip_blank_lines.parse_next(input)?;

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

    Ok(CorpusFile { file_skip, tests })
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
}
