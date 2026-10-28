# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.32.0](https://github.com/andreasjansson/cctr/releases/tag/v0.32.0) - 2026-10-28

### Added

- `#` comments in the file header, before the first test case.
- A bare `---` with no `where` clause explicitly ends an expected block.
- Constraint failures report the test's location and command, the values of
  the variables the constraint references, the value of each compared
  expression, and the first failing element of a `forall`. Evaluation errors
  report bindings too, which previously only unsatisfied constraints did.
- `-v` pretty-prints large values in full and shows the actual output for
  failures where the output matched but the test still failed.

### Changed

- Test commands run in their own process group, so interrupting a test kills
  everything it spawned.
- Suites that had not started when an interrupt arrived are no longer begun.
- cctr exits non-zero when no tests ran, distinguishing "No tests found" from
  "No tests matched pattern".

### Fixed

- A single SIGTERM or SIGINT delivered more than once, as GNU `timeout` does,
  was treated as a second signal and force-quit without running teardown.
- `-u` could write one test's output into a different test's expected block.
- `-u` stripped the blank lines between test cases.
- `-u` overwrote expected blocks containing variables, breaking their `where`
  clauses. These are now reported for manual update instead.
- A file that could not be parsed was silently treated as having zero tests
  and reported success.
- Parentheses in a `%skip(...)` reason made the whole file unparseable.
- Reported line numbers drifted by one per blank line between test cases.

## Before 0.32.0

Released at some point between 0.1.0 and 0.31.0; this changelog was not kept
up to date for those releases.

### Added

- Adjustable delimiter length: use more `=` or `-` characters (e.g., `====`/`----`) when your content contains the standard 3-character delimiters

## [0.1.0](https://github.com/andreasjansson/cctr/releases/tag/v0.1.0) - 2026-01-07

### Other

- Add libc dependency for SIGPIPE handling
- Handle SIGPIPE gracefully to avoid crash when piped to head/tail
- Fix clippy: collapse if and use is_none_or
- Box TestResult when creating ProgressEvent::TestComplete
- Fix clippy: Box large enum variant
- Fix clippy: use is_none_or and is_some_and instead of map_or
- Fix clippy: use is_some_and instead of map_or
- Show relative paths in test failure output
- Fix suite naming: use directory name when suite is at root
- Clean up stale files, add missing files, track Cargo.lock
- Update run_suite to pass pattern to run_corpus_file
- Filter by test name pattern instead of file name
- Update list_tests signature and remove unused parse_filter
- Pass pattern to run_suite instead of file_filter
- Update main.rs for new CLI: test_root and pattern
- Change CLI: positional TEST_ROOT, -p/--pattern for filtering
- Add test_count() method to Suite
- Count individual tests for skipped suites in summary
- Remove debug regex test
- Remove debug output from matcher
- Add debug output to matcher
- Add test for empty string variable parsing
- Add debug test for regex matching
- Add unit test for empty string matching
- allow empty strings in pattern matching
- Fix fixture detection to only check relative path from root
- Fix test helper to use VarType enum
- Fix VarType pattern matching in extract_values
- Fix VarType pattern matching in build_regex
- Fix matcher to use VarType enum pattern matching
- Add matcher module back to cctr lib.rs
- Add matcher module to cctr using cctr-expr for constraints
- Update cctr dependencies to use cctr-expr and cctr-corpus
- Add cctr-parsers dependency to cctr crate
- Update cctr lib.rs to use cctr-match and cctr-parsers
- Add cctr crate Cargo.toml
