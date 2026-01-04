use crate::error::{Error, Result};
use regex::Regex;
use std::path::Path;
use std::sync::LazyLock;

#[derive(Debug, Clone)]
pub struct TestCase {
    pub name: String,
    pub command: String,
    pub expected_output: String,
    pub file_path: std::path::PathBuf,
    pub start_line: usize,
    pub end_line: usize,
}

static CORPUS_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?ms)^(={3,})\n(.+?)\n\1\n(.+?)\n(-{3,})\n(.*?)(?=\n={3,}\n|\z)",
    )
    .unwrap()
});

pub fn parse_corpus_file(path: &Path) -> Result<Vec<TestCase>> {
    let content = std::fs::read_to_string(path).map_err(|e| Error::ReadCorpus {
        path: path.to_path_buf(),
        source: e,
    })?;

    let mut tests = Vec::new();

    for caps in CORPUS_PATTERN.captures_iter(&content) {
        let name = caps.get(2).unwrap().as_str().trim().to_string();
        let command = caps.get(3).unwrap().as_str().trim().to_string();
        let expected = caps
            .get(5)
            .unwrap()
            .as_str()
            .trim_end_matches('\n')
            .to_string();

        let match_start = caps.get(0).unwrap().start();
        let match_end = caps.get(0).unwrap().end();

        let start_line = content[..match_start].matches('\n').count() + 1;
        let end_line = content[..match_end].matches('\n').count() + 1;

        tests.push(TestCase {
            name,
            command,
            expected_output: expected,
            file_path: path.to_path_buf(),
            start_line,
            end_line,
        });
    }

    Ok(tests)
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
