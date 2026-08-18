use crate::runner::TestResult;
use crate::TestCase;
use std::path::Path;

/// Whether update mode can rewrite this test's expected block.
///
/// Tests that declare variables or constraints hold a pattern, not a literal.
/// Overwriting the pattern with actual output would strip the placeholders
/// while leaving the `where` clauses referring to variables that no longer
/// exist, so these must be updated by hand.
pub fn is_updatable(test: &TestCase) -> bool {
    test.variables.is_empty() && test.constraints.is_empty()
}

/// Rewrite the expected-output blocks of failing tests with their actual output.
/// Returns the number of test cases updated.
///
/// Edits are collected against the original file and applied in reverse line
/// order, so that changing the line count of one block cannot shift the
/// recorded line numbers of the blocks that follow it.
///
/// Only the lines holding expected content are replaced; the blank line that
/// separates one test case from the next lies outside that span and is left
/// untouched.
pub fn update_corpus_file(file_path: &Path, results: &[&TestResult]) -> std::io::Result<usize> {
    let content = std::fs::read_to_string(file_path)?;
    let mut lines: Vec<String> = content.lines().map(str::to_string).collect();

    let mut edits: Vec<(usize, usize, Vec<String>)> = results
        .iter()
        .filter(|r| !r.passed)
        .filter(|r| is_updatable(&r.test))
        .filter_map(|r| {
            let actual = r.actual_output.as_ref()?;
            let start = r.test.expected_start_line.checked_sub(1)?;
            let end = start + r.test.expected_line_count;
            if end > lines.len() {
                return None;
            }
            let replacement = if actual.is_empty() {
                Vec::new()
            } else {
                actual.lines().map(str::to_string).collect()
            };
            Some((start, end, replacement))
        })
        .collect();

    if edits.is_empty() {
        return Ok(0);
    }

    edits.sort_by(|a, b| b.0.cmp(&a.0));

    let updated = edits.len();
    for (start, end, replacement) in edits {
        lines.splice(start..end, replacement);
    }

    std::fs::write(file_path, lines.join("\n") + "\n")?;
    Ok(updated)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_file;
    use crate::runner::TestResult;
    use std::io::Write;
    use std::time::Duration;
    use tempfile::NamedTempFile;

    /// Build results for every test in `content`, pairing each with the actual
    /// output it should be updated to.
    fn results_for(path: &Path, actuals: &[&str]) -> Vec<TestResult> {
        let corpus = parse_file(path).unwrap();
        assert_eq!(
            corpus.tests.len(),
            actuals.len(),
            "expected one actual output per test case"
        );
        corpus
            .tests
            .into_iter()
            .zip(actuals)
            .map(|(test, actual)| TestResult {
                expected_output: test.expected_output.clone(),
                test,
                passed: false,
                skipped: false,
                skip_reason: None,
                actual_output: Some(actual.to_string()),
                error: None,
                warning: None,
                elapsed: Duration::ZERO,
                suite: "t".to_string(),
            })
            .collect()
    }

    fn write_temp(content: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        write!(f, "{}", content).unwrap();
        f.flush().unwrap();
        f
    }

    #[test]
    fn test_update_writes_each_output_to_its_own_case() {
        // Regression: the first replacement changes the line count, which used
        // to shift every later test's recorded line numbers and write output
        // into the wrong case.
        let content = "\
===
alpha
===
cmd_a
---
OLD_A

===
beta
===
cmd_b
---
OLD_B

===
gamma
===
cmd_c
---
OLD_C
";
        let f = write_temp(content);
        let results = results_for(f.path(), &["a1\na2\na3", "b1", "c1"]);
        let refs: Vec<&TestResult> = results.iter().collect();

        assert_eq!(update_corpus_file(f.path(), &refs).unwrap(), 3);

        let updated = parse_file(f.path()).unwrap();
        assert_eq!(updated.tests.len(), 3);
        assert_eq!(updated.tests[0].expected_output, "a1\na2\na3");
        assert_eq!(updated.tests[1].expected_output, "b1");
        assert_eq!(updated.tests[2].expected_output, "c1");
    }

    #[test]
    fn test_update_preserves_blank_separators() {
        let content = "\
===
alpha
===
cmd_a
---
OLD_A

===
beta
===
cmd_b
---
OLD_B
";
        let f = write_temp(content);
        let results = results_for(f.path(), &["A", "B"]);
        let refs: Vec<&TestResult> = results.iter().collect();
        update_corpus_file(f.path(), &refs).unwrap();

        let text = std::fs::read_to_string(f.path()).unwrap();
        assert!(
            text.contains("A\n\n==="),
            "blank separator was not preserved:\n{}",
            text
        );
    }

    #[test]
    fn test_update_skips_tests_with_variables() {
        let content = "\
===
vars
===
cmd
---
count: {{ n: number }}
---
where
* n == 999
";
        let f = write_temp(content);
        let results = results_for(f.path(), &["count: 7"]);
        let refs: Vec<&TestResult> = results.iter().collect();

        assert_eq!(update_corpus_file(f.path(), &refs).unwrap(), 0);

        let text = std::fs::read_to_string(f.path()).unwrap();
        assert!(text.contains("{{ n: number }}"), "pattern was overwritten");
        assert!(text.contains("* n == 999"), "where clause was lost");
    }

    #[test]
    fn test_update_shrinking_output() {
        let content = "\
===
alpha
===
cmd_a
---
one
two
three

===
beta
===
cmd_b
---
OLD_B
";
        let f = write_temp(content);
        let results = results_for(f.path(), &["only", "B"]);
        let refs: Vec<&TestResult> = results.iter().collect();
        update_corpus_file(f.path(), &refs).unwrap();

        let updated = parse_file(f.path()).unwrap();
        assert_eq!(updated.tests[0].expected_output, "only");
        assert_eq!(updated.tests[1].expected_output, "B");
    }

    #[test]
    fn test_update_is_idempotent() {
        let content = "\
===
alpha
===
cmd_a
---
OLD_A

===
beta
===
cmd_b
---
OLD_B
";
        let f = write_temp(content);
        let results = results_for(f.path(), &["a1\na2", "b1"]);
        let refs: Vec<&TestResult> = results.iter().collect();
        update_corpus_file(f.path(), &refs).unwrap();
        let first = std::fs::read_to_string(f.path()).unwrap();

        // Re-running with the same actual output must not change the file.
        let results2 = results_for(f.path(), &["a1\na2", "b1"]);
        let refs2: Vec<&TestResult> = results2.iter().collect();
        update_corpus_file(f.path(), &refs2).unwrap();
        let second = std::fs::read_to_string(f.path()).unwrap();

        assert_eq!(first, second);
    }
}
