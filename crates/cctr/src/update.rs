use crate::runner::TestResult;
use regex::Regex;
use std::path::Path;
use std::sync::LazyLock;

static SEPARATOR_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^-{3,}$").unwrap());
static HEADER_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^={3,}$").unwrap());

pub fn update_corpus_file(file_path: &Path, results: &[&TestResult]) -> std::io::Result<()> {
    let content = std::fs::read_to_string(file_path)?;
    let mut lines: Vec<&str> = content.lines().collect();

    for result in results {
        if result.passed || result.actual_output.is_none() {
            continue;
        }

        let actual = result.actual_output.as_ref().unwrap();
        let test = &result.test;

        let mut expected_start: Option<usize> = None;
        let mut expected_end: Option<usize> = None;
        let mut in_expected = false;

        for (i, line) in lines.iter().enumerate() {
            let line_num = i + 1;
            if line_num < test.start_line {
                continue;
            }
            if line_num > test.end_line + 10 {
                break;
            }

            if SEPARATOR_PATTERN.is_match(line) && expected_start.is_none() {
                expected_start = Some(i + 1);
                in_expected = true;
            } else if in_expected && (HEADER_PATTERN.is_match(line) || i >= lines.len() - 1) {
                expected_end = Some(if HEADER_PATTERN.is_match(line) {
                    i
                } else {
                    i + 1
                });
                break;
            }
        }

        if let (Some(start), Some(end)) = (expected_start, expected_end) {
            let actual_lines: Vec<&str> = actual.lines().collect();
            let mut new_lines: Vec<&str> = lines[..start].to_vec();
            new_lines.extend(actual_lines.iter());

            let needs_blank = end < lines.len() && !lines.get(end - 1).is_none_or(|l| l.is_empty());
            if needs_blank && !actual.is_empty() {
                new_lines.push("");
            }

            new_lines.extend(lines[end..].iter());
            lines = new_lines;
        }
    }

    std::fs::write(file_path, lines.join("\n") + "\n")?;
    Ok(())
}
