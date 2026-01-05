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

        i += 1;
        let mut expected_lines = Vec::new();
        while i < lines.len() && !is_header_separator(lines[i]) {
            expected_lines.push(lines[i]);
            i += 1;
        }

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
        });
    }

    Ok(tests)
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
}
