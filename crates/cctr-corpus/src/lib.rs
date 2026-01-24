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
//! Types can be specified inline in placeholders: `{{ x }}`, `{{ x: number }}`,
//! `{{ x:string }}`, `{{ x : json object }}`. If no type is given, the type is
//! inferred from the matched value using duck-typing.

use std::path::Path;
use thiserror::Error;
use winnow::combinator::{alt, opt, repeat};
use winnow::error::ContextError;
use winnow::prelude::*;
use winnow::token::{take_till, take_while};

// ============ Data Types ============

/// A segment of a template string - either literal text or a placeholder.
#[derive(Debug, Clone, PartialEq)]
pub enum Segment {
    Literal(String),
    /// Placeholder with name and optional type annotation
    Placeholder {
        name: String,
        var_type: Option<VarType>,
    },
}

/// Variable type for pattern matching.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VarType {
    Number,
    String,
    JsonString,
    JsonBool,
    JsonArray,
    JsonObject,
}

/// A declared variable with name and optional type (None means duck-typed).
#[derive(Debug, Clone, PartialEq)]
pub struct Variable {
    pub name: String,
    pub var_type: Option<VarType>,
}

/// A single test case parsed from a corpus file.
#[derive(Debug, Clone, PartialEq)]
pub struct TestCase {
    pub description: String,
    pub command: Vec<Segment>,
    pub expected: Vec<Segment>,
    pub variables: Vec<Variable>,
    pub constraints: Vec<String>,
    pub start_line: usize,
    pub end_line: usize,
}

#[derive(Error, Debug)]
pub enum ParseError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("parse error at line {line}: {message}")]
    Parse { line: usize, message: String },
    #[error("invalid variable type '{0}' (expected 'number' or 'string')")]
    InvalidVarType(String),
}

// ============ Public API ============

pub fn parse_file(path: &Path) -> Result<Vec<TestCase>, ParseError> {
    let content = std::fs::read_to_string(path)?;
    parse_content(&content)
}

pub fn parse_content(content: &str) -> Result<Vec<TestCase>, ParseError> {
    let mut input = content;
    match test_file.parse_next(&mut input) {
        Ok(tests) => Ok(tests),
        Err(e) => Err(ParseError::Parse {
            line: 1,
            message: format!("{:?}", e),
        }),
    }
}

// ============ Segment Parsing ============

/// Parse a type annotation string into a VarType
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

pub fn parse_segments(input: &str) -> Vec<Segment> {
    let mut result = Vec::new();
    let mut remaining = input;

    while !remaining.is_empty() {
        if let Some(start) = remaining.find("{{") {
            if start > 0 {
                result.push(Segment::Literal(remaining[..start].to_string()));
            }
            if let Some(end) = remaining[start..].find("}}") {
                let content = remaining[start + 2..start + end].trim();
                // Check for inline type annotation: "name : type" or "name: type" or "name :type"
                let (name, var_type) = if let Some(colon_pos) = content.find(':') {
                    let name = content[..colon_pos].trim().to_string();
                    let type_str = content[colon_pos + 1..].trim();
                    (name, parse_type_annotation(type_str))
                } else {
                    (content.to_string(), None)
                };
                result.push(Segment::Placeholder { name, var_type });
                remaining = &remaining[start + end + 2..];
            } else {
                result.push(Segment::Literal(remaining.to_string()));
                break;
            }
        } else {
            if !remaining.is_empty() {
                result.push(Segment::Literal(remaining.to_string()));
            }
            break;
        }
    }

    result
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

fn description_line(input: &mut &str) -> ModalResult<String> {
    let content = line_content.parse_next(input)?;
    opt_newline.parse_next(input)?;
    Ok(content.trim().to_string())
}

fn command_line(input: &mut &str) -> ModalResult<String> {
    let content = line_content.parse_next(input)?;
    opt_newline.parse_next(input)?;
    Ok(content.to_string())
}

fn expected_line<'a>(input: &mut &'a str) -> ModalResult<&'a str> {
    let content = line_content.parse_next(input)?;
    opt_newline.parse_next(input)?;
    Ok(content)
}

fn is_separator_line(line: &str) -> bool {
    let trimmed = line.trim();
    (trimmed.len() >= 3 && trimmed.chars().all(|c| c == '='))
        || (trimmed.len() >= 3 && trimmed.chars().all(|c| c == '-'))
}

fn expected_block(input: &mut &str) -> ModalResult<String> {
    let mut lines = Vec::new();

    loop {
        if input.is_empty() {
            break;
        }

        // Peek at current line to check for separators
        let peek_line = input.lines().next().unwrap_or("");
        if is_separator_line(peek_line) {
            break;
        }

        let line = expected_line.parse_next(input)?;
        lines.push(line);
    }

    // Trim trailing empty lines
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

    // "where" line
    let _ = take_while(0.., ' ').parse_next(input)?;
    "where".parse_next(input)?;
    opt_newline.parse_next(input)?;

    // Constraints
    let constraints: Vec<String> = repeat(0.., constraint_line).parse_next(input)?;

    Ok(constraints)
}

/// Extract variables from segments (placeholders with their optional types)
fn extract_variables(segments: &[Segment]) -> Vec<Variable> {
    let mut seen = std::collections::HashSet::new();
    let mut variables = Vec::new();

    for segment in segments {
        if let Segment::Placeholder { name, var_type } = segment {
            if seen.insert(name.clone()) {
                variables.push(Variable {
                    name: name.clone(),
                    var_type: *var_type,
                });
            }
        }
    }

    variables
}

fn test_case(input: &mut &str) -> ModalResult<TestCase> {
    skip_blank_lines.parse_next(input)?;

    // Opening ===
    header_sep.parse_next(input)?;
    opt_newline.parse_next(input)?;

    // Description
    let description = description_line.parse_next(input)?;

    // Closing ===
    header_sep.parse_next(input)?;
    opt_newline.parse_next(input)?;

    // Command
    let command_str = command_line.parse_next(input)?;

    // ---
    dash_sep.parse_next(input)?;
    opt_newline.parse_next(input)?;

    // Expected output
    let expected_str = expected_block.parse_next(input)?;

    // Optional where section (constraints only, variables extracted from segments)
    let constraints = opt(where_section).parse_next(input)?.unwrap_or_default();

    skip_blank_lines.parse_next(input)?;

    let expected = parse_segments(&expected_str);
    let variables = extract_variables(&expected);

    Ok(TestCase {
        description,
        command: parse_segments(&command_str),
        expected,
        variables,
        constraints,
        start_line: 1, // Would need more work to track accurately
        end_line: 1,
    })
}

fn test_file(input: &mut &str) -> ModalResult<Vec<TestCase>> {
    skip_blank_lines.parse_next(input)?;
    let tests: Vec<TestCase> = repeat(0.., test_case).parse_next(input)?;
    skip_blank_lines.parse_next(input)?;
    Ok(tests)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_segments_simple() {
        let segments = parse_segments("hello world");
        assert_eq!(segments, vec![Segment::Literal("hello world".to_string())]);
    }

    #[test]
    fn test_parse_segments_placeholder() {
        let segments = parse_segments("hello {{ name }}");
        assert_eq!(
            segments,
            vec![
                Segment::Literal("hello ".to_string()),
                Segment::Placeholder {
                    name: "name".to_string(),
                    var_type: None
                },
            ]
        );
    }

    #[test]
    fn test_parse_segments_placeholder_with_type() {
        let segments = parse_segments("count: {{ n: number }}");
        assert_eq!(
            segments,
            vec![
                Segment::Literal("count: ".to_string()),
                Segment::Placeholder {
                    name: "n".to_string(),
                    var_type: Some(VarType::Number)
                },
            ]
        );
    }

    #[test]
    fn test_parse_segments_placeholder_type_variations() {
        // No spaces
        let s1 = parse_segments("{{ x:number }}");
        assert_eq!(
            s1,
            vec![Segment::Placeholder {
                name: "x".to_string(),
                var_type: Some(VarType::Number)
            }]
        );

        // Spaces around colon
        let s2 = parse_segments("{{ x : string }}");
        assert_eq!(
            s2,
            vec![Segment::Placeholder {
                name: "x".to_string(),
                var_type: Some(VarType::String)
            }]
        );

        // Json types
        let s3 = parse_segments("{{ data : json object }}");
        assert_eq!(
            s3,
            vec![Segment::Placeholder {
                name: "data".to_string(),
                var_type: Some(VarType::JsonObject)
            }]
        );
    }

    #[test]
    fn test_parse_segments_multiple() {
        let segments = parse_segments("{{ a }} + {{ b }}");
        assert_eq!(
            segments,
            vec![
                Segment::Placeholder {
                    name: "a".to_string(),
                    var_type: None
                },
                Segment::Literal(" + ".to_string()),
                Segment::Placeholder {
                    name: "b".to_string(),
                    var_type: None
                },
            ]
        );
    }

    #[test]
    fn test_parse_simple_test() {
        let content = r#"===
test name
===
echo hello
---
hello
"#;
        let tests = parse_content(content).unwrap();
        assert_eq!(tests.len(), 1);
        assert_eq!(tests[0].description, "test name");
        assert_eq!(
            tests[0].command,
            vec![Segment::Literal("echo hello".to_string())]
        );
        assert_eq!(
            tests[0].expected,
            vec![Segment::Literal("hello".to_string())]
        );
    }

    #[test]
    fn test_parse_with_inline_types() {
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
        let tests = parse_content(content).unwrap();
        assert_eq!(tests.len(), 1);
        assert_eq!(
            tests[0].expected,
            vec![
                Segment::Literal("Completed in ".to_string()),
                Segment::Placeholder {
                    name: "n".to_string(),
                    var_type: Some(VarType::Number)
                },
                Segment::Literal("s".to_string()),
            ]
        );
        assert_eq!(tests[0].variables.len(), 1);
        assert_eq!(tests[0].variables[0].name, "n");
        assert_eq!(tests[0].variables[0].var_type, Some(VarType::Number));
        assert_eq!(tests[0].constraints, vec!["n > 0", "n < 60"]);
    }

    #[test]
    fn test_parse_without_type_annotation() {
        let content = r#"===
duck typed
===
some_command
---
value: {{ x }}
---
where
* x > 0
"#;
        let tests = parse_content(content).unwrap();
        assert_eq!(tests.len(), 1);
        assert_eq!(tests[0].variables.len(), 1);
        assert_eq!(tests[0].variables[0].name, "x");
        assert_eq!(tests[0].variables[0].var_type, None); // Duck-typed
    }

    #[test]
    fn test_parse_multiple_tests() {
        let content = r#"===
first
===
echo 1
---
1

===
second
===
echo 2
---
2
"#;
        let tests = parse_content(content).unwrap();
        assert_eq!(tests.len(), 2);
        assert_eq!(tests[0].description, "first");
        assert_eq!(tests[1].description, "second");
    }

    #[test]
    fn test_parse_multiline_expected() {
        let content = r#"===
multiline
===
printf "a\nb\nc"
---
a
b
c
"#;
        let tests = parse_content(content).unwrap();
        assert_eq!(tests.len(), 1);
        assert_eq!(
            tests[0].expected,
            vec![Segment::Literal("a\nb\nc".to_string())]
        );
    }

    #[test]
    fn test_parse_empty_expected() {
        let content = r#"===
exit only
===
true
---
"#;
        let tests = parse_content(content).unwrap();
        assert_eq!(tests.len(), 1);
        assert_eq!(tests[0].expected, vec![]);
    }
}
