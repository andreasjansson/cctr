use crate::error::{Error, Result};
use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
pub enum VarType {
    Number,
    String,
}

#[derive(Debug, Clone)]
pub struct VariableDecl {
    pub name: String,
    pub var_type: VarType,
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

fn parse_corpus_content(content: &str, path: &Path) -> Result<Vec<TestCase>> {
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
        let command = lines[i].trim().to_string();

        i += 1;
        if i >= lines.len() || !is_dash_separator(lines[i]) {
            continue;
        }

        // Parse expected output, which ends at:
        // - A second `---` followed by `with` (pattern matching mode)
        // - Next test header `===`
        // - End of file
        i += 1;
        let mut expected_lines = Vec::new();
        let mut variables = Vec::new();
        let mut constraints = Vec::new();

        while i < lines.len() {
            // Check for second `---` that starts with/having section
            if is_dash_separator(lines[i]) {
                let next_idx = i + 1;
                if next_idx < lines.len() && lines[next_idx].trim() == "with" {
                    // Found with/having section
                    i = next_idx + 1;

                    // Parse variable declarations
                    while i < lines.len() {
                        let line = lines[i].trim();
                        if line == "having" || is_header_separator(lines[i]) {
                            break;
                        }
                        if let Some(var) = parse_variable_decl(line) {
                            variables.push(var);
                        }
                        i += 1;
                    }

                    // Parse constraints (if we're at "having")
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

        let end_line = i;

        tests.push(TestCase {
            name,
            command,
            expected_output: expected_lines.join("\n"),
            file_path: path.to_path_buf(),
            start_line,
            end_line,
            variables,
            constraints,
        });
    }

    Ok(tests)
}

fn parse_variable_decl(line: &str) -> Option<VariableDecl> {
    // Parse "* name: type" or "name: type"
    let line = line.trim_start_matches('*').trim();
    let parts: Vec<&str> = line.splitn(2, ':').collect();
    if parts.len() != 2 {
        return None;
    }
    let name = parts[0].trim().to_string();
    let type_str = parts[1].trim().to_lowercase();
    let var_type = match type_str.as_str() {
        "number" => VarType::Number,
        "string" => VarType::String,
        _ => return None,
    };
    Some(VariableDecl { name, var_type })
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
    fn test_parse_with_variables() {
        let mut file = NamedTempFile::new().unwrap();
        write!(
            file,
            r#"===
timing test
===
time_command
---
Completed in {{{{ n }}}}s
---
with
* n: number
"#
        )
        .unwrap();

        let tests = parse_corpus_file(file.path()).unwrap();
        assert_eq!(tests.len(), 1);
        assert_eq!(tests[0].expected_output, "Completed in {{ n }}s");
        assert_eq!(tests[0].variables.len(), 1);
        assert_eq!(tests[0].variables[0].name, "n");
        assert_eq!(tests[0].variables[0].var_type, VarType::Number);
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
Completed in {{{{ n }}}}s
---
with
* n: number
having
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
{{{{ count }}}} items in {{{{ time }}}}s: {{{{ msg }}}}
---
with
* count: number
* time: number
* msg: string
having
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
        assert_eq!(tests[0].variables[2].var_type, VarType::String);
    }
}
