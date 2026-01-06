//! Corpus test file parser.
//!
//! Parses `.txt` corpus files into structured test cases.

pub mod expr;

use std::path::Path;
use thiserror::Error;

/// A segment of a template string - either literal text or a placeholder.
#[derive(Debug, Clone, PartialEq)]
pub enum Segment {
    /// Literal text
    Literal(String),
    /// A placeholder like `{{ name }}`
    Placeholder(String),
}

/// Variable type for pattern matching.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VarType {
    Number,
    String,
}

/// A declared variable with name and type.
#[derive(Debug, Clone, PartialEq)]
pub struct Variable {
    pub name: String,
    pub var_type: VarType,
}

/// A single test case parsed from a corpus file.
#[derive(Debug, Clone, PartialEq)]
pub struct TestCase {
    /// Test description/name
    pub description: String,
    /// Command line as segments (literals and placeholders)
    pub command: Vec<Segment>,
    /// Expected output as segments (literals and placeholders)
    pub expected: Vec<Segment>,
    /// Declared pattern variables
    pub variables: Vec<Variable>,
    /// Constraint expressions
    pub constraints: Vec<String>,
    /// Line number where this test starts (1-based)
    pub start_line: usize,
    /// Line number where this test ends (1-based)
    pub end_line: usize,
}

#[derive(Error, Debug)]
pub enum ParseError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Parse error at line {line}: {message}")]
    Parse { line: usize, message: String },
    #[error("Invalid variable type '{0}' (expected 'number' or 'string')")]
    InvalidVarType(String),
}

/// Parse a corpus file into test cases.
pub fn parse_file(path: &Path) -> Result<Vec<TestCase>, ParseError> {
    let content = std::fs::read_to_string(path)?;
    parse_content(&content)
}

/// Parse corpus content string into test cases.
pub fn parse_content(content: &str) -> Result<Vec<TestCase>, ParseError> {
    let mut tests = Vec::new();
    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        if !is_header_separator(lines[i]) {
            i += 1;
            continue;
        }

        let start_line = i + 1; // 1-based
        let header_sep = lines[i];

        // Parse description
        i += 1;
        if i >= lines.len() {
            break;
        }
        let description = lines[i].trim().to_string();

        // Expect matching header separator
        i += 1;
        if i >= lines.len() || lines[i] != header_sep {
            continue;
        }

        // Parse command
        i += 1;
        if i >= lines.len() {
            break;
        }
        let command_str = lines[i].to_string();
        let command = parse_segments(&command_str);

        // Expect dash separator
        i += 1;
        if i >= lines.len() || !is_dash_separator(lines[i]) {
            continue;
        }

        // Parse expected output until second --- or next test or EOF
        i += 1;
        let mut expected_lines = Vec::new();
        let mut variables = Vec::new();
        let mut constraints = Vec::new();

        while i < lines.len() {
            // Check for second `---` that starts with/having section
            if is_dash_separator(lines[i]) {
                let next_idx = i + 1;
                if next_idx < lines.len() && lines[next_idx].trim() == "with" {
                    i = next_idx + 1;

                    // Parse variable declarations
                    while i < lines.len() {
                        let line = lines[i].trim();
                        if line == "having" || is_header_separator(lines[i]) {
                            break;
                        }
                        if let Some(var) = parse_variable_decl(line)? {
                            variables.push(var);
                        }
                        i += 1;
                    }

                    // Parse constraints
                    if i < lines.len() && lines[i].trim() == "having" {
                        i += 1;
                        while i < lines.len() && !is_header_separator(lines[i]) {
                            let line = lines[i].trim();
                            if let Some(constraint) = parse_constraint(line) {
                                constraints.push(constraint);
                            }
                            i += 1;
                        }
                    }
                    break;
                }
            }

            // Check for next test header
            if is_header_separator(lines[i]) {
                break;
            }

            expected_lines.push(lines[i]);
            i += 1;
        }

        // Trim trailing empty lines from expected output
        while expected_lines.last() == Some(&"") {
            expected_lines.pop();
        }

        let expected_str = expected_lines.join("\n");
        let expected = parse_segments(&expected_str);

        let end_line = i;

        tests.push(TestCase {
            description,
            command,
            expected,
            variables,
            constraints,
            start_line,
            end_line,
        });
    }

    Ok(tests)
}

/// Parse a string into segments of literals and placeholders.
pub fn parse_segments(input: &str) -> Vec<Segment> {
    let mut segments = Vec::new();
    let mut remaining = input;

    while !remaining.is_empty() {
        if let Some(start) = remaining.find("{{") {
            // Add literal before the placeholder
            if start > 0 {
                segments.push(Segment::Literal(remaining[..start].to_string()));
            }

            // Find the closing }}
            if let Some(end) = remaining[start..].find("}}") {
                let placeholder_content = &remaining[start + 2..start + end];
                let name = placeholder_content.trim().to_string();
                segments.push(Segment::Placeholder(name));
                remaining = &remaining[start + end + 2..];
            } else {
                // No closing }}, treat rest as literal
                segments.push(Segment::Literal(remaining.to_string()));
                break;
            }
        } else {
            // No more placeholders, rest is literal
            if !remaining.is_empty() {
                segments.push(Segment::Literal(remaining.to_string()));
            }
            break;
        }
    }

    segments
}

fn parse_variable_decl(line: &str) -> Result<Option<Variable>, ParseError> {
    let line = line.trim_start_matches('*').trim();
    if line.is_empty() {
        return Ok(None);
    }

    let parts: Vec<&str> = line.splitn(2, ':').collect();
    if parts.len() != 2 {
        return Ok(None);
    }

    let name = parts[0].trim().to_string();
    let type_str = parts[1].trim().to_lowercase();
    let var_type = match type_str.as_str() {
        "number" => VarType::Number,
        "string" => VarType::String,
        _ => return Err(ParseError::InvalidVarType(type_str)),
    };

    Ok(Some(Variable { name, var_type }))
}

fn parse_constraint(line: &str) -> Option<String> {
    let line = line.trim_start_matches('*').trim();
    if line.is_empty() {
        None
    } else {
        Some(line.to_string())
    }
}

fn is_header_separator(line: &str) -> bool {
    line.len() >= 3 && line.chars().all(|c| c == '=')
}

fn is_dash_separator(line: &str) -> bool {
    line.len() >= 3 && line.chars().all(|c| c == '-')
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
                Segment::Placeholder("name".to_string()),
            ]
        );
    }

    #[test]
    fn test_parse_segments_multiple() {
        let segments = parse_segments("{{ a }} + {{ b }} = {{ c }}");
        assert_eq!(
            segments,
            vec![
                Segment::Placeholder("a".to_string()),
                Segment::Literal(" + ".to_string()),
                Segment::Placeholder("b".to_string()),
                Segment::Literal(" = ".to_string()),
                Segment::Placeholder("c".to_string()),
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
        assert!(tests[0].variables.is_empty());
        assert!(tests[0].constraints.is_empty());
    }

    #[test]
    fn test_parse_with_variables() {
        let content = r#"===
timing test
===
time_command
---
Completed in {{ n }}s
---
with
* n: number
having
* n > 0
* n < 60
"#;
        let tests = parse_content(content).unwrap();
        assert_eq!(tests.len(), 1);
        assert_eq!(
            tests[0].expected,
            vec![
                Segment::Literal("Completed in ".to_string()),
                Segment::Placeholder("n".to_string()),
                Segment::Literal("s".to_string()),
            ]
        );
        assert_eq!(tests[0].variables.len(), 1);
        assert_eq!(tests[0].variables[0].name, "n");
        assert_eq!(tests[0].variables[0].var_type, VarType::Number);
        assert_eq!(tests[0].constraints, vec!["n > 0", "n < 60"]);
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
echo -e "a\nb\nc"
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
}
