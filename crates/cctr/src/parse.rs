use crate::error::{Error, Result};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VarType {
    Number,
    String,
    JsonString,
    JsonBool,
    JsonArray,
    JsonObject,
}

#[derive(Debug, Clone)]
pub struct VariableDecl {
    pub name: String,
    pub var_type: Option<VarType>,
}

#[derive(Debug, Clone)]
pub struct TestCase {
    pub name: String,
    pub command: String,
    pub expected_output: String,
    pub file_path: std::path::PathBuf,
    pub start_line: usize,
    pub end_line: usize,
    pub variables: Vec<VariableDecl>,
    pub constraints: Vec<String>,
}

impl TestCase {
    pub fn variable_names(&self) -> Vec<&str> {
        self.variables.iter().map(|v| v.name.as_str()).collect()
    }
}

pub fn parse_corpus_file(path: &Path) -> Result<Vec<TestCase>> {
    let content = std::fs::read_to_string(path).map_err(|e| Error::ReadCorpus {
        path: path.to_path_buf(),
        source: e,
    })?;

    parse_corpus_content(&content, path)
}

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

/// Parse a placeholder content like "name" or "name: type" or "name : type"
fn parse_placeholder(content: &str) -> (String, Option<VarType>) {
    let content = content.trim();
    if let Some(colon_pos) = content.find(':') {
        let name = content[..colon_pos].trim().to_string();
        let type_str = content[colon_pos + 1..].trim();
        (name, parse_type_annotation(type_str))
    } else {
        (content.to_string(), None)
    }
}

/// Extract variables from expected output by finding {{ ... }} placeholders
fn extract_variables_from_expected(expected: &str) -> Vec<VariableDecl> {
    let mut variables = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut remaining = expected;

    while let Some(start) = remaining.find("{{") {
        if let Some(end) = remaining[start..].find("}}") {
            let content = &remaining[start + 2..start + end];
            let (name, var_type) = parse_placeholder(content);
            if !name.is_empty() && seen.insert(name.clone()) {
                variables.push(VariableDecl { name, var_type });
            }
            remaining = &remaining[start + end + 2..];
        } else {
            break;
        }
    }

    variables
}

pub fn parse_corpus_content(content: &str, path: &Path) -> Result<Vec<TestCase>> {
    let mut tests = Vec::new();
    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        if !is_header_separator(lines[i]) {
            i += 1;
            continue;
        }

        let start_line = i + 1;
        let header_sep = lines[i];

        i += 1;
        if i >= lines.len() {
            break;
        }
        let name = lines[i].trim().to_string();

        i += 1;
        if i >= lines.len() || lines[i] != header_sep {
            continue;
        }

        i += 1;
        if i >= lines.len() {
            break;
        }

        // Collect command lines until we hit the --- separator
        let mut command_lines = Vec::new();
        while i < lines.len() && !is_dash_separator(lines[i]) {
            command_lines.push(lines[i]);
            i += 1;
        }

        if command_lines.is_empty() {
            continue;
        }

        // Trim trailing empty lines from command
        while command_lines
            .last()
            .map(|s| s.trim().is_empty())
            .unwrap_or(false)
        {
            command_lines.pop();
        }

        let command = command_lines.join("\n");

        if i >= lines.len() || !is_dash_separator(lines[i]) {
            continue;
        }

        // Parse expected output, which ends at:
        // - A second `---` followed by `where` (constraints section)
        // - Next test header `===`
        // - End of file
        i += 1;
        let mut expected_lines = Vec::new();
        let mut constraints = Vec::new();

        while i < lines.len() {
            // Check for second `---` that starts where section or ends expected output
            if is_dash_separator(lines[i]) {
                let next_idx = i + 1;
                if next_idx < lines.len() && lines[next_idx].trim() == "where" {
                    // Found where section
                    i = next_idx + 1;

                    // Parse constraints
                    while i < lines.len() && !is_header_separator(lines[i]) {
                        let line = lines[i].trim();
                        if let Some(constraint) = parse_constraint(line) {
                            constraints.push(constraint);
                        }
                        i += 1;
                    }
                }
                // Either way, --- ends the expected output
                break;
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

        let end_line = i;
        let expected_output = expected_lines.join("\n");

        // Extract variables from expected output placeholders
        let variables = extract_variables_from_expected(&expected_output);

        tests.push(TestCase {
            name,
            command,
            expected_output,
            file_path: path.to_path_buf(),
            start_line,
            end_line,
            variables,
            constraints,
        });
    }

    Ok(tests)
}

fn parse_constraint(line: &str) -> Option<String> {
    // Parse "* constraint" or just "constraint"
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
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_parse_single_test() {
        let mut file = NamedTempFile::new().unwrap();
        write!(
            file,
            r#"===
test name
===
echo hello
---
hello
"#
        )
        .unwrap();

        let tests = parse_corpus_file(file.path()).unwrap();
        assert_eq!(tests.len(), 1);
        assert_eq!(tests[0].name, "test name");
        assert_eq!(tests[0].command, "echo hello");
        assert_eq!(tests[0].expected_output, "hello");
        assert!(tests[0].variables.is_empty());
        assert!(tests[0].constraints.is_empty());
    }

    #[test]
    fn test_parse_multiple_tests() {
        let mut file = NamedTempFile::new().unwrap();
        write!(
            file,
            r#"===
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
"#
        )
        .unwrap();

        let tests = parse_corpus_file(file.path()).unwrap();
        assert_eq!(tests.len(), 2);
        assert_eq!(tests[0].name, "first test");
        assert_eq!(tests[1].name, "second test");
    }

    #[test]
    fn test_parse_multiline_output() {
        let mut file = NamedTempFile::new().unwrap();
        write!(
            file,
            r#"===
multiline test
===
echo -e "line1\nline2\nline3"
---
line1
line2
line3
"#
        )
        .unwrap();

        let tests = parse_corpus_file(file.path()).unwrap();
        assert_eq!(tests.len(), 1);
        assert_eq!(tests[0].expected_output, "line1\nline2\nline3");
    }

    #[test]
    fn test_parse_empty_expected() {
        let mut file = NamedTempFile::new().unwrap();
        write!(
            file,
            r#"===
exit only test
===
true
---
"#
        )
        .unwrap();

        let tests = parse_corpus_file(file.path()).unwrap();
        assert_eq!(tests.len(), 1);
        assert_eq!(tests[0].expected_output, "");
    }

    #[test]
    fn test_parse_with_inline_type() {
        let mut file = NamedTempFile::new().unwrap();
        write!(
            file,
            r#"===
timing test
===
time_command
---
Completed in {{{{ n: number }}}}s
"#
        )
        .unwrap();

        let tests = parse_corpus_file(file.path()).unwrap();
        assert_eq!(tests.len(), 1);
        assert_eq!(tests[0].expected_output, "Completed in {{ n: number }}s");
        assert_eq!(tests[0].variables.len(), 1);
        assert_eq!(tests[0].variables[0].name, "n");
        assert_eq!(tests[0].variables[0].var_type, Some(VarType::Number));
    }

    #[test]
    fn test_parse_with_constraints() {
        let mut file = NamedTempFile::new().unwrap();
        write!(
            file,
            r#"===
timing test
===
time_command
---
Completed in {{{{ n: number }}}}s
---
where
* n > 0
* n < 60
"#
        )
        .unwrap();

        let tests = parse_corpus_file(file.path()).unwrap();
        assert_eq!(tests.len(), 1);
        assert_eq!(tests[0].variables.len(), 1);
        assert_eq!(tests[0].constraints.len(), 2);
        assert_eq!(tests[0].constraints[0], "n > 0");
        assert_eq!(tests[0].constraints[1], "n < 60");
    }

    #[test]
    fn test_parse_multiple_variables() {
        let mut file = NamedTempFile::new().unwrap();
        write!(
            file,
            r#"===
multi var test
===
some_command
---
{{{{ count: number }}}} items in {{{{ time: number }}}}s: {{{{ msg: string }}}}
---
where
* count > 0
* time < 10
"#
        )
        .unwrap();

        let tests = parse_corpus_file(file.path()).unwrap();
        assert_eq!(tests.len(), 1);
        assert_eq!(tests[0].variables.len(), 3);
        assert_eq!(tests[0].variables[0].name, "count");
        assert_eq!(tests[0].variables[1].name, "time");
        assert_eq!(tests[0].variables[2].name, "msg");
        assert_eq!(tests[0].variables[2].var_type, Some(VarType::String));
    }

    #[test]
    fn test_parse_duck_typed_variable() {
        let mut file = NamedTempFile::new().unwrap();
        write!(
            file,
            r#"===
duck typed
===
echo "val: 42"
---
val: {{{{ x }}}}
---
where
* x > 0
"#
        )
        .unwrap();

        let tests = parse_corpus_file(file.path()).unwrap();
        assert_eq!(tests.len(), 1);
        assert_eq!(tests[0].variables.len(), 1);
        assert_eq!(tests[0].variables[0].name, "x");
        assert_eq!(tests[0].variables[0].var_type, None); // Duck-typed
    }

    #[test]
    fn test_parse_empty_string_var() {
        let mut file = NamedTempFile::new().unwrap();
        write!(
            file,
            r#"===
empty string
===
echo "val: "
---
val: {{{{ s: string }}}}
---
where
* len(s) == 0
"#
        )
        .unwrap();

        let tests = parse_corpus_file(file.path()).unwrap();
        assert_eq!(tests.len(), 1);
        assert_eq!(tests[0].name, "empty string");
        assert_eq!(tests[0].expected_output, "val: {{ s: string }}");
        assert_eq!(tests[0].variables.len(), 1, "should have 1 variable");
        assert_eq!(tests[0].variables[0].name, "s");
        assert_eq!(tests[0].variables[0].var_type, Some(VarType::String));
        assert_eq!(tests[0].constraints.len(), 1);
        assert_eq!(tests[0].constraints[0], "len(s) == 0");
    }
}
